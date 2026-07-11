//! Personal inventory stash UI.
//!
//! The Inventory window lists every `Generator` the owner has tucked aside
//! across editing sessions. Each entry can be renamed in place via a modal,
//! removed, or kept — and once the stash diverges from the PDS-persisted
//! copy, "Save to PDS" commits the live-vs-stored diff as one atomic
//! `com.atproto.repo.applyWrites` batch — one record per item (#696).
//! The stash is loaded during `AppState::Loading`
//! (see [`crate::loading::start_inventory_record_fetch`]) so a freshly-logged-in
//! owner always sees the previously-saved items the moment they land in-game.
//!
//! `InventoryRecord` does not derive `PartialEq` (the underlying `Generator`
//! enum doesn't either, because its variants carry types that themselves
//! would need full equality), so the dirty check round-trips through
//! `serde_json` — same pattern the world editor uses for its Reset button.
//!
//! Drag-to-place: each row is a drag source. When the owner releases a drag
//! over the 3D viewport while standing in their own room,
//! [`drop::handle_generator_drop`] raycasts against the terrain and appends
//! a fresh `Placement::Absolute` to the live `RoomRecord`, copying the
//! dragged generator into the room's `generators` map on first use. The
//! same handler also services drags started from the World Editor's
//! Generators tab — see [`DropSource`].
//!
//! Drag-to-gift: releasing the same drag over a peer row in the People
//! window routes it into an `ItemOffer` instead. Gifting works in ANY room
//! (#699) — only the ground-placement branch is owner-gated, and the drop
//! handler enforces that, not the drag source.

mod drop;

pub use drop::handle_generator_drop;

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, RecordKind};
use crate::pds::{Generator, GeneratorKind, InventoryRecord};
use crate::state::{
    CurrentRoomDid, LiveInventoryRecord, PublishFeedback, PublishStatus, StoredInventoryRecord,
    records_differ,
};
use crate::ui::editable::{RecordAction, publish_status_line, save_load_reset_row};

/// Persistent UI-only state for the Inventory window. Held in a `Local` so
/// it lives for the lifetime of the system without polluting the global
/// resource table.
#[derive(Default)]
pub struct InventoryEditorState {
    /// Active rename modal: `(original_key, draft_key)`.
    pub renaming_generator: Option<(String, String)>,
}

/// Async task for publishing the inventory record to the owner's PDS. Carries
/// the target `did` + dispatch time so [`poll_publish_inventory_tasks`] can emit
/// a typed `RecordWrite*` session event (with the write's duration) on resolve.
#[derive(Component)]
pub struct PublishInventoryTask {
    pub task: bevy::tasks::Task<Result<(), String>>,
    pub did: String,
    pub spawned_at: f64,
    /// Serialized size of the record being written, measured at dispatch so
    /// the poll system can gauge + log it (#694).
    pub record_bytes: Option<usize>,
}

/// Origin of a drag-to-place operation. The raycast + placement path
/// is identical for every source; only the generator lookup differs.
/// Inventory drops copy a blueprint into the room's `generators` map
/// under a collision-safe key; room-editor drops reuse the existing
/// key; catalogue drops resolve the slug against
/// [`crate::catalogue::by_slug`] and stamp a fresh deep-copied
/// blueprint into the room's `generators` map.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum DropSource {
    #[default]
    Inventory,
    RoomGenerators,
    Catalogue,
}

/// Egui → world drag handoff. The UI side sets the generator name and source
/// on drag-start; [`drop::handle_generator_drop`] consumes it on mouse release,
/// runs the raycast, and clears it — whether or not the release landed on a
/// valid ground hit.
///
/// `peer_target` is refreshed every frame by [`crate::ui::people::people_ui`]
/// while a drag is active: it is set to the peer whose row the cursor is
/// currently over (or cleared when the cursor isn't over a peer row). The
/// drop handler consumes it on release to route the drag into an
/// [`crate::protocol::OverlandsMessage::ItemOffer`] instead of a terrain
/// placement. It intentionally is **not** cleared by the inventory or
/// world-editor drag source on its own — the People UI owns the signal
/// because only it can resolve "cursor is over peer row N" via egui's
/// layout.
#[derive(Resource, Default)]
pub struct PendingGeneratorDrop {
    pub generator_name: Option<String>,
    pub source: DropSource,
    pub peer_target: Option<PeerDropTarget>,
}

/// Per-frame hover snapshot for the peer the cursor is currently over
/// during an armed drag. Populated by the People GUI so the drop handler
/// can route release events without reaching into egui itself.
#[derive(Clone, Debug)]
pub struct PeerDropTarget {
    pub peer_id: bevy_symbios_multiuser::prelude::PeerId,
    pub did: String,
    pub handle: String,
}

#[allow(clippy::too_many_arguments)]
pub fn inventory_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut live: Option<ResMut<LiveInventoryRecord>>,
    stored: Option<Res<StoredInventoryRecord>>,
    mut feedback: ResMut<PublishFeedback<InventoryRecord>>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    mut state: Local<InventoryEditorState>,
    time: Res<Time>,
) {
    let (Some(live), Some(stored), Some(session), Some(refresh_ctx)) =
        (live.as_mut(), stored, session, refresh_ctx)
    else {
        return;
    };
    // Rows are draggable everywhere: releasing over a peer row in the People
    // window offers the item as a gift, which is a personal transaction and
    // valid in ANY room (#699 — gating the drag on room ownership locked
    // visitors out of gifting entirely). Ground placement is the drop
    // handler's job to police: [`drop::handle_generator_drop`] only mutates
    // the `RoomRecord` when `session.did == room_did`, so a viewport release
    // in someone else's room is a no-op. Ownership here only tunes the drag
    // tooltip so a visitor isn't promised a placement that can't happen.
    let owns_room = room_did
        .as_ref()
        .map(|r| r.0 == session.did)
        .unwrap_or(false);

    let ctx = contexts.ctx_mut().unwrap();

    // Rename modal — independent top-level egui Window so it floats above
    // the Inventory window. Same shape as the world editor's rename flow.
    if let Some((old_name, mut new_name)) = state.renaming_generator.clone() {
        let mut close = false;
        let mut apply = false;
        egui::Window::new("Rename Inventory Item")
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
            && !live.0.generators.contains_key(&new_name)
            && let Some(g) = live.0.generators.remove(&old_name)
        {
            live.0.generators.insert(new_name.clone(), g);
        }
        if close {
            state.renaming_generator = None;
        } else {
            state.renaming_generator = Some((old_name, new_name));
        }
    }

    egui::Window::new("Inventory")
        .open(&mut panels.inventory)
        .default_pos([390.0, 10.0])
        .default_size([300.0, 400.0])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            ui.label(format!(
                "Stored Generators: {}/{}",
                live.0.generators.len(),
                crate::config::state::MAX_INVENTORY_ITEMS
            ));
            ui.separator();

            // Reserve room below the list for the separator + Publish row +
            // feedback line; the scroll area then fills the rest of the
            // window so dragging the window taller actually grows the list.
            // Without this (and without `auto_shrink = false`) the scroll
            // area collapses to its content and the window height snaps back.
            const FOOTER_RESERVE: f32 = 80.0;
            const LIST_MIN_HEIGHT: f32 = 80.0;
            let list_height = (ui.available_height() - FOOTER_RESERVE).max(LIST_MIN_HEIGHT);

            egui::ScrollArea::vertical()
                .auto_shrink([true, false])
                .max_height(list_height)
                .show(ui, |ui| {
                    let mut to_remove: Option<String> = None;
                    let mut names: Vec<String> = live.0.generators.keys().cloned().collect();
                    names.sort();

                    for name in names {
                        ui.horizontal(|ui| {
                            // Generators that make no sense as a dropped
                            // placement (terrain + water are room-scoped, not
                            // point-placed) render as a plain label so the
                            // drag sense doesn't arm a release we'd ignore.
                            let is_placeable = live
                                .0
                                .generators
                                .get(&name)
                                .map(is_drop_placeable)
                                .unwrap_or(false);
                            if is_placeable {
                                let label =
                                    egui::Label::new(&name).sense(egui::Sense::click_and_drag());
                                let resp = ui.add(label);
                                if resp.drag_started() {
                                    pending_drop.generator_name = Some(name.clone());
                                    pending_drop.source = DropSource::Inventory;
                                }
                                if resp.dragged()
                                    && pending_drop.generator_name.as_deref() == Some(name.as_str())
                                {
                                    // Follow-the-cursor tooltip keeps the
                                    // dragger oriented while they hunt for a
                                    // target — without it, the drag is
                                    // invisible once the pointer leaves the
                                    // row. Visitors can only gift (ground
                                    // placement is owner-only), so say so.
                                    egui::Tooltip::always_open(
                                        ui.ctx().clone(),
                                        ui.layer_id(),
                                        egui::Id::new(("inv_drag_tip", &name)),
                                        egui::PopupAnchor::Pointer,
                                    )
                                    .show(|ui| {
                                        if owns_room {
                                            ui.label(format!(
                                                "Place “{name}” — or drop on a peer to offer it"
                                            ));
                                        } else {
                                            ui.label(format!(
                                                "Offer “{name}” — drop on a peer in the People list"
                                            ));
                                        }
                                    });
                                }
                            } else {
                                ui.label(&name);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new("−")
                                                .fill(egui::Color32::from_rgb(180, 50, 50)),
                                        )
                                        .clicked()
                                    {
                                        to_remove = Some(name.clone());
                                    }
                                    if ui.small_button("Rename").clicked() {
                                        state.renaming_generator =
                                            Some((name.clone(), name.clone()));
                                    }
                                },
                            );
                        });
                    }
                    if let Some(name) = to_remove {
                        live.0.generators.remove(&name);
                    }
                });

            ui.separator();

            // Shared Save / Load / Reset row + status line
            // (`ui::editable`), identical to the World and Avatar
            // editors. Dirty is derived (`records_differ` vs the stored
            // snapshot) so the row needs no per-edit flag; Inventory now
            // also gets Load-from-PDS (revert) and Reset-to-default
            // (empty the stash) — it previously had Publish only.
            let dirty = records_differ(&live.0, &stored.0);
            let default_record = InventoryRecord::default();
            let can_reset = records_differ(&live.0, &default_record);
            // `session` + `refresh_ctx` are guaranteed present (the early
            // return above bails otherwise), so a publish is always
            // attemptable while dirty.
            //
            // Size readout: the stash is one record PER ITEM (#696), so
            // the per-record budget applies to the largest single item —
            // not the whole stash. Same throttled cache as the other
            // editors, custom measurement.
            let now = time.elapsed_secs_f64();
            if feedback
                .live_bytes_at
                .is_none_or(|at| now - at >= crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS)
            {
                feedback.live_bytes = crate::pds::inventory::max_item_bytes(&live.0);
                feedback.live_bytes_at = Some(now);
            }
            let record_bytes = feedback.live_bytes;
            match save_load_reset_row(ui, dirty, true, can_reset, record_bytes) {
                RecordAction::None => {}
                RecordAction::Publish => {
                    feedback.status = PublishStatus::Publishing;
                    spawn_publish_inventory_task(
                        &mut commands,
                        &session,
                        &refresh_ctx,
                        live.0.clone(),
                        stored.0.clone(),
                        time.elapsed_secs_f64(),
                    );
                }
                RecordAction::Load => {
                    live.0 = stored.0.clone();
                }
                RecordAction::Reset => {
                    live.0 = default_record;
                }
            }

            publish_status_line(ui, &feedback.status, time.elapsed_secs_f64());
        });
}

/// Spawn the async inventory publish. Since #696 this commits the
/// live-vs-`stored` diff as per-item records in ONE atomic `applyWrites`
/// batch (see [`crate::pds::inventory`]), so the caller must pass the
/// stored snapshot the diff is computed against. `pub(crate)` because the
/// unsaved-edits guard ([`crate::ui::unsaved_guard`]) and the offer-accept
/// path ([`crate::ui::people`]) drive the same pipeline — the shared
/// [`poll_publish_inventory_tasks`] system lands the result either way.
pub(crate) fn spawn_publish_inventory_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: InventoryRecord,
    stored: InventoryRecord,
    now: f64,
) {
    // The inventory record is the local user's own, saved to their PDS → the
    // write DID is the session DID (like the avatar save).
    let did = session.did.clone();
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    // Per-item wire format → the budget gauge tracks the largest single
    // item record, not the whole stash (#694/#696).
    let record_bytes = crate::pds::inventory::max_item_bytes(&record);
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            crate::pds::publish_inventory_record(
                &client,
                &session_clone,
                &refresh_clone,
                &record,
                &stored,
            )
            .await
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
    commands.spawn(PublishInventoryTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
    });
}

#[allow(clippy::too_many_arguments)]
pub fn poll_publish_inventory_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishInventoryTask)>,
    live: Option<Res<LiveInventoryRecord>>,
    mut stored: Option<ResMut<StoredInventoryRecord>>,
    mut feedback: ResMut<PublishFeedback<InventoryRecord>>,
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
            RecordKind::Inventory,
            task.record_bytes,
        );
        match result {
            Ok(()) => {
                info!("Inventory record saved to PDS");
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.0.clone();
                }
                feedback.status = PublishStatus::Success { at_secs: now };
                session_log.info(
                    now,
                    EventPayload::RecordWriteCompleted {
                        record: RecordKind::Inventory,
                        did,
                        duration_secs,
                    },
                );
            }
            Err(e) => {
                warn!("Failed to save inventory record: {}", e);
                session_log.error(
                    now,
                    EventPayload::RecordWriteFailed {
                        record: RecordKind::Inventory,
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

/// Which generator kinds can be point-placed via drag-and-drop.
/// Terrain + water describe whole-room scope (one heightmap / one water
/// plane) so a ground-level placement of them is nonsensical; they stay
/// editable via the World Editor tabs.
pub fn is_drop_placeable(generator: &Generator) -> bool {
    !matches!(
        generator.kind,
        GeneratorKind::Terrain(_) | GeneratorKind::Water { .. } | GeneratorKind::Unknown
    )
}

/// Pick an inventory key for a gift arriving via [`crate::protocol::OverlandsMessage::ItemOffer`].
/// Policy: if the incoming name is free, use it verbatim; otherwise
/// append `_2`, `_3`, … until we find an unused slot. This matches the
/// user-approved design ("auto-rename with _2 suffix"). Equality of
/// existing entries is not consulted — a gift always lands as a new item,
/// because two players may each have tweaked the same base blueprint and
/// silently coalescing would lose data.
pub fn choose_inventory_gift_key(
    existing: &HashMap<String, Generator>,
    incoming_name: &str,
) -> String {
    if !existing.contains_key(incoming_name) {
        return incoming_name.to_string();
    }
    for i in 2u32..u32::MAX {
        let candidate = format!("{incoming_name}_{i}");
        if !existing.contains_key(&candidate) {
            return candidate;
        }
    }
    incoming_name.to_string()
}
