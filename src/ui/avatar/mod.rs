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
//!   * **Publish to PDS** writes the current `LiveAvatarRecord` to the
//!     owner's PDS via `com.atproto.repo.putRecord` and then syncs the
//!     value into [`StoredAvatarRecord`] on success.
//!   * **Load from PDS** drops all in-flight edits by copying
//!     [`StoredAvatarRecord`] back into `LiveAvatarRecord`.
//!   * **Reset to default** replaces `LiveAvatarRecord` with the canonical
//!     [`AvatarRecord::default_for_did`] seed.

mod locomotion;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{self, AvatarRecord};
use crate::state::{
    LiveAvatarRecord, LiveInventoryRecord, LocalSettings, PublishFeedback, StoredAvatarRecord,
};
use crate::ui::room::RoomEditorState;
use crate::ui::room::generators::{AvatarVisualsTreeSource, GenNodeId, draw_generators_tab};

use locomotion::draw_locomotion_tab;

/// Async task for publishing the avatar record to the owner's PDS.
#[derive(Component)]
pub struct PublishAvatarTask(pub bevy::tasks::Task<Result<(), String>>);

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
}

impl AvatarEditorState {
    /// True when a visuals row is currently selected. The locomotion
    /// freeze gate and the gizmo dispatch read this.
    pub fn has_visuals_selection(&self) -> bool {
        self.selected_prim_path.is_some()
    }

    /// Drop the visuals selection — used when switching tabs, collapsing
    /// the editor window, or losing the mutex to the room editor.
    pub fn clear_visuals_selection(&mut self) {
        self.selected_generator = None;
        self.selected_prim_path = None;
        self.tree_view_state.set_selected(Vec::new());
    }
}

#[allow(clippy::too_many_arguments)]
pub fn avatar_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut live: ResMut<LiveAvatarRecord>,
    stored: Option<Res<StoredAvatarRecord>>,
    mut settings: ResMut<LocalSettings>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    mut feedback: ResMut<PublishFeedback>,
    mut inventory: Option<ResMut<LiveInventoryRecord>>,
    mut editor: ResMut<AvatarEditorState>,
    mut room_editor: Option<ResMut<RoomEditorState>>,
    mut gizmo_frame_pref: ResMut<crate::editor_gizmo::GizmoFramePref>,
    time: Res<Time>,
) {
    use crate::config::ui::airship as cfg;

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

    let window_visible_with_body = {
        let live_mut = live.bypass_change_detection();
        let before = live_mut.0.clone();

        let response = egui::Window::new("Avatar")
            .default_open(false)
            .default_pos(cfg::WINDOW_DEFAULT_POS)
            .default_width(cfg::WINDOW_DEFAULT_WIDTH)
            .resizable(true)
            .collapsible(true)
            .show(contexts.ctx_mut().unwrap(), |ui| {
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
                    crate::editor_gizmo::draw_gizmo_frame_toggle(ui, &mut gizmo_frame_pref);
                });
                ui.separator();

                // Reserve room below the tab body for the separator +
                // Publish/Load/Reset row + feedback line; the scroll
                // area then fills the rest of the window so dragging the
                // window taller actually grows the tab body.
                const FOOTER_RESERVE: f32 = 110.0;
                const BODY_MIN_HEIGHT: f32 = 200.0;
                let body_height = (ui.available_height() - FOOTER_RESERVE).max(BODY_MIN_HEIGHT);

                let AvatarEditorState {
                    selected_tab,
                    selected_generator,
                    selected_prim_path,
                    tree_view_state,
                    renaming_unused,
                    ..
                } = &mut *editor;

                match *selected_tab {
                    AvatarTab::Visuals => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            let mut source = AvatarVisualsTreeSource::new(&mut live_mut.0.visuals);
                            draw_generators_tab(
                                ui,
                                &mut source,
                                selected_generator,
                                selected_prim_path,
                                tree_view_state,
                                renaming_unused,
                                inventory.as_deref_mut(),
                                &mut widget_changed,
                            );
                        });
                    }
                    AvatarTab::Locomotion => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([true, false])
                            .max_height(body_height)
                            .show(ui, |ui| {
                                draw_locomotion_tab(
                                    ui,
                                    &mut live_mut.0.locomotion,
                                    &mut widget_changed,
                                );
                            });
                    }
                }

                ui.separator();

                // --- Networking (local-only, not broadcast) ---------------
                egui::CollapsingHeader::new("Networking")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.checkbox(
                            &mut settings.smooth_kinematics,
                            "Smooth remote peers (Hermite spline + 100 ms buffer)",
                        );
                        ui.label(
                            egui::RichText::new(
                                "Uncheck to snap to the latest packet and expose raw jitter.",
                            )
                            .small()
                            .weak(),
                        );
                    });

                ui.separator();

                // --- Publish / Load from PDS / Reset to default -----------
                let is_dirty = stored.as_ref().is_some_and(|s| s.0 != live_mut.0);

                ui.horizontal(|ui| {
                    let publish_button = egui::Button::new(
                        egui::RichText::new("Publish to PDS").color(if is_dirty {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            egui::Color32::GRAY
                        }),
                    );
                    let publish_enabled = is_dirty && session.is_some() && refresh_ctx.is_some();
                    if ui.add_enabled(publish_enabled, publish_button).clicked()
                        && let (Some(session), Some(refresh)) =
                            (session.as_ref(), refresh_ctx.as_ref())
                    {
                        *feedback = PublishFeedback::Publishing;
                        spawn_publish_avatar_task(
                            &mut commands,
                            session,
                            refresh,
                            live_mut.0.clone(),
                        );
                    }

                    if ui
                        .add_enabled(is_dirty, egui::Button::new("Load from PDS"))
                        .clicked()
                        && let Some(stored) = &stored
                    {
                        live_mut.0 = stored.0.clone();
                    }

                    let default_record = session
                        .as_ref()
                        .map(|s| AvatarRecord::default_for_did(&s.did));
                    let reset_enabled = default_record.as_ref().is_some_and(|d| *d != live_mut.0);
                    if ui
                        .add_enabled(reset_enabled, egui::Button::new("Reset to default"))
                        .clicked()
                        && let Some(default_record) = default_record
                    {
                        live_mut.0 = default_record;
                    }
                });

                // --- Status line ------------------------------------------
                match &*feedback {
                    PublishFeedback::Idle => {}
                    PublishFeedback::Publishing => {
                        ui.label(egui::RichText::new("Publishing…").italics().weak());
                    }
                    PublishFeedback::Success { .. } => {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, "Published ✓");
                    }
                    PublishFeedback::Failed { message, .. } => {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("Publish failed: {message}"),
                        );
                    }
                }
            });

        if live_mut.0 != before {
            widget_changed = true;
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
    }
    if editor.pending_flush_secs > 0.0 {
        editor.pending_flush_secs = (editor.pending_flush_secs - time.delta_secs()).max(0.0);
        if editor.pending_flush_secs <= 0.0 {
            // Debounce drained — publish the accumulated edit to player
            // (visual rebuild) and `broadcast_avatar_state` (peer
            // preview) in a single change tick.
            live.set_changed();
        }
    }
}

fn spawn_publish_avatar_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: AvatarRecord,
) {
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
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
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(PublishAvatarTask(task));
}

/// Poll outstanding avatar publish tasks. On success, sync `LiveAvatarRecord`
/// into `StoredAvatarRecord` so the "Load from PDS" button is disabled until the
/// next edit.
pub fn poll_publish_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishAvatarTask)>,
    live: Res<LiveAvatarRecord>,
    mut stored: Option<ResMut<StoredAvatarRecord>>,
    mut feedback: ResMut<PublishFeedback>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(()) => {
                info!("Avatar record saved to PDS");
                if let Some(stored) = stored.as_mut() {
                    stored.0 = live.0.clone();
                }
                *feedback = PublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to save avatar record: {}", e);
                *feedback = PublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}
