//! Sovereign room editor — tabbed Master/Detail view.
//!
//! Rendered only when `session.did == current_room.0` (the signed-in user
//! owns the room they are visiting). Follows the same **Live UX** paradigm
//! as the avatar editor: every widget mutates the live `ResMut<RoomRecord>`
//! in place.
//!
//! **How fast an edit reaches the world (#1249 f59).** This header used to
//! say "the same frame the slider moves", and that has not been true since
//! the debounce: a widget edit re-arms a 0.25 s timer, and `set_changed()`
//! — which is what `network::broadcast_room_state`, the world compile and
//! the terrain rebuild all watch — fires only when it drains. So an edit
//! burst is one broadcast and one recompile, and a slider being dragged
//! showed nothing at all until the hand stopped. There are two lanes now:
//! the expensive consumers keep the debounce, and
//! [`crate::world_builder::compile::EnvironmentPreview`] is stamped every
//! frame a widget changes so the atmosphere — light, fog, sky, cloud
//! uniforms — follows the drag. Three explicit buttons drive persistence
//! and discard flows:
//!
//! - **Save** publishes the current `RoomRecord` to the owner's PDS
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

pub(crate) mod assets;
pub mod audio;
pub(crate) mod caps;
pub(crate) mod construct;
mod contact_effects;
mod environment;
pub(crate) mod generators;
mod lsystem;
mod material;
mod placements;
mod publish;
pub(crate) use publish::stale_result;
mod raw;
mod shape;
mod terrain;
pub(crate) mod widgets;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{self, RoomRecord};
use crate::state::{
    CurrentRoomDid, LiveInventoryRecord, LiveRoomRecord, LocalPlayer, PublishFeedback,
    PublishStatus, RoomRecordRecovery, StoredRoomRecord,
};
use crate::ui::avatar::AvatarEditorState;
use crate::ui::editable::{
    RecordAction, SeedAction, pin_axis_row, publish_status_line, save_load_reset_row, seed_row,
};
use crate::ui::room::generators::GeneratorTreeSource;

use publish::spawn_reset_task;
pub(crate) use publish::spawn_room_publish_task;
pub use publish::{PublishRoomTask, ResetRoomTask, poll_publish_tasks};

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
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

/// Display order for the Placements list (#1244 f414).
///
/// Insertion order is no index at all once a seeded settlement has handed
/// the owner several hundred machine-authored rows interleaved with their
/// own — and the record's `Vec` order was the ONLY order available.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PlacementSort {
    /// The record's own order, which is also the index shown on each row.
    #[default]
    Order,
    /// Alphabetical by the generator each placement points at — the
    /// question "where are this asset's placements" answered by grouping.
    Generator,
    /// Absolute / Scatter / Grid together.
    Kind,
}

impl PlacementSort {
    pub const ALL: [Self; 3] = [Self::Order, Self::Generator, Self::Kind];

    pub fn label(self) -> &'static str {
        match self {
            Self::Order => "Order",
            Self::Generator => "Item",
            Self::Kind => "Kind",
        }
    }
}

/// Persistent editor state kept across frames. Promoted to a `Resource` so
/// the 3D gizmo controller in `editor_gizmo` can observe which placement the
/// owner has selected in the UI panel.
#[derive(Resource, Default)]
pub struct RoomEditorState {
    pub selected_tab: EditorTab,
    /// The Generators tab's tree: the selected row, the widget's expansion
    /// state, the one-shot focus request, the rename modal and the parked
    /// destructive confirms (#1161). Six loose fields here until the avatar
    /// editor's two trees were given the same struct to own.
    pub(crate) tree: generators::TreePanelState,
    pub selected_placement: Option<usize>,
    /// Additional selected placement rows (#1244 f415) — `selected_placement`
    /// stays the ANCHOR (the gizmo target, the detail panel's subject) and
    /// this carries the rest of a shift/ctrl-extended range. Kept as a
    /// sidecar rather than widening the anchor to a `Vec`, because
    /// `editor_gizmo`, the highlight and `draw_placement_visualizers` all
    /// read the anchor and need no change at all.
    pub extra_placements: Vec<usize>,
    /// Substring filter over the placement list's row labels (#1244 f414).
    pub placement_filter: String,
    /// Display order for the placement list (#1244 f414). Sorts a vector
    /// of INDICES; `record.placements` is never reordered, because the
    /// index is the placement's identity everywhere else in the editor.
    pub placement_sort: PlacementSort,
    /// Substring filter over the generator tree's root names (#1244 f414).
    pub generator_filter: String,
    /// The editor's one-node clipboard (#1244 f422): a deep copy of a
    /// tree node, so a sub-assembly can cross roots without consuming one
    /// of the inventory's 50 slots and round-tripping through another
    /// window. Session-scoped by being editor state.
    pub node_clipboard: Option<crate::pds::Generator>,
    /// Selected recipe row on the Effects tab's master-detail split
    /// (#825). Not gizmo-coupled, so tab switches leave it alone — the
    /// user's place in the recipe list survives a peek at Environment.
    pub selected_effect: Option<usize>,
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
    /// The Raw JSON tab's text, and what it was seeded from (#1212).
    raw: raw::RawJsonBuffer,
    /// Seconds remaining before a pending widget change is flushed into
    /// the live `RoomRecord`'s change tick. Dragging a slider resets
    /// this to `MENU_DEBOUNCE_SECS`; the downstream terrain rebuild,
    /// world-compiler pass, and peer `RoomStateUpdate` broadcast fire
    /// exactly once when the timer drains rather than every frame the
    /// slider moves.
    pending_flush_secs: f32,
    /// Pop-out audio editor state — native working copy + canvas
    /// view-state for the structured node-graph / sequence editor. Held
    /// here so the editor's layout/selection persists across frames and
    /// survives tab switches. See [`audio::AudioEditorState`].
    pub audio_editor: audio::AudioEditorState,
    /// The manual re-roll block (#1005): the seed row's buffer, the pinned
    /// axes and the memoized hunt over the two. See
    /// [`crate::ui::editable::ReRollState`].
    reroll: crate::ui::editable::ReRollState<crate::seeded_defaults::ScenePins>,
    /// Pending recovery-banner "Reset PDS to default" confirmation
    /// (#840): the button hard-overwrites the stored record, and a
    /// stale banner (pre-#840) could offer it against a healthy one.
    recovery_reset_confirm: crate::ui::confirm::ConfirmState<()>,
    /// The bulk placement delete's confirmation (#1244 f415), carrying the
    /// rows it will remove. Behind the shared modal like every other
    /// destructive path, and it names N.
    placement_bulk_delete: crate::ui::confirm::ConfirmState<Vec<usize>>,
    /// Pending publish-after-unrecoverable-fetch confirmation (#1199):
    /// while [`RoomRecordRecovery`] is present the editor holds the
    /// default and Save (or Ctrl+S) would overwrite the real stored
    /// record — the Avatar editor asked first since #840; the room did
    /// not.
    publish_guard: crate::ui::confirm::ConfirmState<()>,
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
    /// Serialized form of the LIVE record, rebuilt only when the record
    /// could have changed (#1270 f418). #674 cached both comparison
    /// BASELINES and left this one running per frame, saying so out loud:
    /// "an open panel pays for ONE live-record serialization per frame".
    /// At the record's own caps — 256 generators of up to 1024 nodes, with
    /// 16 KiB of L-system and 16 KiB of shape source each — that one is a
    /// multi-megabyte `Value` tree allocated, deep-compared and dropped
    /// sixty times a second, and it gets worse the more the owner has
    /// built. See [`crate::ui::perf::LiveValueCache`] for why the key is a
    /// tick AND a flag: this editor writes through
    /// `bypass_change_detection` on purpose, so the tick alone would miss
    /// a slider drag entirely.
    live_baseline: crate::ui::perf::LiveValueCache,
    /// [`Self::live_baseline`]'s rebuild count as of the last size
    /// measurement, so the two 0.5 s readouts below can skip a record that
    /// has not changed since they last looked (#1270 f418). Each of them
    /// is a whole-record encode — `max_publish_record_bytes` serializes
    /// the manifest and every one of the 256 generators; the live-sync
    /// gauge bincodes the entire room.
    size_readout_generation: Option<u64>,
    /// Serialized size of the whole-room live-sync broadcast, refreshed on
    /// the same throttle as the per-record gauge (#1123). Cached rather
    /// than measured per frame because it costs a full bincode encode of
    /// the record — the same encode the broadcaster pays.
    live_sync_bytes: Option<usize>,
}

impl RoomEditorState {
    /// True when the user has any row selected — placement, generator
    /// node, or inferred via tab. Used by the cross-editor mutex and the
    /// collapse-deselect logic to decide whether the gizmo should detach.
    pub fn has_selection(&self) -> bool {
        self.selected_placement.is_some() || self.tree.selection.path.is_some()
    }

    /// The placement the editor is SHOWING selected: the anchor row, and
    /// only while the Placements tab is up (#1297). A selection parked
    /// behind another tab is state, not focus — the gizmo and the
    /// highlight follow the anchor regardless, the world outline does
    /// not. Mirrored out to [`crate::world_builder::PlacementFocus`] by
    /// [`mirror_placement_focus`].
    pub fn focused_placement(&self) -> Option<usize> {
        (self.selected_tab == EditorTab::Placements)
            .then_some(self.selected_placement)
            .flatten()
    }

    /// Drop placement / generator-tree selection. Used when the editor
    /// window is collapsed or when the avatar editor takes the gizmo
    /// over via the cross-editor mutex.
    pub fn clear_selection(&mut self) {
        self.selected_placement = None;
        self.extra_placements.clear();
        self.tree.selection.clear();
        self.tree.view.set_selected(Vec::new());
        self.preferred_pick = None;
    }

    /// Every selected placement index, anchor first (#1244 f415).
    pub fn selected_placements(&self) -> Vec<usize> {
        let mut all: Vec<usize> = self.selected_placement.into_iter().collect();
        all.extend(self.extra_placements.iter().copied());
        all
    }

    /// Snapshot the selection state an undo entry carries (#862) so a
    /// restore (#863) can re-seed it instead of dumping the user to a
    /// full deselect.
    pub(crate) fn undo_selection(&self) -> crate::ui::undo::RoomSelection {
        crate::ui::undo::RoomSelection {
            generator: self.tree.selection.root.clone(),
            placement: self.selected_placement,
            prim_path: self.tree.selection.path.clone(),
            tree: self.tree.view.selected().clone(),
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
        self.tree.confirms.cancel_all();
        self.recovery_reset_confirm.cancel();
        self.placement_bulk_delete.cancel();
        self.publish_guard.cancel();
        self.tree.renaming = None;
        // A widget burst still in the debounce was aimed at record state
        // the restore just replaced; letting the timer drain would fire
        // a second `set_changed` and mint a phantom history entry.
        self.pending_flush_secs = 0.0;
        // Refresh the raw-JSON mirror exactly like Load-from-PDS does —
        // unless it holds unparsed edits, which are kept and flagged (#1212).
        self.raw.sync_to(record);
        // Selection re-seed, validated against the RESTORED record —
        // whatever no longer resolves demotes to "nothing selected"
        // instead of pointing the gizmo at the wrong node.
        self.preferred_pick = None;
        self.selected_placement = sel.placement.filter(|&idx| idx < record.placements.len());
        // The sidecar multi-selection (#1244 f415) is not carried in an
        // undo entry: a restore renumbers the placement vector, so every
        // extra row would point somewhere arbitrary. The anchor above is
        // validated against the restored record; the rest goes.
        self.extra_placements.clear();
        let generator_valid = match (&sel.generator, &sel.prim_path) {
            (Some(root), Some(path)) => record
                .generators
                .get(root)
                .is_some_and(|g| crate::ui::undo::restore::node_path_valid(g, path)),
            (Some(root), None) => record.generators.contains_key(root),
            (None, _) => false,
        };
        if generator_valid {
            self.tree.selection.root = sel.generator.clone();
            self.tree.selection.path = sel.prim_path.clone();
        } else {
            self.tree.selection.clear();
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
                self.tree.view.set_openness(
                    GenNodeId::child(id.root.clone(), id.path[..depth].to_vec()),
                    true,
                );
            }
        }
        self.tree.pending_focus = !tree.is_empty();
        self.tree.view.set_selected(tree);
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
    /// Live road-network stats for the RoadNetwork detail readout (#888).
    road_stats: Res<'w, crate::terrain::RoadPanelStats>,
    /// The last compile abandoned part of the placement queue at the
    /// entity budget (#1211) — the footer says so.
    compile_truncation: Option<Res<'w, crate::world_builder::WorldCompileTruncated>>,
    /// Managed window geometry (#833) for the World Editor + audio pop-out.
    chrome: crate::ui::layout::WindowChrome<'w>,
    /// Pending Ctrl+S request for the shared save row (#836).
    publish_shortcut: ResMut<'w, crate::ui::shortcuts::PublishShortcut>,
    /// Toast channel for structural-op feedback (#841).
    toasts: ResMut<'w, crate::notify::Toasts>,
    /// Undo history (read: button enabled-state + tooltips) and the
    /// shared request the header buttons stamp (#864).
    undo_history: Res<'w, crate::ui::undo::RoomUndoHistory>,
    undo_shortcut: ResMut<'w, crate::ui::undo::UndoShortcut>,
    /// Label channel for the next undo entry (#865): sites name their
    /// edit; the flush fallback names the tab.
    undo_labels: ResMut<'w, crate::ui::undo::PendingUndoLabels>,
    /// Where the gizmo host is, and the channel that walks the player
    /// there (#1244 f148) — the "Go to" button's two halves.
    gizmo_focus: Res<'w, crate::editor_gizmo::GizmoFocus>,
    player_move: ResMut<'w, crate::player::PlayerMoveRequest>,
    /// Click-to-pick face selection (#961): shared with the scene click
    /// handler that resolves what the Faces panel armed.
    face_pick: ResMut<'w, crate::editor_gizmo::FacePick>,
    /// The four asset caches + the retry channel (#1246): every field that
    /// names a fetched image or sound says what happened to it.
    asset_caches: assets::AssetCaches<'w>,
    /// The app-wide mute (#1252 f303): the Effects tab says so, because a
    /// launch-fresh session is muted by default and an owner's first
    /// correct cue is otherwise indistinguishable from four kinds of
    /// broken.
    audio_muted: ResMut<'w, crate::audio_mute::AudioMuted>,
    /// Who is in the world right now (#1269 f293). Every slider move is
    /// broadcast to all of them the frame it happens, and the only place
    /// that was ever said was a hover on a warning label that renders at
    /// 75% of the peer-sync ceiling — i.e. never, in the ordinary case.
    peers: Query<'w, 's, (), With<crate::state::RemotePeer>>,
    /// The in-flight terrain rebuild (#1249 f63). Present for as long as
    /// the async heightmap job runs, which for a big grid with erosion on
    /// is several seconds during which the World Editor said nothing at
    /// all and the old mesh stayed on screen.
    terrain_task: Option<Res<'w, crate::terrain::TerrainTask>>,
}

/// The footer line while a terrain rebuild is in flight (#1249 f63).
///
/// A terrain-config change tears the heightmap down and dispatches an async
/// regeneration, keeping the old mesh up as `OutgoingTerrain` until the new
/// one lands. The knobs make that job arbitrarily expensive — grid size to
/// 2048, erosion drops to 500 000 — and the only completion signal was a
/// session-log event. A knob whose effect is deferred by seconds with no
/// acknowledgement reads as broken, and the owner drags it again, which
/// re-queues the job.
///
/// Pure so the wording and the arithmetic are testable without egui: the
/// elapsed seconds come from the task's own dispatch stamp.
fn terrain_rebuild_line(dispatched_at: f64, now: f64) -> String {
    let elapsed = (now - dispatched_at).max(0.0);
    format!("⟳ Rebuilding the landscape… {elapsed:.0}s")
}

/// How full the room is (#1210, finding 413): the two hard caps every add
/// is refused at, as `N/cap`, warn-tinted from 80 % and error-tinted at
/// the cap — the Inventory's treatment. And how much of the placement
/// list the road layer grew (#1211, finding 394): a budget warning the
/// owner cannot attribute to their own handful of objects is worse than
/// none.
fn counts_line(ui: &mut egui::Ui, record: &RoomRecord) {
    use caps::Cap;
    let (generators, g_tone) = Cap::Generators.readout(record.generators.len());
    let (placements, p_tone) = Cap::Placements.readout(record.placements.len());
    let grown = record
        .placements
        .iter()
        .filter(|p| crate::terrain::is_road_grown(p))
        .count();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(generators)
                .small()
                .color(caps::tone_color(ui, g_tone)),
        )
        .on_hover_text(format!(
            "Top-level items in this world. {}",
            Cap::Generators.full_reason()
        ));
        ui.label(egui::RichText::new("·").small().weak());
        ui.label(
            egui::RichText::new(placements)
                .small()
                .color(caps::tone_color(ui, p_tone)),
        )
        .on_hover_text(format!(
            "Placements in this world. {}",
            Cap::Placements.full_reason()
        ));
        if grown > 0 {
            ui.label(egui::RichText::new("·").small().weak());
            ui.label(
                egui::RichText::new(format!("{grown} grown by the road layer"))
                    .small()
                    .weak(),
            )
            .on_hover_text(
                "Buildings and street furniture the road network planted. They count \
                 against the placement cap and the record size like anything else; the \
                 Lots controls on the RoadNetwork part (Items) tune how many.",
            );
        }
    });
}

/// The sentence for a compile that hit the entity budget (#1211), shared by
/// the one-shot toast and the footer line.
pub(crate) fn compile_truncated_text(t: &crate::world_builder::WorldCompileTruncated) -> String {
    let from = t
        .first_skipped_index
        .map(|i| format!(" (from placement #{i})"))
        .unwrap_or_default();
    format!(
        "{} This world is too dense to build — {} placement{} skipped past the {} object \
         limit{from}",
        crate::ui::affordances::CROSS,
        t.skipped_placements,
        if t.skipped_placements == 1 { "" } else { "s" },
        crate::world_builder::compile::MAX_ROOM_ENTITIES,
    )
}

/// Publish [`crate::world_builder::PlacementFocus`] from this frame's
/// editor state and access gate (#1297 step 4) — the ONE writer of that
/// resource, and the `ui::avatar::mirror_rig_hold` shape a second time.
///
/// The placement visualiser used to read `RoomEditorState` and
/// [`crate::ui::toolbar::RoomEditAccess`] itself, which pointed the
/// dependency arrow from the world pipeline into the egui layer. It read
/// one predicate ([`RoomEditorState::focused_placement`]) behind one gate
/// ([`crate::ui::toolbar::RoomEditAccess::can_edit_room`], #1237 f142),
/// so that is what crosses; the gate's reasoning stays with the gate.
///
/// `PreUpdate`, unconditionally: a mirror inside `room_admin_ui` would
/// draw only while the window is open and latch at its last value the
/// moment it closed. With no editor state, or no access param at all
/// (before login, the headless render tool), the answer is `None`.
pub fn mirror_placement_focus(
    editor: Option<Res<RoomEditorState>>,
    access: Option<crate::ui::toolbar::RoomEditAccess>,
    mut focus: ResMut<crate::world_builder::PlacementFocus>,
) {
    let selected = match (editor.as_deref(), access.as_ref()) {
        (Some(editor), Some(access)) if access.can_edit_room() => editor.focused_placement(),
        _ => None,
    };
    let next = crate::world_builder::PlacementFocus { selected };
    // Guarded write (#879): an unconditional `*focus = next` would mark
    // the resource changed every frame.
    if *focus != next {
        *focus = next;
    }
}

/// Drop the room-scoped editor selection when the room changes (#1237
/// f142, moved out of `player::portal` by #1297 group 3).
///
/// The selection is an INDEX into the room record's placements, so it
/// belongs to one world. Travel swaps the record, the DID, the socket,
/// the peers, the chat and the player's pose in one step and never
/// touched the editor state, so an index into the room you left arrived
/// pointing into a stranger's. The ownership gates elsewhere stop it
/// being *drawn* or *dragged*; this is the state itself not surviving the
/// journey.
///
/// [`crate::state::CurrentRoomDid`] has exactly two writers —
/// `install_completed_session` and `poll_portal_travel_tasks` — which are
/// precisely the two moments a room-scoped selection stops meaning
/// anything. The clear used to sit inside `release_travel_on_arrival`,
/// several frames later, and reaching it needed the world pipeline to
/// hold a `ResMut<RoomEditorState>`; watching the DID from this side is
/// both earlier and one less edge into the egui layer. At login there is
/// nothing selected, so the insert fires it harmlessly.
pub fn clear_selection_on_room_change(
    room: Option<Res<crate::state::CurrentRoomDid>>,
    editor: Option<ResMut<RoomEditorState>>,
) {
    let Some(room) = room else {
        return;
    };
    if !room.is_changed() {
        return;
    }
    // Guarded (#879): `RoomEditorState` is not prefs-watched, but the undo
    // ring keys off its change ticks, so a write per frame is not free.
    if let Some(mut editor) = editor
        && editor.has_selection()
    {
        editor.clear_selection();
    }
}

/// Toast the entity-budget truncation once per compile that hit it (#1211).
/// `Update`, `InGame`. The executor cannot toast itself — `compile_room_record`
/// sits at the parameter ceiling — so it leaves the resource and this
/// drains it, the shape `grammar_diag` uses to cross the same boundary.
/// Guarded-dirty: the resource is written only on the frame it announces.
pub fn announce_compile_truncation(
    truncated: Option<ResMut<crate::world_builder::WorldCompileTruncated>>,
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
) {
    let Some(mut truncated) = truncated else {
        return;
    };
    if truncated.announced {
        return;
    }
    truncated.announced = true;
    toasts.warn(compile_truncated_text(&truncated), time.elapsed_secs_f64());
}

/// Live-sync gauge (#1123): what the whole-room peer broadcast weighs
/// against the reliable-payload ceiling, beside the per-record PDS budget
/// the row above shows.
///
/// Two gauges because there are two limits and they move independently. A
/// room split across a slim manifest and many small children can sit deep
/// inside the per-record budget — green, saveable — while the monolithic
/// `RoomStateUpdate` that carries the owner's *unsaved* edits to guests is
/// past the wire ceiling and being refused. That is the whole of #1123: the
/// one number the owner could see was the one that was fine.
///
/// Kept quiet under the ceiling — this is a limit almost no room reaches,
/// and a second permanent line of alarm-coloured text in the footer would
/// cost every owner for a case that costs a few of them. It warns from 75%
/// and names the consequence at the ceiling.
fn live_sync_gauge(ui: &mut egui::Ui, bytes: Option<usize>) {
    use crate::config::network::MAX_RELIABLE_PAYLOAD_BYTES as CEILING;
    let Some(bytes) = bytes else {
        return;
    };
    let theme = crate::ui::theme::current(ui.ctx());
    let (text, color) = if bytes > CEILING {
        (
            format!(
                "{} Live sync paused — {} over the {} peer-sync limit",
                crate::ui::affordances::CROSS,
                crate::pds::record_size::human_bytes(bytes),
                crate::pds::record_size::human_bytes(CEILING),
            ),
            theme.status.error,
        )
    } else if bytes * 4 >= CEILING * 3 {
        (
            format!(
                "⚠ Live sync {} of the {} peer-sync limit",
                crate::pds::record_size::human_bytes(bytes),
                crate::pds::record_size::human_bytes(CEILING),
            ),
            theme.status.warn,
        )
    } else {
        return;
    };
    ui.label(egui::RichText::new(text).color(color).small())
        .on_hover_text(format!(
            "Your unsaved edits reach guests as one peer-to-peer message \
             carrying the whole room. Past {} that message is refused and \
             guests keep seeing your last saved version until you save again. \
             This is a separate limit from the record size beside \"Save\", \
             which measures the largest single record a save writes.",
            crate::pds::record_size::human_bytes(CEILING),
        ));
}

/// The recovery banner's words, per cause (#1265 f210).
///
/// Pure and separate because the two causes disagree about the one thing
/// this banner decides: whether the owner is offered a hard delete of the
/// record on their PDS. The banner used to assert `Decode`'s headline
/// ("incompatible with this build") over whatever string arrived, so a
/// server that went away for long enough rendered as "Decode error: PDS
/// unreachable — …" above a button that destroys a perfectly good world.
///
/// **`Unreachable` offers no reset at all.** Not a disabled one, not a
/// confirmed one: nothing here knows the stored record is bad, and the
/// non-destructive remedy — travel out through a gateway and back, which
/// re-reads the record behind #1231's arrival gate — is the whole of what
/// this state should suggest.
#[derive(Debug)]
struct RecoveryBanner {
    headline: &'static str,
    /// What labels the raw error underneath. `Decode` names a decode
    /// error because that is what it is; `Unreachable`'s string is a
    /// transport failure and calling it a decode error is the lie.
    detail_prefix: &'static str,
    body: &'static str,
    offers_reset: bool,
}

fn recovery_banner(cause: crate::state::RecoveryCause) -> RecoveryBanner {
    match cause {
        crate::state::RecoveryCause::Decode => RecoveryBanner {
            headline: "⚠ Your stored world can't be read by this version.",
            detail_prefix: "Decode error",
            body: "You are currently editing the default homeworld. Click below to \
                   overwrite the stored copy with this default so the next login \
                   loads cleanly. Saving will overwrite the stored copy too \
                   (you'll be asked first).",
            offers_reset: true,
        },
        crate::state::RecoveryCause::Unreachable => RecoveryBanner {
            headline: "⚠ Couldn't load your world — showing the generated default.",
            detail_prefix: "Reason",
            body: "Your stored world is still there; this session just never \
                   managed to read it. Saving would overwrite it with what you \
                   see now (you'll be asked first).",
            offers_reset: false,
        },
    }
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
        mut asset_caches,
        audio_monitor,
        mut audio_requests,
        heightmap,
        mut blob_ctx,
        players,
        gizmo_focus,
        mut audio_muted,
        terrain_task,
        peers,
        mut player_move,
        grammar_diag,
        road_stats,
        compile_truncation,
        mut chrome,
        mut publish_shortcut,
        mut toasts,
        undo_history,
        mut undo_shortcut,
        mut undo_labels,
        mut face_pick,
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

    editor.raw.ensure_seeded(&record.0);

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
        tree,
        selected_placement,
        extra_placements,
        placement_filter,
        placement_sort,
        generator_filter,
        node_clipboard,
        selected_effect,
        raw,
        pending_flush_secs,
        audio_editor,
        reroll,
        recovery_reset_confirm,
        placement_bulk_delete,
        publish_guard,
        default_cache,
        stored_baseline,
        live_baseline,
        size_readout_generation,
        live_sync_bytes,
        ..
    } = &mut *editor;

    let ctx = contexts.ctx_mut().unwrap();

    // One borrowed view of the four asset caches for the whole frame
    // (#1246). Built here rather than per tab because it holds the retry
    // channel mutably, and a second one would be a second `&mut`.
    let mut asset_panel = asset_caches.panel(time.elapsed_secs_f64());

    // `ResMut::deref_mut` unconditionally flips the change tick, so any
    // `&mut record.field` access taken while the window is open would mark
    // the resource as changed every frame — which in turn spams peers with
    // `RoomStateUpdate` broadcasts even when nothing was actually edited.
    // Route all UI access through `bypass_change_detection` and call
    // `record.set_changed()` explicitly at the bottom only when a widget or
    // Load/Reset click actually mutated the record.
    let mut widget_change = false;
    let mut needs_broadcast = false;
    // #1239 f81: the Region Assets tab's "Place it at my position" writes
    // its request here, because it is this level that holds the record's
    // placements, the player's pose and the tab/selection state.
    let mut place_root: Option<String> = None;

    {
        // Taken before the bypassing reborrow below, which is what makes
        // this readable at all: it is the tick of the last real
        // `set_changed()`, i.e. every edit that reached the record from
        // OUTSIDE this editor (the 3D gizmo, an inventory drop, a peer's
        // live-sync update, an undo restore, a fresh fetch). #1270 f418.
        let record_tick = record.last_changed();
        let record_mut: &mut RoomRecord = &mut record.bypass_change_detection().0;

        // Rename dialog — the shared modal (#838): keeps itself open on an
        // empty/taken name with the reason inline, Enter applies, Esc
        // cancels. Cloning the `(old, draft)` pair out first lets us mutate
        // the draft in a scratch variable and feed the final decision back
        // into the panel's `renaming` without holding a long-lived mutable
        // borrow across the modal's `show` call.
        if let Some((old_name, mut new_name)) = tree.renaming.clone() {
            let outcome = crate::ui::confirm::rename_dialog(
                ctx,
                "Rename item",
                &old_name,
                &mut new_name,
                |draft| record_mut.generators.contains_key(draft),
            );
            match outcome {
                crate::ui::confirm::RenameOutcome::Open => {
                    tree.renaming = Some((old_name, new_name));
                }
                crate::ui::confirm::RenameOutcome::Cancelled => {
                    tree.renaming = None;
                }
                crate::ui::confirm::RenameOutcome::Renamed(applied) => {
                    // The record half of the rename — moving the key and
                    // carrying every Placement and traits entry that named
                    // it — belongs to the tree source, beside the delete
                    // that sweeps those same references
                    // (`generators::retarget_root_refs`). What stays here
                    // is the editor's own half: where the tree now points,
                    // and what the undo entry is called.
                    if generators::RoomTreeSource::new(record_mut).rename_root(&old_name, &applied)
                    {
                        tree.selection.root = Some(applied.clone());
                        undo_labels.set_room(format!("rename {old_name} to {applied}"));
                        // Tree-view ids are keyed on `(root, path)`, so the
                        // rename also has to retarget the current selection at
                        // the new root key — otherwise the tree highlights
                        // nothing while the gizmo still tracks the renamed
                        // root.
                        tree.view.set_one_selected(GenNodeId::root(applied));
                        widget_change = true;
                    }
                    tree.renaming = None;
                }
            }
        }

        let (pos, size) = chrome.place(crate::ui::layout::UiWindow::WorldEditor, ctx);
        // Guarded-dirty (#879): `.open(&mut panels.world_editor)` through
        // the `ResMut` would mark UiPanels changed every frame, starving
        // the prefs save debounce — local copy in, write back on close.
        let mut open = panels.world_editor;
        let world_editor_response = egui::Window::new("World Editor")
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_size(size)
            .default_pos(pos)
            .constrain_to(chrome.available_rect(ctx))
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
                    let words = recovery_banner(rec.cause);
                    banner.show(ui, |ui| {
                        ui.colored_label(
                            crate::ui::theme::current(ui.ctx()).danger_surface_text,
                            words.headline,
                        );
                        ui.label(format!("{}: {}", words.detail_prefix, rec.reason));
                        ui.label(words.body);
                        // The non-destructive direction, named (#1230 f33).
                        // Unlike the avatar and inventory banners this is a
                        // sentence rather than a button: re-reading a room
                        // record in place regenerates the terrain and
                        // recompiles the world under the owner's feet with
                        // no arrival gate — which is the defect #1231 f20
                        // is about — while travelling out and back already
                        // does the re-read behind the gate that exists for
                        // it.
                        ui.label(
                            egui::RichText::new(
                                "If your account's server was only briefly unreachable, \
                                 travelling out through a gateway and back home reads \
                                 the stored copy again — no logout needed.",
                            )
                            .small(),
                        );
                        // Confirmed reset (#840): this button hard-deletes
                        // and replaces the stored record — never on the
                        // click itself. Offered ONLY when the cause knows
                        // the stored record is unreadable (#1265 f210).
                        if words.offers_reset && ui.button("Reset to default").clicked() {
                            recovery_reset_confirm.request(
                                "Reset your stored world?",
                                "Deletes the world stored on your account and replaces \
                                 it with this default. Whatever the old one contained \
                                 is gone for good.",
                                "Reset stored world",
                                (),
                            );
                        }
                        if recovery_reset_confirm
                            .show(ui.ctx(), "room-recovery-reset")
                            .is_some()
                        {
                            let default_record = pds::RoomRecord::default_for_did(&room_did.0);
                            *record_mut = default_record.clone();
                            raw.sync_to(&default_record);
                            needs_broadcast = true;
                            undo_labels.set_room("reset to default");
                            // Use the delete-then-put reset path. The vanilla
                            // putRecord upsert can return 500 when the stored
                            // record is incompatible with the current lexicon;
                            // hard-deleting first sidesteps that failure mode.
                            //
                            // A save in flight like any other (#1199): the
                            // status line shows it, the Save button reads
                            // "Saving…", and the unsaved guard waits for it.
                            // The marker itself retires in `poll_publish_tasks`
                            // when the write is KNOWN to have landed — retiring
                            // it here left a failed reset with no banner and
                            // no way to retry.
                            publish_feedback.status = PublishStatus::Publishing {
                                since_secs: time.elapsed_secs_f64(),
                            };
                            spawn_reset_task(
                                &mut commands,
                                &session,
                                &refresh_ctx,
                                default_record,
                                room_did.0.clone(),
                                time.elapsed_secs_f64(),
                            );
                        }
                    });
                    ui.add_space(6.0);
                }

                // Tab bar
                ui.horizontal(|ui| {
                    // Unparsed Raw JSON edits are marked on the tab itself
                    // (#1212), the one place they are visible from another
                    // tab.
                    let raw_label = if raw.is_edited() {
                        "Raw JSON •"
                    } else {
                        "Raw JSON"
                    };
                    let tabs = [
                        (EditorTab::Environment, "Environment"),
                        (EditorTab::Generators, "Items"),
                        (EditorTab::Placements, "Placements"),
                        (EditorTab::Effects, "Effects"),
                        (EditorTab::Raw, raw_label),
                    ];
                    for (tab, label) in tabs {
                        if ui.selectable_label(*selected_tab == tab, label).clicked() {
                            // Refresh the JSON text when the user arrives at
                            // the Raw tab so it reflects any edits made in
                            // the other tabs since the last time it was
                            // viewed — unless the buffer holds unparsed
                            // edits, which are kept (#1212).
                            if tab == EditorTab::Raw && *selected_tab != EditorTab::Raw {
                                raw.sync_to(&*record_mut);
                            }
                            // Drop selections whose tab we're leaving so the
                            // 3D gizmo doesn't linger on an entity the user
                            // can no longer see in the current panel.
                            if tab != *selected_tab {
                                if tab != EditorTab::Placements {
                                    *selected_placement = None;
                                }
                                if tab != EditorTab::Generators {
                                    tree.selection.clear();
                                    tree.view.set_selected(Vec::new());
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
                    // "Go to" (#1244 f148): the tree is the primary way to
                    // select, and `sync_gizmo_selection` attaches the gizmo
                    // to whichever live instance is nearest the CAMERA — so
                    // selecting a distant or behind-the-camera asset
                    // produced no visible result and there was no command
                    // to reach it. Disabled with the reason when there is
                    // nothing aimed.
                    let go = ui
                        .add_enabled(
                            gizmo_focus.centre.is_some(),
                            egui::Button::new("Go to").small(),
                        )
                        .on_hover_text("Walk to the selected object (F)")
                        .on_disabled_hover_text("Select something first");
                    if go.clicked()
                        && let (Some(centre), Ok(player)) = (gizmo_focus.centre, players.single())
                    {
                        player_move.request(crate::player::PlayerMove::GoTo(
                            crate::player::go_to_pose(
                                centre,
                                gizmo_focus.radius,
                                player.translation,
                            ),
                        ));
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

                // --- Manual re-roll -------------------------------------
                // The same DID-seeded engine that builds the defaults, but
                // with an owner-chosen master seed. Re-rolling replaces the
                // whole working record exactly like "Reset to default"
                // (which is this with seed = fnv1a_64(did)) — clear
                // selections, refresh the raw-JSON mirror, and arm a
                // broadcast/recompile.
                //
                // Laid out in the window's normal flow rather than inside
                // the footer panel (#1048). A `TopBottomPanel` reserves the
                // height it measured LAST frame, so on the frame the
                // collapsible section below opens, the taller content
                // overflowed that reserve and egui grew the window to
                // contain it — and a `Window`'s desired size never shrinks
                // again, so collapsing handed the freed height to the
                // greedy tab body instead of giving it back. Toggling
                // therefore ratcheted the window taller every cycle. Here
                // the tab body measures what is left AFTER this block is
                // laid out, so the body absorbs the change in the same
                // frame and the window height never moves.
                let did_seed = crate::seeded_defaults::fnv1a_64(&room_did.0);
                // Collapsible (#1047): the seed field plus five pin rows is
                // the tallest fixed furniture in this window, and an owner
                // who has settled on a world rarely re-rolls it again.
                // Collapsed, the whole block folds to one header row and the
                // tab body takes back the space.
                let (action, start, effective) = crate::ui::editable::reroll_section(
                    ui,
                    "world_reroll",
                    "Seed & re-roll",
                    |ui| {
                        let action = seed_row(
                            ui,
                            &mut reroll.seed_row,
                            did_seed,
                            time.elapsed_secs_f64(),
                            "world",
                        );

                        // Pinned re-roll readout (#1005): what "Re-roll" will
                        // roll for each top-level scene axis, each lockable.
                        // The preview derives from the hunted seed — the one
                        // a click will actually build from — not the typed
                        // one, so 🎲 previews exactly what Apply then
                        // delivers. Memoized: the hunt only reruns when the
                        // seed text or the pins change.
                        let start = reroll.start_seed(did_seed);
                        let effective = reroll.effective_seed(start);
                        use crate::seeded_defaults::{
                            BiomeArchetype, EscalationTier, LandformArchetype, ProsperityTier,
                            SceneCharacter, ThemeArchetype,
                        };
                        let rolled = SceneCharacter::for_seed(effective.unwrap_or(start));
                        egui::Grid::new("scene_pin_axes")
                            .num_columns(3)
                            .show(ui, |ui| {
                                pin_axis_row(
                                    ui,
                                    "Landform",
                                    &LandformArchetype::ALL,
                                    LandformArchetype::label,
                                    &mut reroll.pins.landform,
                                    rolled.landform,
                                );
                                pin_axis_row(
                                    ui,
                                    "Biome",
                                    &BiomeArchetype::ALL,
                                    BiomeArchetype::label,
                                    &mut reroll.pins.biome,
                                    rolled.biome,
                                );
                                pin_axis_row(
                                    ui,
                                    "Theme",
                                    &ThemeArchetype::ALL,
                                    ThemeArchetype::label,
                                    &mut reroll.pins.theme,
                                    rolled.theme,
                                );
                                pin_axis_row(
                                    ui,
                                    "Prosperity",
                                    &ProsperityTier::ALL,
                                    ProsperityTier::label,
                                    &mut reroll.pins.prosperity,
                                    rolled.prosperity_tier(),
                                );
                                pin_axis_row(
                                    ui,
                                    "Escalation",
                                    &EscalationTier::ALL,
                                    EscalationTier::label,
                                    &mut reroll.pins.escalation,
                                    rolled.escalation_tier(),
                                );
                            });
                        crate::ui::editable::hunt_disclosure_line(ui, start, effective);
                        (action, start, effective)
                    },
                )
                // Collapsed: no Re-roll button was drawn, so there is
                // nothing to act on this frame.
                .unwrap_or((SeedAction::None, did_seed, None));

                if let SeedAction::Reroll(_) = action {
                    // Build from the same hunted seed the readout previewed
                    // — never the raw typed one.
                    if let Some(seed) = effective {
                        // Said BEFORE the build, because the build is the
                        // stall (#1249 f270): `default_for_seed` runs the
                        // whole procedural pipeline inside this frame, and
                        // a toast queued after it would appear on the far
                        // side of the freeze it was meant to explain.
                        let clicked_at = time.elapsed_secs_f64();
                        toasts.info(
                            format!("Re-rolling your world from seed {seed}…"),
                            clicked_at,
                        );
                        commands.insert_resource(crate::world_builder::RebuildAnnounce {
                            what: format!("Re-rolled from seed {seed}"),
                            since_secs: clicked_at,
                        });
                        reroll.seed_row.set_seed(seed);
                        *record_mut = pds::RoomRecord::default_for_seed(seed, &room_did.0);
                        raw.sync_to(&*record_mut);
                        tree.selection.root = None;
                        *selected_placement = None;
                        tree.selection.path = None;
                        tree.view.set_selected(Vec::new());
                        needs_broadcast = true;
                        undo_labels.set_room(format!("seed re-roll ({seed})"));
                    } else {
                        // Unreachable in practice (the cap misses a legal
                        // pin-set with probability ~e⁻¹³⁸); keep the record
                        // untouched rather than violate the locks.
                        bevy::log::warn!(
                            "pinned re-roll found no seed matching {:?} from {start}",
                            reroll.pins
                        );
                        // Said out loud, not only logged (#1268 f69): the
                        // click otherwise does literally nothing, with the
                        // axis readout still previewing a world it will
                        // not build. The line under the seed row says the
                        // same thing; the toast is for the click.
                        toasts.error(
                            "No seed matches these locks — unlock an axis and try again.",
                            time.elapsed_secs_f64(),
                        );
                    }
                }
                ui.separator();

                // --- Footer as a real bottom panel (#830 idiom) ---------
                // The old code reserved a fixed FOOTER_RESERVE below the
                // tab body; the pinned re-roll readout (#1005) made the
                // real footer taller than the guess, so the window grew by
                // the overflow every frame until it spanned the screen.
                // Declared BEFORE the tab body (egui's panels-before-
                // content rule) but rendered pinned to the window's bottom
                // edge; the tab body then fills exactly what remains.
                // Everything in here is fixed-height, which is what keeps
                // the panel's reserve honest (see the re-roll block above).
                egui::Panel::bottom("world_editor_footer")
                    .resizable(false)
                    .show(ui, |ui| {
                        // Publish / Revert to saved / Reset to default — the
                        // shared row + status line used by every editor
                        // (`ui::editable`). `dirty` is *derived* (the live
                        // record serialises differently from the stored
                        // snapshot) rather than a flag: a failed publish
                        // stays dirty and retryable, and an out-of-band edit
                        // (the 3D gizmo, an inventory drop) lights the row
                        // up with no explicit `mark_dirty` call. Rebuild the
                        // seeded default only when the room DID changes, not
                        // every frame (#637) — it's a full procedural build.
                        let did = &room_did.0;
                        if default_cache.as_ref().is_none_or(|(d, _, _)| d != did) {
                            let default_record = pds::RoomRecord::default_for_did(did);
                            let default_value = serde_json::to_value(&default_record).ok();
                            *default_cache = Some((did.clone(), default_record, default_value));
                        }
                        let (_, default_record, default_value) =
                            default_cache.as_ref().expect("just populated");
                        // Both comparison baselines are cached (#674): the
                        // stored side re-serializes only when the resource
                        // changes and the default side only per DID, so an
                        // open panel pays for ONE live-record serialization
                        // per frame. The comparisons are value-identical to
                        // `records_differ` (Option<Value> both sides,
                        // `.ok()` semantics preserved).
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
                        // …and the LIVE side is cached too now (#1270
                        // f418). `record_tick` covers every edit from
                        // outside this editor; the `touch()` at the bottom
                        // of this system covers this editor's own widgets,
                        // which write through `bypass_change_detection` and
                        // so move no tick at all until the ~0.25 s debounce
                        // drains. Without that second half a dragged slider
                        // would leave the Save row reading "no unsaved
                        // changes" for the whole drag.
                        let (dirty, can_reset) = {
                            let live_value = live_baseline.value(record_tick, &*record_mut);
                            let dirty = match stored_baseline.as_ref() {
                                Some((_, baseline)) => baseline != live_value,
                                None => true,
                            };
                            (dirty, default_value != live_value)
                        };
                        // `session` + `refresh_ctx` are guaranteed present
                        // (the early return at the top bails otherwise), so
                        // the PDS write can always be attempted while dirty.
                        // Size readout: the room publishes as a manifest +
                        // child generator records (#697), so the per-record
                        // budget applies to the largest single record — not
                        // the in-memory monolith. Same throttled cache as
                        // the other editors.
                        let now = time.elapsed_secs_f64();
                        // …and skip it entirely on a record that has not
                        // changed since it was last measured (#1270 f418).
                        // Both encodes below are whole-record: the readout
                        // serializes the manifest AND every one of the 256
                        // generators, the gauge bincodes the entire room.
                        // The rebuild count is a conservative content
                        // generation — a rebuild that happens to produce
                        // identical bytes re-measures, which costs a
                        // measurement and never a stale number.
                        let generation = live_baseline.recomputes();
                        if crate::ui::editable::refresh_size_readout(
                            &mut *publish_feedback,
                            &*record_mut,
                            now,
                            *size_readout_generation != Some(generation),
                            pds::room::measure_publish,
                        ) {
                            *size_readout_generation = Some(generation);
                            // Second measurement on the same throttle
                            // (#1123): what the live-sync broadcast puts on
                            // the wire, which is the WHOLE room in one
                            // message — not the largest single record the
                            // publish splits it into.
                            *live_sync_bytes =
                                crate::protocol::OverlandsMessage::room_state_update(record_mut)
                                    .as_ref()
                                    .and_then(crate::network::chunk::wire_payload_bytes);
                        }
                        let size = publish_feedback.live_size.clone();
                        let ctrl_s = publish_shortcut.take(crate::ui::shortcuts::EditorKind::World);
                        let mut do_publish = false;
                        match save_load_reset_row(
                            ui,
                            crate::ui::editable::SaveRow {
                                kind: crate::diagnostics::event::RecordKind::Room,
                                dirty,
                                can_publish: true,
                                can_reset,
                                size: &size,
                                publish_shortcut: ctrl_s,
                                status: &mut publish_feedback.status,
                                // Undo covers Revert/Reset here (#866) — no modal.
                                confirm: None,
                                reset: crate::ui::editable::ResetWording::Record,
                            },
                        ) {
                            RecordAction::None => {}
                            RecordAction::Refused(reason) => {
                                toasts.info(crate::ui::editable::ctrl_s_refused(&reason), now);
                            }
                            RecordAction::Publish => {
                                // Clobber protection (#1199, the room half of
                                // #840): after an unrecoverable fetch the
                                // editor holds the default while the real
                                // record may still sit on the PDS — the
                                // first publish asks. Ctrl+S lands here too.
                                match recovery.as_deref() {
                                    Some(rec) => crate::ui::editable::request_overwrite_confirm(
                                        publish_guard,
                                        crate::diagnostics::event::RecordKind::Room,
                                        &rec.reason,
                                    ),
                                    None => do_publish = true,
                                }
                            }
                            RecordAction::Load => {
                                if let Some(stored) = stored.as_ref() {
                                    *record_mut = stored.0.clone();
                                    raw.sync_to(&*record_mut);
                                    tree.selection.root = None;
                                    *selected_placement = None;
                                    tree.selection.path = None;
                                    tree.view.set_selected(Vec::new());
                                    needs_broadcast = true;
                                    undo_labels.set_room("revert to saved");
                                }
                            }
                            RecordAction::Reset => {
                                *record_mut = default_record.clone();
                                raw.sync_to(&*record_mut);
                                tree.selection.root = None;
                                *selected_placement = None;
                                tree.selection.path = None;
                                tree.view.set_selected(Vec::new());
                                needs_broadcast = true;
                                undo_labels.set_room("reset to default");
                            }
                        }
                        if publish_guard
                            .show(ui.ctx(), "room-recovery-publish")
                            .is_some()
                        {
                            // Acknowledged. The marker stays until the poll
                            // system sees the write land (#1199).
                            do_publish = true;
                        }
                        if do_publish {
                            publish_feedback.status = PublishStatus::Publishing { since_secs: now };
                            spawn_room_publish_task(
                                &mut commands,
                                &session,
                                &refresh_ctx,
                                record_mut.clone(),
                                room_did.0.clone(),
                                now,
                            );
                        }

                        if let Some(task) = terrain_task.as_deref() {
                            ui.label(
                                egui::RichText::new(terrain_rebuild_line(task.1, now))
                                    .small()
                                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
                            )
                            .on_hover_text(
                                "The land you are standing on is the previous version; it is \
                                 replaced when the new one finishes. Editing the terrain again \
                                 starts the job over.",
                            );
                        }
                        live_sync_gauge(ui, *live_sync_bytes);
                        crate::ui::editable::audience_notice(
                            ui,
                            crate::ui::editable::EditVisibility::Live,
                            peers.iter().count(),
                            "world",
                        );
                        counts_line(ui, record_mut);
                        if let Some(truncated) = compile_truncation.as_deref() {
                            ui.label(
                                egui::RichText::new(compile_truncated_text(truncated))
                                    .color(crate::ui::theme::current(ui.ctx()).status.error)
                                    .small(),
                            )
                            .on_hover_text(
                                "Placements are built in list order, so it is the LATER rows \
                                 that are missing from the world. Lower a scatter's count or \
                                 delete placements, and the world rebuilds.",
                            );
                        }
                        publish_status_line(ui, &publish_feedback.status, now, dirty);
                    });

                // The tab body fills exactly what the footer left over.
                const BODY_MIN_HEIGHT: f32 = 160.0;
                let body_height = ui.available_height().max(BODY_MIN_HEIGHT);

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
                                placements::PlacementList {
                                    filter: placement_filter,
                                    sort: placement_sort,
                                    extra: extra_placements,
                                    bulk_delete: placement_bulk_delete,
                                },
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
                                &mut asset_panel,
                                &mut audio_muted,
                                peers.iter().count(),
                            );
                        });
                    }
                    EditorTab::Generators => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            let mut tree_source = generators::RoomTreeSource::new(record_mut);
                            generators::draw_generators_tab(
                                ui,
                                &mut tree_source,
                                tree,
                                // A reborrowed `Mut`, never `as_deref_mut()`
                                // (#1322): this runs every frame the tab is
                                // drawn, and the stash's tick must move only
                                // when the tree actually writes to it.
                                inventory.as_mut().map(|inv| inv.reborrow()),
                                audio_editor,
                                &grammar_diag,
                                &mut widget_change,
                                &mut blob_ctx.selected_element,
                                &mut toasts,
                                time.elapsed_secs_f64(),
                                &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::World),
                                Some(&road_stats),
                                &mut face_pick,
                                &session.did,
                                &mut place_root,
                                generator_filter,
                                node_clipboard,
                                &mut asset_panel,
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
                                        &mut asset_panel,
                                    );
                                }
                                EditorTab::Raw => {
                                    raw::draw_raw_tab(
                                        ui,
                                        raw,
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
            });

        // #1239 f81: perform the requested placement. Appending here — not
        // in the tree panel — is what lets it also switch the tab and land
        // the selection on the new row, so the user SEES the thing the
        // click made.
        if let Some(root) = place_root {
            let cap = caps::Cap::Placements;
            if cap.is_full(record_mut.placements.len()) {
                toasts.warn(cap.full_reason(), time.elapsed_secs_f64());
            } else {
                undo_labels
                    .slot(crate::ui::shortcuts::EditorKind::World)
                    .set(format!("placement of {root}"));
                record_mut
                    .placements
                    .push(placements::new_absolute_placement(
                        root,
                        placements::anchor_xz(player_pose),
                    ));
                *selected_placement = Some(record_mut.placements.len() - 1);
                *selected_tab = EditorTab::Placements;
                widget_change = true;
            }
        }

        // Pop-out audio editor — a top-level Window sibling to the World
        // Editor so its node canvas has room to pan/zoom. Slot-agnostic:
        // it edits a native working copy and stages the committed result
        // in `audio_editor`'s pending map, which the matching slot's bridge
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
        if panels.world_editor && !open {
            panels.world_editor = false;
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
            tree.selection.root = None;
            tree.selection.path = None;
            tree.view.set_selected(Vec::new());
        }
    }

    // Cross-editor mutex: when this frame's room selection rose from
    // None → Some, drop the avatar editor's visuals selection so only
    // one gizmo is attached at a time. The reverse direction is
    // enforced by the analogous block in `avatar::avatar_ui`. Read
    // selection state via the destructured fields — `editor` is still
    // mutably borrowed until end of function.
    let now_room_selected = selected_placement.is_some() || tree.selection.path.is_some();
    if now_room_selected && !prev_room_selected && avatar_editor.has_visuals_selection() {
        avatar_editor.release_visuals_aim();
    }

    // A widget edit only arms the broadcast/recompile debounce now —
    // the Publish/Load row's dirty state is derived from
    // `records_differ`, so there is no flag to set here.
    if widget_change || needs_broadcast {
        // The record changed through `bypass_change_detection`, so no tick
        // moved and the cached wire form is stale (#1270 f418). One place,
        // at the end of the frame — the tab bodies draw after the footer
        // that reads the answer, so the one-frame latency is the one they
        // always had.
        live_baseline.touch();
    }
    if widget_change {
        *pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
        // The cheap lane (#1249 f59). `set_changed()` below waits for the
        // debounce, so a slider being dragged showed nothing until the
        // hand stopped — a hard binary of nothing, then everything, on the
        // one tab whose whole job is to be tuned by eye. Stamping this
        // marks only `apply_environment_state`, which re-paints light, fog,
        // sky and cloud uniforms and is safe at frame rate; the peer
        // broadcast, the world compile and the terrain rebuild all still
        // wait for the pause.
        commands.insert_resource(crate::world_builder::compile::EnvironmentPreview);
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

#[cfg(test)]
mod terrain_rebuild_tests {
    use super::*;

    /// #1249 f63. A terrain edit dispatches an async heightmap job whose
    /// cost the knobs set (grid to 2048, drops to 500 000) and whose only
    /// completion signal was a session-log line. The footer now counts the
    /// wait out loud; this pins the two facts the line has to carry — that
    /// something is happening, and how long it has been.
    #[test]
    fn the_rebuild_line_names_the_work_and_counts_the_wait() {
        let line = terrain_rebuild_line(10.0, 13.4);
        assert!(line.contains("Rebuilding"), "{line}");
        assert!(line.contains('3'), "three seconds in: {line}");
        // A clock that has not moved reads as zero rather than as
        // something negative or missing.
        assert!(terrain_rebuild_line(10.0, 10.0).contains('0'));
        // And it never goes backwards if the stamp is somehow ahead.
        assert!(terrain_rebuild_line(10.0, 9.0).contains('0'));
    }
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    /// #1211, findings 61 / 271. Sequence: set a scatter to 60 000, watch
    /// half the buildings vanish. The executor stopped at the entity
    /// budget, cleared the queue, logged one `warn!`, and completed as a
    /// success. The resource it now leaves feeds one sentence — the toast
    /// and the footer — that names the count and where the loss starts.
    #[test]
    fn the_truncation_sentence_names_the_count_and_the_first_missing_row() {
        let text = compile_truncated_text(&crate::world_builder::WorldCompileTruncated {
            skipped_placements: 37,
            first_skipped_index: Some(211),
            announced: false,
        });
        assert!(text.contains("37 placements skipped"), "{text}");
        assert!(text.contains("from placement #211"), "{text}");
        assert!(
            text.contains(&crate::world_builder::compile::MAX_ROOM_ENTITIES.to_string()),
            "{text}"
        );
        let one = compile_truncated_text(&crate::world_builder::WorldCompileTruncated {
            skipped_placements: 1,
            first_skipped_index: None,
            announced: false,
        });
        assert!(one.contains("1 placement skipped"), "{one}");
    }

    /// The toast fires once per compile that truncated, never again while
    /// the same report stands, and a compile that builds everything clears
    /// the report (the executor removes the resource).
    #[test]
    fn the_truncation_toast_fires_once() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<crate::notify::Toasts>();
        app.add_systems(Update, announce_compile_truncation);
        app.insert_resource(crate::world_builder::WorldCompileTruncated {
            skipped_placements: 5,
            first_skipped_index: Some(3),
            announced: false,
        });
        app.update();
        app.update();
        let shown = app.world().resource::<crate::notify::Toasts>().shown();
        assert_eq!(shown.len(), 1, "one toast per report");
        assert_eq!(shown[0].0, crate::notify::ToastKind::Warn);
        assert!(
            app.world()
                .resource::<crate::world_builder::WorldCompileTruncated>()
                .announced
        );
    }
}

#[cfg(test)]
mod recovery_banner_tests {
    use super::*;
    use crate::state::RecoveryCause;

    /// #1265 f210. THE SEQUENCE: the owner's PDS is unreachable for the
    /// whole ten-minute retry budget, the loader installs the generated
    /// default, and the World Editor opens on "⚠ Stored room record is
    /// incompatible with this build. / Decode error: PDS unreachable — …"
    /// over a button that hard-deletes the healthy record.
    ///
    /// Two claims, both untrue in that state, and the second is the
    /// destructive one. This pins both.
    #[test]
    fn only_a_decode_failure_offers_to_destroy_the_stored_world() {
        let decode = recovery_banner(RecoveryCause::Decode);
        assert!(
            decode.offers_reset,
            "a record this build cannot read is exactly what the overwrite is for"
        );

        let unreachable = recovery_banner(RecoveryCause::Unreachable);
        assert!(
            !unreachable.offers_reset,
            "nothing here knows the stored world is bad, so nothing may destroy it"
        );
    }

    /// The copy half: an unreachable server must not be reported as a
    /// decode problem, and must not claim anything about this build.
    #[test]
    fn an_unreachable_server_is_not_reported_as_a_decode_error() {
        let unreachable = recovery_banner(RecoveryCause::Unreachable);
        assert!(
            !unreachable.detail_prefix.contains("Decode"),
            "the string under this prefix is a transport failure: {:?}",
            unreachable.detail_prefix
        );
        assert!(
            !unreachable.headline.contains("this version")
                && !unreachable.headline.contains("read by"),
            "an outage says nothing about schema compatibility: {:?}",
            unreachable.headline
        );

        // The control: the wording this was mistaken FOR is still what the
        // decode arm says, so the test would notice the two collapsing
        // back into one string.
        let decode = recovery_banner(RecoveryCause::Decode);
        assert_eq!(decode.detail_prefix, "Decode error");
        assert!(
            decode.headline.contains("read by this version"),
            "{decode:?}"
        );
    }
}

#[cfg(test)]
mod placement_focus_tests {
    use super::*;
    use crate::state::CurrentRoomDid;
    use crate::ui::toolbar::UiPanels;
    use crate::world_builder::PlacementFocus;
    use bevy::ecs::system::RunSystemOnce;
    use bevy_symbios_multiuser::auth::AtprotoSession;

    /// A signed-in owner. The mirror reads only `did`; the rest is the
    /// live signing session the type insists on (same fixture as
    /// `reauth::tests`).
    fn session(did: &str) -> AtprotoSession {
        AtprotoSession {
            did: String::from(did),
            handle: String::from("alice"),
            pds_url: String::from("https://pds.example"),
            session: std::sync::Arc::new(proto_blue_oauth::session::OAuthSession::new(
                proto_blue_oauth::types::TokenSet {
                    issuer: String::from("https://as.example"),
                    sub: String::from(did),
                    scope: String::from("atproto"),
                    access_token: String::from("access"),
                    refresh_token: Some(String::from("refresh")),
                    token_type: String::from("DPoP"),
                    expires_at: Some(String::from("2099-01-01T00:00:00Z")),
                    aud: Some(String::from("https://as.example")),
                },
                proto_blue_oauth::DpopKey::generate().expect("DPoP key"),
                proto_blue_oauth::DpopNonceCache::new(),
            )),
        }
    }

    fn mirrored(app: &mut App) -> Option<usize> {
        app.world_mut()
            .run_system_once(mirror_placement_focus)
            .expect("the mirror runs");
        app.world().resource::<PlacementFocus>().selected
    }

    /// #1297 step (4), the `RigHold` shape a second time
    /// (`the_rig_hold_mirrors_every_predicate_including_absence`). The
    /// placement visualiser used to read `RoomEditorState` and
    /// `RoomEditAccess` itself; now ONE `Option<usize>` crosses, and this
    /// is the one place the mapping can go wrong. Asserted against the
    /// predicate rather than by restating its value, and through every
    /// way the answer can be `None`: nothing to read at all, the editor
    /// on another tab, the window closed, a visitor in a stranger's room
    /// — and back, so a latched value cannot pass.
    #[test]
    fn the_placement_focus_mirrors_the_editor_and_the_access_gate_including_absence() {
        // Nothing to read — before login, and the headless render tool.
        let mut app = App::new();
        app.init_resource::<PlacementFocus>();
        assert_eq!(
            mirrored(&mut app),
            None,
            "absent editor state and absent access must focus nothing"
        );

        // The gate passes but there is no editor state yet.
        app.insert_resource(UiPanels {
            world_editor: true,
            ..Default::default()
        });
        app.insert_resource(session("did:plc:alice"));
        app.insert_resource(CurrentRoomDid(String::from("did:plc:alice")));
        assert_eq!(mirrored(&mut app), None, "access alone is not a focus");

        // Placements tab, a selection, and the owner's own open editor.
        let state = RoomEditorState {
            selected_tab: EditorTab::Placements,
            selected_placement: Some(7),
            ..Default::default()
        };
        assert_eq!(state.focused_placement(), Some(7), "the predicate itself");
        app.insert_resource(state);
        assert_eq!(mirrored(&mut app), Some(7));

        // Another tab: the same selection is not what the editor shows.
        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .selected_tab = EditorTab::Generators;
        assert_eq!(
            app.world()
                .resource::<RoomEditorState>()
                .focused_placement(),
            None
        );
        assert_eq!(mirrored(&mut app), None, "another tab focuses nothing");
        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .selected_tab = EditorTab::Placements;
        assert_eq!(mirrored(&mut app), Some(7), "and back");

        // The window closed (#1237 f142's first gate).
        app.world_mut().resource_mut::<UiPanels>().world_editor = false;
        assert_eq!(mirrored(&mut app), None, "a closed editor focuses nothing");
        app.world_mut().resource_mut::<UiPanels>().world_editor = true;

        // A visitor: the selection survived portal travel into a
        // stranger's room (#1237 f142's second gate).
        app.insert_resource(CurrentRoomDid(String::from("did:plc:bob")));
        assert_eq!(mirrored(&mut app), None, "a visitor focuses nothing");
        app.insert_resource(CurrentRoomDid(String::from("did:plc:alice")));
        assert_eq!(mirrored(&mut app), Some(7), "home again");

        // And it RELEASES on the editor's own clear.
        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .clear_selection();
        assert_eq!(
            mirrored(&mut app),
            None,
            "clearing the selection releases the focus"
        );
    }
}
