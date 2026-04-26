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
//! - **Publish to PDS** pushes the current `RoomRecord` to the owner's PDS
//!   via `com.atproto.repo.putRecord` and syncs the value into
//!   [`StoredRoomRecord`] on success.
//! - **Load from PDS** drops all in-flight edits by copying
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

mod construct;
mod environment;
mod generators;
mod lsystem;
mod material;
mod placements;
mod raw;
mod shape;
mod terrain;
mod widgets;

/// Compact material editor reused by `ui::avatar` for its hull / pontoon /
/// mast slots. Kept in [`construct`] alongside the Prim node material
/// widget it was factored out of; re-exported here so call sites reach it
/// as `super::room::draw_universal_material`.
pub(crate) use construct::draw_universal_material;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{self, Placement, RoomRecord};
use crate::state::{
    CurrentRoomDid, LiveInventoryRecord, PublishFeedback, RoomRecordRecovery, StoredRoomRecord,
};

/// Async task for publishing the room record to the owner's PDS.
#[derive(Component)]
pub struct PublishRoomTask(pub bevy::tasks::Task<Result<(), String>>);

/// Async task for the hard-reset publish path (delete-then-put). Separate
/// from `PublishRoomTask` only for logging clarity — the two share the same
/// result type and poll system.
#[derive(Component)]
pub struct ResetRoomTask(pub bevy::tasks::Task<Result<(), String>>);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorTab {
    #[default]
    Environment,
    Generators,
    Placements,
    Raw,
}

/// Stable identifier for one node in the unified generator tree. The pair
/// `(root, path)` walks from a top-level entry in `RoomRecord::generators`
/// (`root` is the HashMap key) into its `children` (`path` carries the
/// child-index chain). An empty `path` denotes the root node itself. The
/// tree-view widget keys its selection / expansion state on this type.
///
/// `Default` returns an `(empty-root, empty-path)` sentinel that stands
/// in for "the implicit virtual root" of the tree-view widget. Our
/// `unique_key` rejects empty prefixes, so a real generator can never
/// have `root == ""`; the sentinel is therefore unambiguous.
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
    raw_text: String,
    raw_text_initialised: bool,
    raw_error: Option<String>,
    /// True once a widget mutates the live record relative to the last
    /// committed / loaded / reset state — drives the Publish button
    /// colouring.
    is_dirty: bool,
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
}

impl RoomEditorState {
    /// Flag the editor as having uncommitted edits so the Publish and Load
    /// buttons light up. Used by out-of-band edit paths — the 3D gizmo
    /// commit, drag-drop placement from the Inventory — that mutate the
    /// live `RoomRecord` without going through the egui widget flow that
    /// normally sets [`Self::is_dirty`] via `widget_change`.
    ///
    /// Does not touch `pending_flush_secs`: those edit paths call
    /// `record.set_changed()` themselves at commit time, so the debounce
    /// timer that collapses slider drags has no work to do here.
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn room_admin_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut room_record: Option<ResMut<RoomRecord>>,
    stored: Option<Res<StoredRoomRecord>>,
    recovery: Option<Res<RoomRecordRecovery>>,
    mut editor: ResMut<RoomEditorState>,
    mut publish_feedback: ResMut<PublishFeedback>,
    mut inventory: Option<ResMut<LiveInventoryRecord>>,
    time: Res<Time>,
) {
    let (Some(session), Some(refresh_ctx), Some(room_did), Some(record)) =
        (session, refresh_ctx, room_did, room_record.as_mut())
    else {
        return;
    };

    // Security gate — only the owner may edit their own room.
    if session.did != room_did.0 {
        return;
    }

    if !editor.raw_text_initialised {
        editor.raw_text = serde_json::to_string_pretty(record.as_ref())
            .unwrap_or_else(|e| format!("// serialize error: {}", e));
        editor.raw_text_initialised = true;
    }

    // Destructure the Local into independent field borrows so the
    // borrow-checker can see that the tab-body closure and the commit-row
    // closure each touch *disjoint* subsets of the editor state. Without
    // this, re-borrowing `editor` inside nested egui closures trips E0499.
    let RoomEditorState {
        selected_tab,
        selected_generator,
        selected_placement,
        selected_prim_path,
        tree_view_state,
        raw_text,
        raw_error,
        is_dirty,
        pending_flush_secs,
        renaming_generator,
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
        let record_mut: &mut RoomRecord = record.bypass_change_detection();

        // Rename modal — rendered as an independent top-level egui Window so
        // it floats above the World Editor. Cloning the `(old, draft)` pair
        // out first lets us mutate the draft in a scratch variable and feed
        // the final decision back into `renaming_generator` without holding
        // a long-lived mutable borrow across the `egui::Window::show` call.
        if let Some((old_name, mut new_name)) = renaming_generator.clone() {
            let mut close = false;
            let mut apply = false;
            egui::Window::new("Rename Generator")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut new_name).request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            apply = true;
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });

            if apply
                && !new_name.is_empty()
                && !record_mut.generators.contains_key(&new_name)
                && let Some(g) = record_mut.generators.remove(&old_name)
            {
                record_mut.generators.insert(new_name.clone(), g);
                // Rewrite every Placement that referenced the old key so the
                // world compiler can still resolve its generator after the
                // rename. Unknown placements (forward-compat variants) stay
                // untouched because we can't see their generator_ref field.
                for p in record_mut.placements.iter_mut() {
                    match p {
                        Placement::Absolute { generator_ref, .. }
                        | Placement::Scatter { generator_ref, .. }
                        | Placement::Grid { generator_ref, .. } => {
                            if generator_ref == &old_name {
                                *generator_ref = new_name.clone();
                            }
                        }
                        Placement::Unknown => {}
                    }
                }
                // Migrate the traits mapping too — `RoomRecord::traits` is
                // keyed on generator name, so a rename without this step
                // orphans ECS trait bindings like `collider_heightfield`
                // and leaves the renamed generator with no collision.
                if let Some(traits) = record_mut.traits.remove(&old_name) {
                    record_mut.traits.insert(new_name.clone(), traits);
                }
                *selected_generator = Some(new_name.clone());
                // Tree-view ids are keyed on `(root, path)`, so the rename
                // also has to retarget the current selection at the new
                // root key — otherwise the tree highlights nothing while
                // the gizmo still tracks the renamed root.
                tree_view_state.set_one_selected(GenNodeId::root(new_name.clone()));
                widget_change = true;
            }
            if close {
                *renaming_generator = None;
            } else {
                *renaming_generator = Some((old_name, new_name));
            }
        }

        egui::Window::new("World Editor")
            .default_open(false)
            .collapsible(true)
            .resizable(true)
            .default_width(820.0)
            .default_height(620.0)
            .default_pos([580.0, 10.0])
            .show(ctx, |ui| {
                // Recovery banner — shown when the stored PDS record failed
                // to decode and we're running on the synthesised default.
                // Offers a one-click "Reset PDS to default" so the owner can
                // deliberately overwrite the incompatible record instead of
                // being stuck.
                if let Some(rec) = recovery.as_deref() {
                    let banner = egui::Frame::new()
                        .fill(egui::Color32::from_rgb(90, 30, 30))
                        .inner_margin(6.0)
                        .corner_radius(4.0);
                    banner.show(ui, |ui| {
                        ui.colored_label(
                            egui::Color32::WHITE,
                            "⚠ Stored room record is incompatible with this build.",
                        );
                        ui.label(format!("Decode error: {}", rec.reason));
                        ui.label(
                            "You are currently editing the default homeworld. Click below \
                             to overwrite the stored record on your PDS with this default \
                             so the next login loads cleanly.",
                        );
                        if ui.button("Reset PDS to default").clicked() {
                            let default_record = pds::RoomRecord::default_for_did(&room_did.0);
                            *record_mut = default_record.clone();
                            *raw_text =
                                serde_json::to_string_pretty(&default_record).unwrap_or_default();
                            *raw_error = None;
                            *is_dirty = false;
                            needs_broadcast = true;
                            // Use the delete-then-put reset path. The vanilla
                            // putRecord upsert can return 500 when the stored
                            // record is incompatible with the current lexicon;
                            // hard-deleting first sidesteps that failure mode.
                            spawn_reset_task(&mut commands, &session, &refresh_ctx, default_record);
                            commands.remove_resource::<RoomRecordRecovery>();
                        }
                    });
                    ui.add_space(6.0);
                }

                // Tab bar
                ui.horizontal(|ui| {
                    let tabs = [
                        (EditorTab::Environment, "Environment"),
                        (EditorTab::Generators, "Generators"),
                        (EditorTab::Placements, "Placements"),
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

                // The Generators tab paints its own SidePanel + CentralPanel
                // split, so it manages its own scrolls and bypasses the
                // outer ScrollArea that the simpler tabs share. Wrapping a
                // nested SidePanel inside an outer ScrollArea collapses the
                // sidebar's height to zero.
                match *selected_tab {
                    EditorTab::Generators => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            generators::draw_generators_tab(
                                ui,
                                record_mut,
                                selected_generator,
                                selected_prim_path,
                                tree_view_state,
                                renaming_generator,
                                inventory.as_deref_mut(),
                                &mut widget_change,
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
                                        &mut widget_change,
                                    );
                                }
                                EditorTab::Placements => {
                                    placements::draw_placements_tab(
                                        ui,
                                        record_mut,
                                        selected_placement,
                                        &mut widget_change,
                                    );
                                }
                                EditorTab::Raw => {
                                    raw::draw_raw_tab(
                                        ui,
                                        raw_text,
                                        raw_error,
                                        record_mut,
                                        &mut widget_change,
                                    );
                                }
                                EditorTab::Generators => unreachable!(),
                            });
                    }
                }

                ui.separator();

                // Publish / Load from PDS / Reset to default
                ui.horizontal(|ui| {
                    let publish_button = egui::Button::new(
                        egui::RichText::new("Publish to PDS").color(if *is_dirty {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            egui::Color32::GRAY
                        }),
                    );
                    if ui.add_enabled(*is_dirty, publish_button).clicked() {
                        let new_record = record_mut.clone();
                        *is_dirty = false;
                        *publish_feedback = PublishFeedback::Publishing;
                        spawn_publish_task(&mut commands, &session, &refresh_ctx, new_record);
                    }

                    let can_load = stored.is_some() && *is_dirty;
                    if ui
                        .add_enabled(can_load, egui::Button::new("Load from PDS"))
                        .clicked()
                        && let Some(stored) = stored.as_ref()
                    {
                        *record_mut = stored.0.clone();
                        *raw_text = serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                        *raw_error = None;
                        *is_dirty = false;
                        *selected_generator = None;
                        *selected_placement = None;
                        *selected_prim_path = None;
                        tree_view_state.set_selected(Vec::new());
                        needs_broadcast = true;
                    }

                    if ui.button("Reset to default").clicked() {
                        *record_mut = pds::RoomRecord::default_for_did(&room_did.0);
                        *raw_text = serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                        *raw_error = None;
                        *is_dirty = stored
                            .as_ref()
                            .map(|s| {
                                serde_json::to_value(&s.0).ok()
                                    != serde_json::to_value(&*record_mut).ok()
                            })
                            .unwrap_or(true);
                        *selected_generator = None;
                        *selected_placement = None;
                        *selected_prim_path = None;
                        tree_view_state.set_selected(Vec::new());
                        needs_broadcast = true;
                    }
                });

                // Publish status indicator. `Idle` stays silent; other states
                // render a coloured one-liner so the owner knows whether the
                // PDS round-trip actually landed without having to tail the
                // console.
                match publish_feedback.as_ref() {
                    PublishFeedback::Idle => {}
                    PublishFeedback::Publishing => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 200, 80),
                            "⟳ Publishing to PDS…",
                        );
                    }
                    PublishFeedback::Success { at_secs } => {
                        let ago = (time.elapsed_secs_f64() - at_secs).max(0.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            format!("✓ Published to PDS ({:.0}s ago)", ago),
                        );
                    }
                    PublishFeedback::Failed { at_secs, message } => {
                        let ago = (time.elapsed_secs_f64() - at_secs).max(0.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 90, 90),
                            format!("✗ Publish failed ({:.0}s ago): {}", ago, message),
                        );
                    }
                }
            });
    }

    if widget_change {
        *is_dirty = true;
        *pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
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
        record.set_changed();
    }
}

// ---------------------------------------------------------------------------
// Publish pipeline
// ---------------------------------------------------------------------------

fn spawn_publish_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: RoomRecord,
) {
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::publish_room_record(&client, &session_clone, &refresh_clone, &record).await
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
    commands.spawn(PublishRoomTask(task));
}

/// Spawn the hard-reset publish task — delete the stored record first, then
/// create a fresh one. Used by the recovery banner's "Reset PDS to default"
/// button, which has to work around PDS implementations that return 500 on
/// `putRecord` when the prior blob is schema-incompatible.
fn spawn_reset_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: RoomRecord,
) {
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::reset_room_record(&client, &session_clone, &refresh_clone, &record).await
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
    commands.spawn(ResetRoomTask(task));
}

/// Poll outstanding publish and reset tasks and log results. On success,
/// pin `StoredRoomRecord` to the live `RoomRecord` so subsequent "Load from
/// PDS" presses restore the now-committed state and the dirty indicator
/// resets.
pub fn poll_publish_tasks(
    mut commands: Commands,
    mut publish_tasks: Query<(Entity, &mut PublishRoomTask)>,
    mut reset_tasks: Query<(Entity, &mut ResetRoomTask)>,
    live: Option<Res<RoomRecord>>,
    mut stored: Option<ResMut<StoredRoomRecord>>,
    mut publish_feedback: ResMut<PublishFeedback>,
    time: Res<Time>,
) {
    for (entity, mut task) in publish_tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(()) => {
                info!("Room record saved to PDS");
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.as_ref().clone();
                }
                *publish_feedback = PublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to save room record: {}", e);
                *publish_feedback = PublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
    for (entity, mut task) in reset_tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(()) => {
                info!("Room record reset on PDS (delete + put)");
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.as_ref().clone();
                }
                *publish_feedback = PublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to reset room record: {}", e);
                *publish_feedback = PublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}
