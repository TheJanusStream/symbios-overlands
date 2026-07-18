//! Sovereign room editor — tabbed Master/Detail view.
//!
//! Rendered only when `session.did == current_room.0` (the signed-in user
//! owns the room they are visiting). Follows the same **Live UX** paradigm
//! as the avatar editor: every widget mutates the live `ResMut<RoomRecord>`
//! in place, so the world recompiles and remote peers mirror the edit the
//! same frame the slider moves — the peer broadcast is driven by the
//! `network::broadcast_room_state` system watching `Res::is_changed`. Three
//! explicit buttons drive persistence and discard flows:
//!
//! - **Save to PDS** publishes the current `RoomRecord` to the owner's PDS
//!   as a slim manifest plus content-addressed child generator records in
//!   one atomic `com.atproto.repo.applyWrites` batch (#697), and syncs the
//!   value into [`StoredRoomRecord`] on success.
//! - **Revert to saved** drops all in-flight edits by copying
//!   [`StoredRoomRecord`] back into the live `RoomRecord`.
//! - **Reset to default** replaces `RoomRecord` with the canonical
//!   `RoomRecord::default_for_did` seed — useful after a botched edit or
//!   when starting from scratch.
//!
//! The editor is intentionally forgiving: any field it doesn't yet expose
//! as a widget still round-trips via the Raw JSON tab, so L-system code,
//! prop mappings, traits, etc. stay editable while the visual UI catches
//! up to the full schema.
//!
//! Sub-module map: each [`EditorTab`] variant has a dedicated file so the
//! mod.rs stays focused on orchestration. [`widgets`] holds the shared
//! egui helpers (sliders, colour pickers, transform editor), plus the
//! ternary-tree L-system preset used when adding a new generator.

pub mod audio;
pub(crate) mod construct;
mod contact_effects;
mod environment;
pub(crate) mod generators;
mod lsystem;
mod material;
mod placements;
mod publish;
mod raw;
mod shape;
mod terrain;
mod widgets;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{self, Placement, RoomRecord};
use crate::state::{
    CurrentRoomDid, LiveInventoryRecord, LiveRoomRecord, LocalPlayer, PublishFeedback,
    PublishStatus, RoomRecordRecovery, StoredRoomRecord,
};
use crate::ui::avatar::AvatarEditorState;
use crate::ui::editable::{
    RecordAction, SeedAction, publish_status_line, save_load_reset_row, seed_row,
};

use publish::spawn_reset_task;
pub(crate) use publish::spawn_room_publish_task;
pub use publish::{PublishRoomTask, poll_publish_tasks};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorTab {
    #[default]
    Environment,
    Generators,
    Placements,
    Effects,
    Raw,
}

/// Stable identifier for one node in the unified generator tree. The pair
/// `(root, path)` walks from a top-level entry in `RoomRecord::generators`
/// (`root` is the HashMap key) into its `children` (`path` carries the
/// child-index chain). An empty `path` denotes the root node itself. The
/// tree-view widget keys its selection / expansion state on this type.
///
/// `Default` returns an `(empty-root, empty-path)` sentinel that stands
/// in for "the implicit virtual root" of the tree-view widget. A real
/// generator never has `root == ""` because every `unique_key` caller
/// passes a non-empty kind-tag/name prefix (`unique_key` itself does not
/// reject empty prefixes); the sentinel is therefore unambiguous.
#[derive(Clone, Default, PartialEq, Eq, Hash, Debug)]
pub struct GenNodeId {
    pub root: String,
    pub path: Vec<usize>,
}

impl GenNodeId {
    pub fn root(name: impl Into<String>) -> Self {
        Self {
            root: name.into(),
            path: Vec::new(),
        }
    }

    pub fn child(root: impl Into<String>, path: Vec<usize>) -> Self {
        Self {
            root: root.into(),
            path,
        }
    }

    /// True when this id is the implicit virtual root that the tree-view
    /// widget uses as the parent of every top-level row. We treat
    /// reparent targets pointing at this sentinel as "drop at top-level".
    pub fn is_virtual_root(&self) -> bool {
        self.root.is_empty() && self.path.is_empty()
    }
}

/// Persistent editor state kept across frames. Promoted to a `Resource` so
/// the 3D gizmo controller in `editor_gizmo` can observe which placement the
/// owner has selected in the UI panel.
#[derive(Resource, Default)]
pub struct RoomEditorState {
    pub selected_tab: EditorTab,
    pub selected_generator: Option<String>,
    pub selected_placement: Option<usize>,
    /// Selected recipe row on the Effects tab's master-detail split
    /// (#825). Not gizmo-coupled, so tab switches leave it alone — the
    /// user's place in the recipe list survives a peek at Environment.
    pub selected_effect: Option<usize>,
    /// Path through the selected named generator's tree to the node the
    /// owner has selected in the unified tree view. An empty `Vec` means the
    /// generator's own root; a `Some([i0, i1, ...])` means the `i_n`-th
    /// child at each depth. `None` means no node is currently selected. The
    /// gizmo controller in `editor_gizmo` reads this pair `(selected_generator,
    /// selected_prim_path)` to attach the 3D gizmo to the live entity that
    /// matches.
    pub selected_prim_path: Option<Vec<usize>>,
    /// State for the [`egui_ltreeview`] widget that drives the Generators
    /// tab's left sidebar. Holds expansion + selection across frames so
    /// resizing / scrolling doesn't reset what the owner had open.
    pub tree_view_state: egui_ltreeview::TreeViewState<GenNodeId>,
    /// Set for one frame by the in-world pick (#719) when a scene click
    /// selects a generator node. On the next Generators-tab draw the tree
    /// grabs keyboard focus so the picked row highlights like a direct
    /// click, then this clears. A world-pick bypasses the tree's own
    /// click-to-focus path, which is what normally focuses it.
    pub pending_tree_focus: bool,
    /// Where the owner's most recent scene-click pick landed (#822).
    /// For a multi-instance node (a scattered blueprint), the gizmo sync
    /// prefers the live instance nearest this position over the
    /// camera-nearest one, so the gizmo appears on the instance the
    /// owner actually clicked — and, because the position (not the
    /// entity id) is stored, the preference survives the record-driven
    /// respawns a drag commit triggers. Identity-gated: sync consults it
    /// only while the selection still matches `generator_ref`/`path`, so
    /// a GUI-originated selection (tree row click) naturally falls back
    /// to camera proximity without anyone having to clear this.
    pub preferred_pick: Option<PreferredPick>,
    raw_text: String,
    raw_text_initialised: bool,
    raw_error: Option<String>,
    /// Seconds remaining before a pending widget change is flushed into
    /// the live `RoomRecord`'s change tick. Dragging a slider resets
    /// this to `MENU_DEBOUNCE_SECS`; the downstream terrain rebuild,
    /// world-compiler pass, and peer `RoomStateUpdate` broadcast fire
    /// exactly once when the timer drains rather than every frame the
    /// slider moves.
    pending_flush_secs: f32,
    /// Active rename modal: `(original_key, draft_key)`. Set when the
    /// owner clicks "Rename" on a generator; cleared when the modal
    /// applies the rename or is dismissed.
    pub renaming_generator: Option<(String, String)>,
    /// Pop-out audio editor state — native working copy + canvas
    /// view-state for the structured node-graph / sequence editor. Held
    /// here so the editor's layout/selection persists across frames and
    /// survives tab switches. See [`audio::AudioEditorState`].
    pub audio_editor: audio::AudioEditorState,
    /// Buffer for the manual re-roll "Random seed" row — defaults to the
    /// owner's DID seed, editable to re-roll the whole room. See
    /// [`crate::ui::editable::seed_row`].
    seed_row_state: crate::ui::editable::SeedRowState,
    /// Pending destructive tree-operation confirmations (#838): root
    /// delete + kind change on the Generators tab.
    tree_confirms: generators::TreeConfirms,
    /// Pending recovery-banner "Reset PDS to default" confirmation
    /// (#840): the button hard-overwrites the stored record, and a
    /// stale banner (pre-#840) could offer it against a healthy one.
    recovery_reset_confirm: crate::ui::confirm::ConfirmState<()>,
    /// Cached seeded-default record, keyed by the DID it was built for (#637).
    /// `RoomRecord::default_for_did` runs the whole procedural pipeline (9
    /// derivers, catalogue builds, a mini-settlement, an ambient-audio recipe),
    /// so build it once per room instead of every frame the editor is open;
    /// invalidated when the keyed DID changes (portal / logout). The third
    /// element is the record's serialized form, pre-baked for the per-frame
    /// `can_reset` comparison (#674).
    default_cache: Option<(String, pds::RoomRecord, Option<serde_json::Value>)>,
    /// Serialized form of [`StoredRoomRecord`] for the per-frame dirty check
    /// (#674). Recomputed only when the stored resource changes (fresh fetch,
    /// publish success, room transition), so an open panel serializes just
    /// the LIVE record each frame instead of live×2 + stored + default.
    /// Keyed by the resource's `last_changed` tick rather than `is_changed()`
    /// — the change flag is consumed even on frames where this system
    /// early-returns (visiting another room, mid-Loading), which would
    /// otherwise leave a stale baseline after a room transition.
    stored_baseline: Option<(bevy::ecs::change_detection::Tick, Option<serde_json::Value>)>,
}

impl RoomEditorState {
    /// True when the user has any row selected — placement, generator
    /// node, or inferred via tab. Used by the cross-editor mutex and the
    /// collapse-deselect logic to decide whether the gizmo should detach.
    pub fn has_selection(&self) -> bool {
        self.selected_placement.is_some() || self.selected_prim_path.is_some()
    }

    /// Drop placement / generator-tree selection. Used when the editor
    /// window is collapsed or when the avatar editor takes the gizmo
    /// over via the cross-editor mutex.
    pub fn clear_selection(&mut self) {
        self.selected_placement = None;
        self.selected_generator = None;
        self.selected_prim_path = None;
        self.tree_view_state.set_selected(Vec::new());
        self.preferred_pick = None;
    }

    /// Snapshot the selection state an undo entry carries (#862) so a
    /// restore (#863) can re-seed it instead of dumping the user to a
    /// full deselect.
    pub(crate) fn undo_selection(&self) -> crate::ui::undo::RoomSelection {
        crate::ui::undo::RoomSelection {
            generator: self.selected_generator.clone(),
            placement: self.selected_placement,
            prim_path: self.selected_prim_path.clone(),
            tree: self.tree_view_state.selected().clone(),
        }
    }

    /// Post-restore fixup (#863): the record was just wholesale-replaced
    /// with an undo/redo snapshot, so every piece of editor state that
    /// referenced the old tree must be re-seeded or dropped — the same
    /// lockstep `reparent.rs` keeps after a structural edit.
    pub(crate) fn restore_from_undo(
        &mut self,
        record: &pds::RoomRecord,
        sel: &crate::ui::undo::RoomSelection,
    ) {
        // Parked confirm payloads (a `GenNodeId`) were resolved against
        // the pre-restore tree and could re-resolve to a different node;
        // drop them rather than let a stale dialog apply to the restored
        // record. Same for a half-typed rename.
        self.tree_confirms.delete.cancel();
        self.tree_confirms.kind.cancel();
        self.recovery_reset_confirm.cancel();
        self.renaming_generator = None;
        // A widget burst still in the debounce was aimed at record state
        // the restore just replaced; letting the timer drain would fire
        // a second `set_changed` and mint a phantom history entry.
        self.pending_flush_secs = 0.0;
        // Refresh the raw-JSON mirror exactly like Load-from-PDS does.
        self.raw_text = serde_json::to_string_pretty(record).unwrap_or_default();
        self.raw_error = None;
        self.raw_text_initialised = true;
        // Selection re-seed, validated against the RESTORED record —
        // whatever no longer resolves demotes to "nothing selected"
        // instead of pointing the gizmo at the wrong node.
        self.preferred_pick = None;
        self.selected_placement = sel.placement.filter(|&idx| idx < record.placements.len());
        let generator_valid = match (&sel.generator, &sel.prim_path) {
            (Some(root), Some(path)) => record
                .generators
                .get(root)
                .is_some_and(|g| crate::ui::undo::restore::node_path_valid(g, path)),
            (Some(root), None) => record.generators.contains_key(root),
            (None, _) => false,
        };
        if generator_valid {
            self.selected_generator = sel.generator.clone();
            self.selected_prim_path = sel.prim_path.clone();
        } else {
            self.selected_generator = None;
            self.selected_prim_path = None;
        }
        let tree: Vec<GenNodeId> = sel
            .tree
            .iter()
            .filter(|id| crate::ui::undo::restore::room_node_id_valid(&record.generators, id))
            .cloned()
            .collect();
        // Reveal what survived: expand every ancestor of each restored
        // row (the tree collapses by default — the scene-pick path in
        // `editor_gizmo` does the same) and arm the one-shot focus so
        // the row highlights like a direct click.
        for id in &tree {
            for depth in 0..id.path.len() {
                self.tree_view_state.set_openness(
                    GenNodeId::child(id.root.clone(), id.path[..depth].to_vec()),
                    true,
                );
            }
        }
        self.pending_tree_focus = !tree.is_empty();
        self.tree_view_state.set_selected(tree);
    }
}

/// A scene-click pick's landing spot (#822): which node was picked and
/// where in the world the picked instance stood. See
/// [`RoomEditorState::preferred_pick`].
#[derive(Clone, Debug)]
pub struct PreferredPick {
    pub generator_ref: String,
    pub path: Vec<usize>,
    pub pos: Vec3,
}

/// Extra system params for [`room_admin_ui`], grouped into one
/// `SystemParam` so the system stays under Bevy's 16-parameter ceiling.
/// The heightmap rides along for the Placements tab's snap-toggle
/// compensation (#700): flipping "Snap to Terrain" rewrites translation.y
/// against the terrain height so the object stays where it renders. The
/// player pose feeds the Environment tab's arrival-point "set to my
/// position & facing" button (#773).
#[derive(SystemParam)]
pub struct RoomEditorExtras<'w, 's> {
    audio_monitor: Res<'w, bevy_symbios_audio::ui::AudioMonitor>,
    audio_requests: MessageWriter<'w, bevy_symbios_audio::ui::MonitorRequest>,
    heightmap: Option<Res<'w, crate::terrain::FinishedHeightMap>>,
    blob_ctx: ResMut<'w, crate::editor_gizmo::BlobEditContext>,
    players: Query<'w, 's, &'static Transform, With<LocalPlayer>>,
    /// Grammar compile outcomes for the forges' status lines (#829).
    grammar_diag: Res<'w, crate::world_builder::grammar_diag::GrammarDiagnostics>,
    /// Managed window geometry (#833) for the World Editor + audio pop-out.
    chrome: crate::ui::layout::WindowChrome<'w>,
    /// Pending Ctrl+S request for the shared save row (#836).
    publish_shortcut: ResMut<'w, crate::ui::shortcuts::PublishShortcut>,
    /// Toast channel for structural-op feedback (#841).
    toasts: ResMut<'w, crate::ui::toast::Toasts>,
    /// Undo history (read: button enabled-state + tooltips) and the
    /// shared request the header buttons stamp (#864).
    undo_history: Res<'w, crate::ui::undo::RoomUndoHistory>,
    undo_shortcut: ResMut<'w, crate::ui::undo::UndoShortcut>,
    /// Label channel for the next undo entry (#865): sites name their
    /// edit; the flush fallback names the tab.
    undo_labels: ResMut<'w, crate::ui::undo::PendingUndoLabels>,
}

#[allow(clippy::too_many_arguments)]
pub fn room_admin_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut room_record: Option<ResMut<LiveRoomRecord>>,
    stored: Option<Res<StoredRoomRecord>>,
    recovery: Option<Res<RoomRecordRecovery>>,
    mut editor: ResMut<RoomEditorState>,
    mut avatar_editor: ResMut<AvatarEditorState>,
    mut gizmo_frame_pref: ResMut<crate::editor_gizmo::GizmoFramePref>,
    mut publish_feedback: ResMut<PublishFeedback<RoomRecord>>,
    mut inventory: Option<ResMut<LiveInventoryRecord>>,
    extras: RoomEditorExtras,
    time: Res<Time>,
) {
    let RoomEditorExtras {
        audio_monitor,
        mut audio_requests,
        heightmap,
        mut blob_ctx,
        players,
        grammar_diag,
        mut chrome,
        mut publish_shortcut,
        mut toasts,
        undo_history,
        mut undo_shortcut,
        mut undo_labels,
    } = extras;
    let (Some(session), Some(refresh_ctx), Some(room_did), Some(record)) =
        (session, refresh_ctx, room_did, room_record.as_mut())
    else {
        return;
    };

    // Security gate — only the owner may edit their own room.
    if session.did != room_did.0 {
        return;
    }

    // Snapshot the owner's current pose for the arrival-point "set to my
    // position" button (#773). Copy-typed, so it threads straight into the
    // egui closure without holding the query borrow.
    let player_pose = players
        .iter()
        .next()
        .map(environment::PlayerPose::from_transform);

    if !editor.raw_text_initialised {
        editor.raw_text = serde_json::to_string_pretty(&record.0)
            .unwrap_or_else(|e| format!("// serialize error: {}", e));
        editor.raw_text_initialised = true;
    }

    // Snapshot pre-frame selection so we can detect (a) "selection just
    // appeared" — the rising edge that clears the avatar editor's
    // selection per the cross-editor mutex, and (b) the collapse-deselect
    // path which fires when the egui Window response reports no inner
    // closure run.
    let prev_room_selected = editor.has_selection();

    // Destructure the Local into independent field borrows so the
    // borrow-checker can see that the tab-body closure and the commit-row
    // closure each touch *disjoint* subsets of the editor state. Without
    // this, re-borrowing `editor` inside nested egui closures trips E0499.
    let RoomEditorState {
        selected_tab,
        selected_generator,
        selected_placement,
        selected_effect,
        selected_prim_path,
        tree_view_state,
        pending_tree_focus,
        raw_text,
        raw_error,
        pending_flush_secs,
        renaming_generator,
        audio_editor,
        seed_row_state,
        tree_confirms,
        recovery_reset_confirm,
        default_cache,
        stored_baseline,
        ..
    } = &mut *editor;

    let ctx = contexts.ctx_mut().unwrap();

    // `ResMut::deref_mut` unconditionally flips the change tick, so any
    // `&mut record.field` access taken while the window is open would mark
    // the resource as changed every frame — which in turn spams peers with
    // `RoomStateUpdate` broadcasts even when nothing was actually edited.
    // Route all UI access through `bypass_change_detection` and call
    // `record.set_changed()` explicitly at the bottom only when a widget or
    // Load/Reset click actually mutated the record.
    let mut widget_change = false;
    let mut needs_broadcast = false;

    {
        let record_mut: &mut RoomRecord = &mut record.bypass_change_detection().0;

        // Rename dialog — the shared modal (#838): keeps itself open on an
        // empty/taken name with the reason inline, Enter applies, Esc
        // cancels. Cloning the `(old, draft)` pair out first lets us mutate
        // the draft in a scratch variable and feed the final decision back
        // into `renaming_generator` without holding a long-lived mutable
        // borrow across the modal's `show` call.
        if let Some((old_name, mut new_name)) = renaming_generator.clone() {
            let outcome = crate::ui::confirm::rename_dialog(
                ctx,
                "Rename Generator",
                &old_name,
                &mut new_name,
                |draft| record_mut.generators.contains_key(draft),
            );
            match outcome {
                crate::ui::confirm::RenameOutcome::Open => {
                    *renaming_generator = Some((old_name, new_name));
                }
                crate::ui::confirm::RenameOutcome::Cancelled => {
                    *renaming_generator = None;
                }
                crate::ui::confirm::RenameOutcome::Renamed(applied) => {
                    if applied != old_name
                        && let Some(g) = record_mut.generators.remove(&old_name)
                    {
                        record_mut.generators.insert(applied.clone(), g);
                        // Rewrite every Placement that referenced the old key
                        // so the world compiler can still resolve its
                        // generator after the rename. Unknown placements
                        // (forward-compat variants) stay untouched because we
                        // can't see their generator_ref field.
                        for p in record_mut.placements.iter_mut() {
                            match p {
                                Placement::Absolute { generator_ref, .. }
                                | Placement::Scatter { generator_ref, .. }
                                | Placement::Grid { generator_ref, .. } => {
                                    if generator_ref == &old_name {
                                        *generator_ref = applied.clone();
                                    }
                                }
                                Placement::Unknown => {}
                            }
                        }
                        // Migrate the traits mapping too — `RoomRecord::traits`
                        // is keyed on generator name, so a rename without this
                        // step orphans ECS trait bindings like
                        // `collider_heightfield` and leaves the renamed
                        // generator with no collision.
                        if let Some(traits) = record_mut.traits.remove(&old_name) {
                            record_mut.traits.insert(applied.clone(), traits);
                        }
                        *selected_generator = Some(applied.clone());
                        undo_labels.set_room(format!("rename of {old_name} to {applied}"));
                        // Tree-view ids are keyed on `(root, path)`, so the
                        // rename also has to retarget the current selection at
                        // the new root key — otherwise the tree highlights
                        // nothing while the gizmo still tracks the renamed
                        // root.
                        tree_view_state.set_one_selected(GenNodeId::root(applied));
                        widget_change = true;
                    }
                    *renaming_generator = None;
                }
            }
        }

        let (pos, size) = chrome.place(crate::ui::layout::UiWindow::WorldEditor, ctx);
        let world_editor_response = egui::Window::new("World Editor")
            .open(&mut panels.world_editor)
            .collapsible(true)
            .resizable(true)
            .default_size(size)
            .default_pos(pos)
            .constrain_to(ctx.available_rect())
            .show(ctx, |ui| {
                // Recovery banner — shown when the stored PDS record failed
                // to decode and we're running on the synthesised default.
                // Offers a one-click "Reset PDS to default" so the owner can
                // deliberately overwrite the incompatible record instead of
                // being stuck.
                if let Some(rec) = recovery.as_deref() {
                    let banner = egui::Frame::new()
                        .fill(crate::ui::theme::current(ui.ctx()).danger_surface)
                        .inner_margin(6.0)
                        .corner_radius(4.0);
                    banner.show(ui, |ui| {
                        ui.colored_label(
                            crate::ui::theme::current(ui.ctx()).danger_surface_text,
                            "⚠ Stored room record is incompatible with this build.",
                        );
                        ui.label(format!("Decode error: {}", rec.reason));
                        ui.label(
                            "You are currently editing the default homeworld. Click below \
                             to overwrite the stored record on your PDS with this default \
                             so the next login loads cleanly.",
                        );
                        // Confirmed reset (#840): this button hard-deletes
                        // and replaces the stored record — never on the
                        // click itself.
                        if ui.button("Reset PDS to default").clicked() {
                            recovery_reset_confirm.request(
                                "Reset your stored world?",
                                "Deletes the room record stored on your PDS and \
                                 replaces it with this default. Whatever the old \
                                 record contained is gone for good.",
                                "Reset PDS record",
                                (),
                            );
                        }
                        if recovery_reset_confirm
                            .show(ui.ctx(), "room-recovery-reset")
                            .is_some()
                        {
                            let default_record = pds::RoomRecord::default_for_did(&room_did.0);
                            *record_mut = default_record.clone();
                            *raw_text =
                                serde_json::to_string_pretty(&default_record).unwrap_or_default();
                            *raw_error = None;
                            needs_broadcast = true;
                            undo_labels.set_room("reset PDS to default");
                            // Use the delete-then-put reset path. The vanilla
                            // putRecord upsert can return 500 when the stored
                            // record is incompatible with the current lexicon;
                            // hard-deleting first sidesteps that failure mode.
                            spawn_reset_task(
                                &mut commands,
                                &session,
                                &refresh_ctx,
                                default_record,
                                room_did.0.clone(),
                                time.elapsed_secs_f64(),
                            );
                            commands.remove_resource::<RoomRecordRecovery>();
                        }
                    });
                    ui.add_space(6.0);
                }

                // Tab bar
                ui.horizontal(|ui| {
                    let tabs = [
                        (EditorTab::Environment, "Environment"),
                        (EditorTab::Generators, "Region Assets"),
                        (EditorTab::Placements, "Placements"),
                        (EditorTab::Effects, "Effects"),
                        (EditorTab::Raw, "Raw JSON"),
                    ];
                    for (tab, label) in tabs {
                        if ui.selectable_label(*selected_tab == tab, label).clicked() {
                            // Refresh the JSON text when the user arrives at
                            // the Raw tab so it reflects any edits made in
                            // the other tabs since the last time it was
                            // viewed.
                            if tab == EditorTab::Raw && *selected_tab != EditorTab::Raw {
                                *raw_text =
                                    serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                                *raw_error = None;
                            }
                            // Drop selections whose tab we're leaving so the
                            // 3D gizmo doesn't linger on an entity the user
                            // can no longer see in the current panel.
                            if tab != *selected_tab {
                                if tab != EditorTab::Placements {
                                    *selected_placement = None;
                                }
                                if tab != EditorTab::Generators {
                                    *selected_generator = None;
                                    *selected_prim_path = None;
                                    tree_view_state.set_selected(Vec::new());
                                }
                            }
                            *selected_tab = tab;
                        }
                    }
                    ui.separator();
                    // Bypassed borrow + explicit tick (#871): the pref is
                    // persisted on change, and a raw ResMut deref here would
                    // re-arm the save debounce every frame the tab bar draws.
                    if crate::editor_gizmo::draw_gizmo_frame_toggle(
                        ui,
                        gizmo_frame_pref.bypass_change_detection(),
                        blob_ctx.selected_element.is_some(),
                    ) {
                        gizmo_frame_pref.set_changed();
                    }
                    ui.separator();
                    crate::ui::undo::undo_redo_buttons(
                        ui,
                        &undo_history,
                        crate::ui::shortcuts::EditorKind::World,
                        &mut undo_shortcut,
                    );
                });
                ui.separator();

                // Reserve room below the tab body for the separator +
                // Publish/Load/Reset row + feedback line; the scroll area
                // then fills the rest of the window so dragging the window
                // taller actually grows the tab body. Without this (and
                // without `auto_shrink = false`) the scroll area collapses
                // to its content and the window height snaps back.
                const FOOTER_RESERVE: f32 = 90.0;
                const BODY_MIN_HEIGHT: f32 = 160.0;
                let body_height = (ui.available_height() - FOOTER_RESERVE).max(BODY_MIN_HEIGHT);

                // The Generators, Placements and Effects tabs paint their
                // own SidePanel + CentralPanel splits (#825), so they
                // manage their own scrolls and bypass the outer ScrollArea
                // that the simpler tabs share. Wrapping a nested SidePanel
                // inside an outer ScrollArea collapses the sidebar's
                // height to zero.
                match *selected_tab {
                    EditorTab::Placements => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            placements::draw_placements_tab(
                                ui,
                                record_mut,
                                selected_placement,
                                heightmap.as_deref(),
                                player_pose,
                                &mut widget_change,
                                &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::World),
                            );
                        });
                    }
                    EditorTab::Effects => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            contact_effects::draw_contact_effects_tab(
                                ui,
                                &mut record_mut.contact_effects,
                                selected_effect,
                                &mut widget_change,
                            );
                        });
                    }
                    EditorTab::Generators => {
                        // Consume the one-shot focus request set by the
                        // in-world pick (#719): read + clear it here so the
                        // tree focuses on exactly the draw that follows a
                        // scene click and never re-focuses on later frames.
                        let request_focus = std::mem::take(pending_tree_focus);
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            let mut tree_source = generators::RoomTreeSource::new(record_mut);
                            generators::draw_generators_tab(
                                ui,
                                &mut tree_source,
                                selected_generator,
                                selected_prim_path,
                                tree_view_state,
                                request_focus,
                                renaming_generator,
                                inventory.as_deref_mut(),
                                audio_editor,
                                &grammar_diag,
                                &mut widget_change,
                                &mut blob_ctx.selected_element,
                                tree_confirms,
                                &mut toasts,
                                time.elapsed_secs_f64(),
                                &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::World),
                            );
                        });
                    }
                    other => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([true, false])
                            .max_height(body_height)
                            .show(ui, |ui| match other {
                                EditorTab::Environment => {
                                    environment::draw_environment_tab(
                                        ui,
                                        &mut record_mut.environment,
                                        &mut record_mut.default_landing,
                                        player_pose,
                                        &mut widget_change,
                                        audio_editor,
                                    );
                                }
                                EditorTab::Raw => {
                                    raw::draw_raw_tab(
                                        ui,
                                        raw_text,
                                        raw_error,
                                        record_mut,
                                        &mut widget_change,
                                        &mut undo_labels
                                            .slot(crate::ui::shortcuts::EditorKind::World),
                                    );
                                }
                                // Generators / Placements / Effects paint
                                // their own split panels in the outer
                                // match arms above.
                                _ => unreachable!(),
                            });
                    }
                }

                ui.separator();

                // Manual re-roll: the same DID-seeded engine that builds
                // the defaults, but with an owner-chosen master seed.
                // Re-rolling replaces the whole working record exactly
                // like "Reset to default" (which is this with seed =
                // fnv1a_64(did)) — clear selections, refresh the raw-JSON
                // mirror, and arm a broadcast/recompile.
                if let SeedAction::Reroll(seed) = seed_row(
                    ui,
                    seed_row_state,
                    crate::seeded_defaults::fnv1a_64(&room_did.0),
                    time.elapsed_secs_f64(),
                    "world",
                ) {
                    *record_mut = pds::RoomRecord::default_for_seed(seed, &room_did.0);
                    *raw_text = serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                    *raw_error = None;
                    *selected_generator = None;
                    *selected_placement = None;
                    *selected_prim_path = None;
                    tree_view_state.set_selected(Vec::new());
                    needs_broadcast = true;
                    undo_labels.set_room(format!("seed re-roll ({seed})"));
                }

                ui.separator();

                // Publish / Revert to saved / Reset to default — the shared
                // row + status line used by every editor (`ui::editable`).
                // `dirty` is *derived* (the live record serialises
                // differently from the stored snapshot) rather than a
                // flag: a failed publish stays dirty and retryable, and an
                // out-of-band edit (the 3D gizmo, an inventory drop) lights
                // the row up with no explicit `mark_dirty` call.
                // Rebuild the seeded default only when the room DID changes,
                // not every frame (#637) — it's a full procedural build.
                let did = &room_did.0;
                if default_cache.as_ref().is_none_or(|(d, _, _)| d != did) {
                    let default_record = pds::RoomRecord::default_for_did(did);
                    let default_value = serde_json::to_value(&default_record).ok();
                    *default_cache = Some((did.clone(), default_record, default_value));
                }
                let (_, default_record, default_value) =
                    default_cache.as_ref().expect("just populated");
                // Both comparison baselines are cached (#674): the stored
                // side re-serializes only when the resource changes and the
                // default side only per DID, so an open panel pays for ONE
                // live-record serialization per frame. The comparisons are
                // value-identical to `records_differ` (Option<Value> both
                // sides, `.ok()` semantics preserved).
                match stored.as_ref() {
                    Some(s)
                        if stored_baseline
                            .as_ref()
                            .is_none_or(|(tick, _)| *tick != s.last_changed()) =>
                    {
                        *stored_baseline =
                            Some((s.last_changed(), serde_json::to_value(&s.0).ok()));
                    }
                    None => *stored_baseline = None,
                    _ => {}
                }
                let live_value = serde_json::to_value(&*record_mut).ok();
                let dirty = match stored_baseline.as_ref() {
                    Some((_, baseline)) => *baseline != live_value,
                    None => true,
                };
                let can_reset = *default_value != live_value;
                // `session` + `refresh_ctx` are guaranteed present (the
                // early return at the top bails otherwise), so the PDS
                // write can always be attempted while dirty.
                // Size readout: the room publishes as a manifest + child
                // generator records (#697), so the per-record budget
                // applies to the largest single record — not the in-memory
                // monolith. Same throttled cache as the other editors.
                let now = time.elapsed_secs_f64();
                if publish_feedback.live_bytes_at.is_none_or(|at| {
                    now - at >= crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS
                }) {
                    publish_feedback.live_bytes = pds::room::max_publish_record_bytes(&*record_mut);
                    publish_feedback.live_bytes_at = Some(now);
                }
                let record_bytes = publish_feedback.live_bytes;
                let ctrl_s = publish_shortcut.take(crate::ui::shortcuts::EditorKind::World);
                match save_load_reset_row(
                    ui,
                    dirty,
                    true,
                    can_reset,
                    record_bytes,
                    ctrl_s,
                    matches!(publish_feedback.status, PublishStatus::Publishing),
                    // Undo covers Revert/Reset here (#866) — no modal.
                    None,
                ) {
                    RecordAction::None => {}
                    RecordAction::Publish => {
                        publish_feedback.status = PublishStatus::Publishing;
                        spawn_room_publish_task(
                            &mut commands,
                            &session,
                            &refresh_ctx,
                            record_mut.clone(),
                            room_did.0.clone(),
                            time.elapsed_secs_f64(),
                        );
                    }
                    RecordAction::Load => {
                        if let Some(stored) = stored.as_ref() {
                            *record_mut = stored.0.clone();
                            *raw_text =
                                serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                            *raw_error = None;
                            *selected_generator = None;
                            *selected_placement = None;
                            *selected_prim_path = None;
                            tree_view_state.set_selected(Vec::new());
                            needs_broadcast = true;
                            undo_labels.set_room("load from PDS");
                        }
                    }
                    RecordAction::Reset => {
                        *record_mut = default_record.clone();
                        *raw_text = serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                        *raw_error = None;
                        *selected_generator = None;
                        *selected_placement = None;
                        *selected_prim_path = None;
                        tree_view_state.set_selected(Vec::new());
                        needs_broadcast = true;
                        undo_labels.set_room("reset to default");
                    }
                }

                publish_status_line(ui, &publish_feedback.status, time.elapsed_secs_f64());
            });

        // Pop-out audio editor — a top-level Window sibling to the World
        // Editor so its node canvas has room to pan/zoom. Slot-agnostic:
        // it edits a native working copy and stages the committed result
        // in `audio_editor.committed`, which the matching slot's bridge
        // (room-ambient here, per-construct in the Generators tab) picks
        // up on its next frame and writes into the live record.
        audio::draw_audio_editor_window(
            ctx,
            audio_editor,
            &audio_monitor,
            &mut audio_requests,
            &mut chrome,
        );

        if let Some(response) = world_editor_response.as_ref() {
            chrome.remember(
                crate::ui::layout::UiWindow::WorldEditor,
                response.response.rect,
            );
        }

        // `Window::show` returns `Some(InnerResponse { inner: None, .. })`
        // when the window is rendered but collapsed (the closure does
        // not fire). `Some(InnerResponse { inner: Some(_), .. })` means
        // the body ran. `None` means the window is closed entirely.
        // Treat collapsed and closed identically: the user can no
        // longer see the selection in the panel, so the gizmo should
        // detach.
        let body_visible = world_editor_response
            .as_ref()
            .is_some_and(|r| r.inner.is_some());
        if !body_visible {
            *selected_placement = None;
            *selected_generator = None;
            *selected_prim_path = None;
            tree_view_state.set_selected(Vec::new());
        }
    }

    // Cross-editor mutex: when this frame's room selection rose from
    // None → Some, drop the avatar editor's visuals selection so only
    // one gizmo is attached at a time. The reverse direction is
    // enforced by the analogous block in `avatar::avatar_ui`. Read
    // selection state via the destructured fields — `editor` is still
    // mutably borrowed until end of function.
    let now_room_selected = selected_placement.is_some() || selected_prim_path.is_some();
    if now_room_selected && !prev_room_selected && avatar_editor.has_visuals_selection() {
        avatar_editor.clear_visuals_selection();
    }

    // A widget edit only arms the broadcast/recompile debounce now —
    // the Publish/Load row's dirty state is derived from
    // `records_differ`, so there is no flag to set here.
    if widget_change {
        *pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
        // Coarse per-tab undo label (#865) — only when no site named the
        // edit specifically this burst (latest-wins would otherwise let
        // the generic name clobber "delete of oak_3").
        if !undo_labels.room_pending() {
            undo_labels.set_room(match *selected_tab {
                EditorTab::Environment => "environment edit",
                EditorTab::Generators => "asset edit",
                EditorTab::Placements => "placement edit",
                EditorTab::Effects => "effects edit",
                EditorTab::Raw => "raw JSON edit",
            });
        }
    }
    // Drain the debounce timer and flip `needs_broadcast` on the frame it
    // reaches zero. A slider drag keeps resetting `pending_flush_secs`
    // above, so the flush only fires once the user pauses — collapsing a
    // ~60 Hz storm of RoomStateUpdate broadcasts and terrain rebuilds into
    // one event per edit burst.
    if *pending_flush_secs > 0.0 {
        *pending_flush_secs = (*pending_flush_secs - time.delta_secs()).max(0.0);
        if *pending_flush_secs <= 0.0 {
            needs_broadcast = true;
        }
    }
    if needs_broadcast {
        // Explicit Load / Reset / recovery clicks land here too; zero the
        // timer so a concurrently-debounced slider flush cannot double-fire
        // set_changed() on the very next frame.
        *pending_flush_secs = 0.0;
        // Clamp through the same bounds the network-ingress path enforces
        // before the world compiler sees the tick — egui's DragValue
        // parses typed `NaN`/`inf` and its range clamp passes NaN
        // through, so a widget edit can otherwise carry NaN into mesh /
        // collider construction. The raw-JSON tab already sanitizes on
        // parse; this covers the visual-tab widgets.
        record.bypass_change_detection().0.sanitize();
        record.set_changed();
    }
}
