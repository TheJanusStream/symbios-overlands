//! Avatar editor — tabbed split view.
//!
//! The avatar window has two tabs:
//!
//!   * **Visuals** — embeds the same tree-view + detail-panel widget that
//!     drives the room editor's Generators tab, fed by an
//!     [`AvatarVisualsTreeSource`] adapter so the avatar's single
//!     `visuals` root is editable through the unified vocabulary
//!     (primitives only in v1).
//!   * **Locomotion** — picker for the [`crate::pds::LocomotionConfig`]
//!     preset (HoverBoat / Humanoid / Airplane / Helicopter / Car) plus a
//!     per-preset slider panel for collider dimensions and physics
//!     tuning. Each preset's panel lives in `locomotion`.
//!
//! Live UX is preserved: every widget mutates [`LiveAvatarRecord`] in
//! place, the player module rebuilds visuals or swaps locomotion the same
//! frame the resource changes, and `network::broadcast_avatar_state`
//! pushes a preview update to peers so they see the edit before the
//! author commits. Three explicit buttons drive persistence and discard
//! flows:
//!
//!   * **Save to PDS** writes the current `LiveAvatarRecord` to the
//!     owner's PDS via `com.atproto.repo.putRecord` and then syncs the
//!     value into [`StoredAvatarRecord`] on success.
//!   * **Revert to saved** drops all in-flight edits by copying
//!     [`StoredAvatarRecord`] back into `LiveAvatarRecord`.
//!   * **Reset to default** replaces `LiveAvatarRecord` with the canonical
//!     [`AvatarRecord::default_for_did`] seed.

mod locomotion;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, RecordKind};
use crate::pds::{self, AvatarRecord};
use crate::state::{
    LiveAvatarRecord, LiveInventoryRecord, PublishFeedback, PublishStatus, StoredAvatarRecord,
    records_differ,
};
use crate::ui::editable::{
    RecordAction, SeedAction, publish_status_line, save_load_reset_row, seed_row,
};
use crate::ui::room::RoomEditorState;
use crate::ui::room::generators::{AvatarVisualsTreeSource, GenNodeId, draw_generators_tab};

use locomotion::draw_locomotion_tab;

/// Async task for publishing the avatar record to the owner's PDS. Carries the
/// target `did` + dispatch time so [`poll_publish_avatar_tasks`] can emit a typed
/// `RecordWrite*` session event (with the write's duration) when it resolves.
#[derive(Component)]
pub struct PublishAvatarTask {
    pub task: bevy::tasks::Task<Result<(), String>>,
    pub did: String,
    pub spawned_at: f64,
    /// Serialized size of the record being written, measured at dispatch so
    /// the poll system can gauge + log it (#694).
    pub record_bytes: Option<usize>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum AvatarTab {
    #[default]
    Visuals,
    Locomotion,
}

/// Persistent avatar-editor state across frames. Promoted to a `Resource`
/// (alongside `RoomEditorState`) so the 3D gizmo controller in
/// `editor_gizmo` and the locomotion-freeze gate in `player::mod` can
/// observe which visuals node the owner has selected without reading
/// through the egui closure.
#[derive(Resource, Default)]
pub struct AvatarEditorState {
    selected_tab: AvatarTab,
    /// Tree-view selection mirrors the room editor's RoomEditorState.
    /// `selected_generator` is always `Some(AvatarVisualsTreeSource::ROOT_NAME)`
    /// once a node has been picked; `selected_prim_path` is the child
    /// chain into the visuals tree.
    pub selected_generator: Option<String>,
    pub selected_prim_path: Option<Vec<usize>>,
    tree_view_state: egui_ltreeview::TreeViewState<GenNodeId>,
    /// Unused for the avatar (single-root sources have no rename) but
    /// required by [`draw_generators_tab`]'s signature. Holding an owned
    /// `Option` lets us hand a `&mut` to the callee without
    /// conditionally constructing a stack reference each frame.
    renaming_unused: Option<(String, String)>,
    /// Seconds remaining before a pending widget change is flushed into
    /// `LiveAvatarRecord`'s change tick. The downstream player rebuild
    /// and `network::broadcast_avatar_state` peer broadcast fire once
    /// when the timer drains rather than every frame.
    pending_flush_secs: f32,
    /// Pop-out audio editor state for the per-construct audio slot on
    /// avatar visuals generators. Shares the same widget as the room
    /// editor; see [`crate::ui::room::audio::AudioEditorState`].
    pub(crate) audio_editor: crate::ui::room::audio::AudioEditorState,
    /// Buffer for the manual re-roll "Random seed" row — defaults to the
    /// owner's DID seed, editable to re-roll the whole avatar. See
    /// [`crate::ui::editable::seed_row`].
    seed_row_state: crate::ui::editable::SeedRowState,
    /// Pending publish-after-unrecoverable-fetch confirmation (#840):
    /// while [`crate::state::AvatarRecordRecovery`] is present the
    /// editor holds the default, and saving would overwrite the real
    /// stored record — the first publish asks first.
    publish_guard: crate::ui::confirm::ConfirmState<()>,
    /// Pending destructive tree-operation confirmations (#838): root
    /// delete (no-op for the single-root avatar tree, but the kind-change
    /// half is live) shared with the room editor's Generators machinery.
    tree_confirms: crate::ui::room::generators::TreeConfirms,
    /// Cached seeded-default record, keyed by the DID it was built for (#637).
    /// `AvatarRecord::default_for_did` runs the full part-composition pipeline,
    /// so build it once per session rather than every frame the editor is open;
    /// invalidated when the session DID changes.
    default_cache: Option<(String, AvatarRecord)>,
    /// Mirror of this frame's "Avatar window is open and un-collapsed"
    /// state, written by [`avatar_ui`] so non-UI systems can read it
    /// without reaching into egui. The gait pause keys on this (#741):
    /// the whole editing session should show the avatar at rest, not just
    /// the moments a row is selected. Collapsing the window deliberately
    /// counts as closed — tuck the panel away to preview the live sway.
    window_visible: bool,
    /// Set for one frame when an in-world pick (#823) selects a visuals
    /// node. On the next Visuals-tab draw the tree grabs keyboard focus
    /// so the picked row highlights like a direct click — the same
    /// one-shot mechanism as `RoomEditorState::pending_tree_focus`.
    pending_tree_focus: bool,
}

impl AvatarEditorState {
    /// True when a visuals row is currently selected. The locomotion
    /// freeze gate and the gizmo dispatch read this.
    pub fn has_visuals_selection(&self) -> bool {
        self.selected_prim_path.is_some()
    }

    /// True while the Avatar window is open with its body visible (as of
    /// the last [`avatar_ui`] run). The local gait pause reads this.
    pub fn window_visible(&self) -> bool {
        self.window_visible
    }

    /// True whenever the local avatar should be held perfectly still for
    /// editing: the window is open (un-collapsed) *or* a visuals row is
    /// selected during the close-frame gap. This is the wide gate the whole
    /// editing session keys on — the cosmetic gait/sway hold
    /// (`player::gait::animate_avatar_gait`) *and* the full-body chassis
    /// freeze (`player::freeze_local_avatar_while_editing`, which also stops
    /// falling-physics and the passive movers) — so the avatar shows at rest
    /// for the whole session, not just the moments a row is selected (#814).
    /// Collapsing the window resumes the live animation *and* physics for
    /// previewing.
    pub fn holds_avatar_still(&self) -> bool {
        self.window_visible || self.has_visuals_selection()
    }

    /// Drop the visuals selection — used when switching tabs, collapsing
    /// the editor window, or losing the mutex to the room editor.
    pub fn clear_visuals_selection(&mut self) {
        self.selected_generator = None;
        self.selected_prim_path = None;
        self.tree_view_state.set_selected(Vec::new());
    }

    /// Snapshot the selection state an undo entry carries (#862) so a
    /// restore (#863) can re-seed it instead of dumping the user to a
    /// full deselect.
    pub(crate) fn undo_selection(&self) -> crate::ui::undo::AvatarSelection {
        crate::ui::undo::AvatarSelection {
            generator: self.selected_generator.clone(),
            prim_path: self.selected_prim_path.clone(),
            tree: self.tree_view_state.selected().clone(),
        }
    }

    /// Post-restore fixup (#863): re-seed the visuals selection from the
    /// undo entry, validated against the restored record; drop parked
    /// confirm payloads and the pending widget debounce. Mirrors
    /// [`RoomEditorState::restore_from_undo`](crate::ui::room::RoomEditorState).
    pub(crate) fn restore_from_undo(
        &mut self,
        record: &AvatarRecord,
        sel: &crate::ui::undo::AvatarSelection,
    ) {
        self.publish_guard.cancel();
        self.tree_confirms.delete.cancel();
        self.tree_confirms.kind.cancel();
        // A pending burst was aimed at pre-restore state; draining it
        // would double-fire `set_changed` and mint a phantom entry.
        self.pending_flush_secs = 0.0;
        match &sel.prim_path {
            // `select_from_scene_pick` is exactly the fixup contract:
            // fields set, ancestors expanded, row selected + focused.
            Some(path) if crate::ui::undo::restore::node_path_valid(&record.visuals, path) => {
                self.select_from_scene_pick(path.clone());
            }
            // Root row selected without a node path: keep it — the
            // single visuals root always exists.
            None if sel.generator.is_some() => {
                let root = AvatarVisualsTreeSource::ROOT_NAME.to_string();
                self.selected_generator = Some(root.clone());
                self.selected_prim_path = None;
                self.tree_view_state
                    .set_selected(vec![GenNodeId::root(root)]);
            }
            _ => self.clear_visuals_selection(),
        }
    }

    /// Select a visuals node from an in-world scene pick (#823), exactly
    /// as if its tree row had been clicked: selection set, every
    /// ancestor expanded (the tree collapses by default, so the picked
    /// row must be revealed), the row marked selected in the tree
    /// widget, and a one-shot focus request armed so the row gets the
    /// bright focused highlight on the next draw. Mirrors the room
    /// editor's pick path in `editor_gizmo::pick_on_scene_click`.
    pub fn select_from_scene_pick(&mut self, path: Vec<usize>) {
        let root = AvatarVisualsTreeSource::ROOT_NAME.to_string();
        self.selected_generator = Some(root.clone());
        self.selected_prim_path = Some(path.clone());
        for depth in 0..path.len() {
            self.tree_view_state
                .set_openness(GenNodeId::child(root.clone(), path[..depth].to_vec()), true);
        }
        self.tree_view_state
            .set_selected(vec![GenNodeId::child(root, path)]);
        self.pending_tree_focus = true;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn avatar_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut commands: Commands,
    mut live: ResMut<LiveAvatarRecord>,
    stored: Option<Res<StoredAvatarRecord>>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    mut feedback: ResMut<PublishFeedback<AvatarRecord>>,
    mut inventory: Option<ResMut<LiveInventoryRecord>>,
    mut editor: ResMut<AvatarEditorState>,
    mut room_editor: Option<ResMut<RoomEditorState>>,
    mut gizmo_frame_pref: ResMut<crate::editor_gizmo::GizmoFramePref>,
    mut chrome: crate::ui::layout::WindowChrome,
    mut publish_shortcut: ResMut<crate::ui::shortcuts::PublishShortcut>,
    // Grouped into one tuple param so `session_log` fits under Bevy's 16-param
    // `IntoSystem` ceiling (needed to record an avatar re-seed, #627).
    (
        audio_monitor,
        mut audio_requests,
        time,
        mut session_log,
        mut blob_ctx,
        grammar_diag,
        recovery,
        mut toasts,
        undo_history,
        mut undo_shortcut,
        mut undo_labels,
    ): (
        Res<bevy_symbios_audio::ui::AudioMonitor>,
        MessageWriter<bevy_symbios_audio::ui::MonitorRequest>,
        Res<Time>,
        ResMut<SessionLog>,
        ResMut<crate::editor_gizmo::BlobEditContext>,
        Res<crate::world_builder::grammar_diag::GrammarDiagnostics>,
        Option<Res<crate::state::AvatarRecordRecovery>>,
        ResMut<crate::ui::toast::Toasts>,
        Res<crate::ui::undo::AvatarUndoHistory>,
        ResMut<crate::ui::undo::UndoShortcut>,
        ResMut<crate::ui::undo::PendingUndoLabels>,
    ),
) {
    // `ResMut::deref_mut` unconditionally flips the change tick, so
    // mutating `live.0` inside the egui closure would otherwise mark the
    // resource changed every frame the editor is visible — and
    // `network::broadcast_avatar_state` turns that into a peer broadcast
    // storm. Route UI access through `bypass_change_detection` and call
    // `live.set_changed()` explicitly below, only after the debounce
    // timer drains.
    let mut widget_changed = false;
    // Snapshot pre-frame selection state so we can detect (a) "selection
    // just appeared" — the rising edge that clears the room editor's
    // selection per the cross-editor mutex contract, and (b) tab change —
    // switching off the Visuals tab drops the gizmo target the same way
    // the room editor's tab bar already does.
    let prev_visuals_selected = editor.has_visuals_selection();

    // `.open()` only hides the window *body* — without this gate the
    // whole-record `before` clone below (and the egui Window bookkeeping)
    // ran every in-game frame with the panel closed (#674). The tail logic
    // after this block still runs: collapse-deselect sees `false` here, and
    // a debounce flush pending from just before the panel closed still
    // drains and broadcasts.
    let window_visible_with_body = if !panels.avatar {
        false
    } else {
        let live_mut = live.bypass_change_detection();
        let before = live_mut.0.clone();

        let ctx = contexts.ctx_mut().unwrap();
        // Width only from the layout slot — the Avatar window auto-heights
        // to its content, and forcing the persisted height back on it
        // would pad the shorter Locomotion tab with dead space.
        let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Avatar, ctx);
        let response = egui::Window::new("Avatar")
            .open(&mut panels.avatar)
            .default_pos(pos)
            .default_width(size.x)
            .constrain_to(ctx.available_rect())
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                // --- Tab bar ----------------------------------------------
                ui.horizontal(|ui| {
                    let tabs = [
                        (AvatarTab::Visuals, "Visuals"),
                        (AvatarTab::Locomotion, "Locomotion"),
                    ];
                    for (tab, label) in tabs {
                        if ui
                            .selectable_label(editor.selected_tab == tab, label)
                            .clicked()
                        {
                            editor.selected_tab = tab;
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
                        crate::ui::shortcuts::EditorKind::Avatar,
                        &mut undo_shortcut,
                    );
                });
                ui.separator();

                let AvatarEditorState {
                    selected_tab,
                    selected_generator,
                    selected_prim_path,
                    tree_view_state,
                    renaming_unused,
                    audio_editor,
                    seed_row_state,
                    publish_guard,
                    tree_confirms,
                    default_cache,
                    pending_tree_focus,
                    ..
                } = &mut *editor;

                // Recovery banner (#840) — the stored record could not be
                // loaded and this editor holds the DID default. Same idiom
                // as the World editor's banner; the deliberate-overwrite
                // affordance here is the publish confirm, not a reset
                // button (publishing IS the reset).
                if let Some(rec) = recovery.as_deref() {
                    egui::Frame::new()
                        .fill(crate::ui::theme::current(ui.ctx()).danger_surface)
                        .inner_margin(6.0)
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.colored_label(
                                crate::ui::theme::current(ui.ctx()).danger_surface_text,
                                "⚠ Your stored avatar could not be loaded — this is the default.",
                            );
                            ui.label(
                                egui::RichText::new(format!("Reason: {}", rec.reason)).small(),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Saving will overwrite the stored copy (you'll be asked \
                                     first). Logging out and back in retries the load.",
                                )
                                .small(),
                            );
                        });
                    ui.add_space(4.0);
                }

                // --- Footer as a real bottom panel (#830) -----------------
                // Declared BEFORE the tab body (egui's panels-before-content
                // rule) but rendered pinned to the window's bottom edge, so
                // it can never be clipped off a short window — the old
                // fixed FOOTER_RESERVE guessed the footer height and lost
                // whenever the guess was wrong. The tab body then fills
                // exactly the space that remains.
                egui::TopBottomPanel::bottom("avatar_footer")
                    .resizable(false)
                    .show_inside(ui, |ui| {
                        // The "Smooth remote peers" toggle moved to the
                        // Settings window (#857) — it's a client network
                        // preference, not part of the avatar record this
                        // editor publishes.

                        // --- Publish / Revert / Reset -------------------------
                        // Same shared row + status line as the World and
                        // Inventory editors (`ui::editable`). Dirty is derived
                        // through `records_differ` — the *same* canonical
                        // equality the other two use — instead of
                        // `AvatarRecord`'s `PartialEq`, so all three editors
                        // behave identically.

                        // Manual re-roll — the same DID-seeded engine as the
                        // defaults, with an owner-chosen master seed. Replaces
                        // the whole working avatar like "Reset to default"
                        // (which is this with seed = fnv1a_64(did)). The pfp
                        // banner tracks the DID, not the seed, so it survives
                        // a re-roll.
                        if let Some(s) = session.as_ref() {
                            let reroll = seed_row(
                                ui,
                                seed_row_state,
                                crate::seeded_defaults::fnv1a_64(&s.did),
                                time.elapsed_secs_f64(),
                                "avatar",
                            );
                            if let SeedAction::Reroll(seed) = reroll {
                                live_mut.0 = AvatarRecord::default_for_seed(seed);
                                undo_labels.set_avatar(format!("seed re-roll ({seed})"));
                                session_log.info(
                                    time.elapsed_secs_f64(),
                                    EventPayload::AvatarReseeded { seed },
                                );
                            }
                        }
                        ui.separator();

                        let dirty = stored
                            .as_ref()
                            .is_some_and(|s| records_differ(&s.0, &live_mut.0));
                        let can_publish = session.is_some() && refresh_ctx.is_some();
                        // Rebuild the seeded default only when the session DID
                        // changes, not every frame (#637) — full
                        // part-composition build.
                        match session.as_ref() {
                            Some(s) if default_cache.as_ref().is_none_or(|(d, _)| d != &s.did) => {
                                *default_cache =
                                    Some((s.did.clone(), AvatarRecord::default_for_did(&s.did)));
                            }
                            None => *default_cache = None,
                            _ => {}
                        }
                        let default_record = default_cache.as_ref().map(|(_, r)| r);
                        let can_reset =
                            default_record.is_some_and(|d| records_differ(d, &live_mut.0));

                        let record_bytes = crate::ui::editable::refresh_size_readout(
                            &mut *feedback,
                            &live_mut.0,
                            time.elapsed_secs_f64(),
                        );
                        let ctrl_s =
                            publish_shortcut.take(crate::ui::shortcuts::EditorKind::Avatar);
                        let mut do_publish = false;
                        match save_load_reset_row(
                            ui,
                            dirty,
                            can_publish,
                            can_reset,
                            record_bytes,
                            ctrl_s,
                            matches!(feedback.status, PublishStatus::Publishing),
                            // Undo covers Revert/Reset here (#866).
                            None,
                        ) {
                            RecordAction::None => {}
                            RecordAction::Publish => {
                                // Clobber protection (#840): after an
                                // unrecoverable fetch the editor holds the
                                // default while the real record may still
                                // sit on the PDS — the first publish asks.
                                if let Some(rec) = recovery.as_deref() {
                                    publish_guard.request(
                                        "Overwrite your stored avatar?",
                                        format!(
                                            "Your avatar loaded as the default because \
                                             the stored copy could not be read ({}). \
                                             Saving now replaces whatever is stored on \
                                             your PDS with what you see here.",
                                            rec.reason
                                        ),
                                        "Save anyway",
                                        (),
                                    );
                                } else {
                                    do_publish = true;
                                }
                            }
                            RecordAction::Load => {
                                if let Some(stored) = &stored {
                                    live_mut.0 = stored.0.clone();
                                    undo_labels.set_avatar("load from PDS");
                                }
                            }
                            RecordAction::Reset => {
                                if let Some(default_record) = default_record {
                                    live_mut.0 = default_record.clone();
                                    undo_labels.set_avatar("reset to default");
                                }
                            }
                        }
                        if publish_guard
                            .show(ui.ctx(), "avatar-recovery-publish")
                            .is_some()
                        {
                            // Acknowledged: the overwrite is deliberate now,
                            // so the marker (and its banner) retires.
                            commands.remove_resource::<crate::state::AvatarRecordRecovery>();
                            do_publish = true;
                        }
                        if do_publish
                            && let (Some(session), Some(refresh)) =
                                (session.as_ref(), refresh_ctx.as_ref())
                        {
                            feedback.status = PublishStatus::Publishing;
                            spawn_publish_avatar_task(
                                &mut commands,
                                session,
                                refresh,
                                live_mut.0.clone(),
                                time.elapsed_secs_f64(),
                            );
                        }

                        publish_status_line(ui, &feedback.status, time.elapsed_secs_f64());
                    });

                // The tab body fills exactly what the footer left over.
                let body_height = ui.available_height();

                match *selected_tab {
                    AvatarTab::Visuals => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            let mut source = AvatarVisualsTreeSource::new(&mut live_mut.0.visuals);
                            // One-shot focus request from an in-world pick
                            // (#823) — same consume-on-draw contract as the
                            // room editor's tree.
                            let request_focus = std::mem::take(pending_tree_focus);
                            draw_generators_tab(
                                ui,
                                &mut source,
                                selected_generator,
                                selected_prim_path,
                                tree_view_state,
                                request_focus,
                                renaming_unused,
                                inventory.as_deref_mut(),
                                audio_editor,
                                &grammar_diag,
                                &mut widget_changed,
                                &mut blob_ctx.selected_element,
                                tree_confirms,
                                &mut toasts,
                                time.elapsed_secs_f64(),
                                &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                            );
                        });
                    }
                    AvatarTab::Locomotion => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([true, false])
                            .max_height(body_height)
                            .show(ui, |ui| {
                                // The full-body edit freeze (#814) makes
                                // tuning feel like editing a statue; the
                                // sanctioned preview path existed only in
                                // code comments until #830.
                                ui.label(
                                    egui::RichText::new(
                                        "⏵ Collapse this window (double-click its title \
                                         bar) to test-drive — physics resumes while it's \
                                         collapsed, reopen to keep tuning.",
                                    )
                                    .small()
                                    .weak(),
                                );
                                ui.add_space(4.0);
                                draw_locomotion_tab(
                                    ui,
                                    &mut live_mut.0.locomotion,
                                    &mut widget_changed,
                                    &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                                );
                            });
                    }
                }
            });

        if live_mut.0 != before {
            widget_changed = true;
        }

        if let Some(response) = response.as_ref() {
            chrome.remember(crate::ui::layout::UiWindow::Avatar, response.response.rect);
        }

        // `Window::show` returns `Some(InnerResponse { inner: None, .. })`
        // when the window is rendered but collapsed (the closure does not
        // fire). `Some(InnerResponse { inner: Some(_), .. })` means the
        // body ran. `None` means the window is closed entirely. Treat
        // collapsed *and* closed identically: the user can no longer see
        // the selection in the panel, so the gizmo should detach and the
        // mutex against the room editor should release.
        response.as_ref().is_some_and(|r| r.inner.is_some())
    };
    // Publish the window state for non-UI readers (the gait pause, #741)
    // every frame this system runs — including the `!panels.avatar` arm,
    // so closing the window un-pauses without a stale frame.
    editor.window_visible = window_visible_with_body;

    // Pop-out audio editor for the per-construct slot on avatar visuals
    // generators — a top-level Window sibling to the Avatar window.
    // Rendered after the Avatar window's borrow of the egui context is
    // released. Slot-agnostic: it stages committed edits in
    // `audio_editor.committed`, which the construct's bridge in the
    // Visuals tab picks up next frame and writes into the live record.
    crate::ui::room::audio::draw_audio_editor_window(
        contexts.ctx_mut().unwrap(),
        &mut editor.audio_editor,
        &audio_monitor,
        &mut audio_requests,
        &mut chrome,
    );

    // Collapse-deselect: if the window is hidden or collapsed and we still
    // hold a visuals selection, drop it so the gizmo can detach. Mirrors
    // the room editor's tab-switch clear, which serves the same role
    // (selection only persists while the panel showing it is visible).
    if !window_visible_with_body && editor.has_visuals_selection() {
        editor.clear_visuals_selection();
    }

    // Tab-switch clear: the avatar editor doesn't gizmo-edit Locomotion,
    // so leaving the Visuals tab also drops the selection.
    if editor.selected_tab != AvatarTab::Visuals && editor.has_visuals_selection() {
        editor.clear_visuals_selection();
    }

    // Cross-editor mutex: when this frame's avatar selection rose from
    // None → Some, drop the room editor's selection so only one gizmo is
    // attached at a time. The reverse direction is enforced by the
    // analogous block in `room::room_admin_ui`.
    let now_visuals_selected = editor.has_visuals_selection();
    if now_visuals_selected
        && !prev_visuals_selected
        && let Some(room) = room_editor.as_deref_mut()
    {
        room.selected_placement = None;
        room.selected_generator = None;
        room.selected_prim_path = None;
        room.tree_view_state.set_selected(Vec::new());
    }

    if widget_changed {
        editor.pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
        // Coarse per-tab undo label (#865) when no site named the edit.
        if !undo_labels.avatar_pending() {
            undo_labels.set_avatar(match editor.selected_tab {
                AvatarTab::Visuals => "visuals edit",
                AvatarTab::Locomotion => "locomotion edit",
            });
        }
    }
    if editor.pending_flush_secs > 0.0 {
        editor.pending_flush_secs = (editor.pending_flush_secs - time.delta_secs()).max(0.0);
        if editor.pending_flush_secs <= 0.0 {
            // Debounce drained — clamp the accumulated edit through the
            // same bounds the network-ingress path enforces, then publish
            // it to player (visual rebuild) and `broadcast_avatar_state`
            // (peer preview) in a single change tick. The clamp matters:
            // egui's DragValue parses typed `NaN`/`inf` and its range
            // clamp passes NaN through, so an unsanitized flush could
            // hand NaN half-extents straight to the collider builders.
            live.bypass_change_detection().0.sanitize();
            live.set_changed();
        }
    }
}

/// Spawn the async avatar-record publish. `pub(crate)` because the
/// unsaved-edits guard ([`crate::ui::unsaved_guard`]) drives the same
/// pipeline for its "Publish & log out" path — the shared
/// [`poll_publish_avatar_tasks`] system lands the result either way.
pub(crate) fn spawn_publish_avatar_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: AvatarRecord,
    now: f64,
) {
    // The avatar record is always the local user's own, saved to their PDS, so
    // the write DID is the session DID (unlike a room save, whose DID is the
    // room owner's `CurrentRoomDid`).
    let did = session.did.clone();
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    let record_bytes = pds::record_size::serialized_record_bytes(&record);
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::publish_avatar_record(&client, &session_clone, &refresh_clone, &record).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(PublishAvatarTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
    });
}

/// Poll outstanding avatar publish tasks. On success, sync `LiveAvatarRecord`
/// into `StoredAvatarRecord` so the "Load from PDS" button is disabled until the
/// next edit.
#[allow(clippy::too_many_arguments)]
pub fn poll_publish_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishAvatarTask)>,
    live: Res<LiveAvatarRecord>,
    mut stored: Option<ResMut<StoredAvatarRecord>>,
    mut feedback: ResMut<PublishFeedback<AvatarRecord>>,
    mut session_log: ResMut<SessionLog>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        let did = task.did.clone();
        let duration_secs = now - task.spawned_at;
        crate::ui::editable::log_record_size(
            &mut session_log,
            &mut metrics,
            now,
            RecordKind::Avatar,
            task.record_bytes,
        );
        match result {
            Ok(()) => {
                info!("Avatar record saved to PDS");
                if let Some(stored) = stored.as_mut() {
                    stored.0 = live.0.clone();
                }
                feedback.status = PublishStatus::Success { at_secs: now };
                session_log.info(
                    now,
                    EventPayload::RecordWriteCompleted {
                        record: RecordKind::Avatar,
                        did,
                        duration_secs,
                    },
                );
            }
            Err(e) => {
                warn!("Failed to save avatar record: {}", e);
                session_log.error(
                    now,
                    EventPayload::RecordWriteFailed {
                        record: RecordKind::Avatar,
                        did,
                        reason: e.clone(),
                    },
                );
                feedback.status = PublishStatus::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The editing hold must engage whenever the window is open — not only
    /// when a visuals row is selected. Both the cosmetic gait/sway hold and
    /// the full-body chassis freeze (which stops falling-physics and the
    /// passive movers) key on this, so a narrow selection-only gate lets the
    /// avatar keep falling/drifting the moment no row is picked (#814).
    #[test]
    fn holds_avatar_still_covers_the_whole_open_session_not_just_selection() {
        let mut state = AvatarEditorState::default();
        // Window shut, nothing selected: live physics/animation run.
        assert!(!state.holds_avatar_still());

        // Window open, no row selected: still held (the #814 fix — this was
        // the case that previously left the chassis falling).
        state.window_visible = true;
        assert!(!state.has_visuals_selection());
        assert!(state.holds_avatar_still());

        // Window collapsed but a row lingers during the close-frame gap.
        state.window_visible = false;
        state.selected_prim_path = Some(vec![0]);
        assert!(state.holds_avatar_still());
    }

    /// #823: a scene pick must land the full row-click state — selection
    /// set to the picked path under the fixed "visuals" root, the row
    /// selected in the tree widget, every ancestor expanded, and the
    /// one-shot focus request armed (then consumed by the next draw).
    #[test]
    fn scene_pick_selects_expands_and_arms_focus() {
        let mut state = AvatarEditorState::default();
        state.select_from_scene_pick(vec![1, 0, 2]);

        assert_eq!(
            state.selected_generator.as_deref(),
            Some(AvatarVisualsTreeSource::ROOT_NAME)
        );
        assert_eq!(state.selected_prim_path, Some(vec![1, 0, 2]));
        assert!(state.has_visuals_selection());
        assert!(state.pending_tree_focus, "focus request armed");

        // The tree widget mirrors the selection...
        let selected_id = GenNodeId::child(
            AvatarVisualsTreeSource::ROOT_NAME.to_string(),
            vec![1, 0, 2],
        );
        assert_eq!(state.tree_view_state.selected(), &vec![selected_id]);
        // ...and every ancestor (root, [1], [1,0]) is explicitly opened
        // so the picked row is actually visible.
        for depth in 0..3 {
            let ancestor = GenNodeId::child(
                AvatarVisualsTreeSource::ROOT_NAME.to_string(),
                vec![1, 0, 2][..depth].to_vec(),
            );
            assert_eq!(
                state.tree_view_state.is_open(&ancestor),
                Some(true),
                "ancestor at depth {depth} expanded"
            );
        }
    }
}
