//! Personal inventory stash UI.
//!
//! The Inventory window lists every `Generator` the owner has tucked aside
//! across editing sessions. Each entry can be renamed in place via a modal,
//! removed, or kept — and once the stash diverges from the PDS-persisted
//! copy, "Publish to PDS" pushes the new state through
//! `com.atproto.repo.putRecord`. The stash is loaded during `AppState::Loading`
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

mod drop;

pub use drop::handle_generator_drop;

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{Generator, GeneratorKind};
use crate::state::{
    CurrentRoomDid, InventoryPublishFeedback, LiveInventoryRecord, StoredInventoryRecord,
};

/// Persistent UI-only state for the Inventory window. Held in a `Local` so
/// it lives for the lifetime of the system without polluting the global
/// resource table.
#[derive(Default)]
pub struct InventoryEditorState {
    /// Active rename modal: `(original_key, draft_key)`.
    pub renaming_generator: Option<(String, String)>,
}

/// Async task for publishing the inventory record to the owner's PDS.
#[derive(Component)]
pub struct PublishInventoryTask(pub bevy::tasks::Task<Result<(), String>>);

/// Origin of a drag-to-place operation. The raycast + placement path is
/// identical for both sources; only the generator lookup differs — inventory
/// drops copy a blueprint into the room's `generators` map under a
/// collision-safe key, while room-editor drops reuse the existing key.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum DropSource {
    #[default]
    Inventory,
    RoomGenerators,
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
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut live: Option<ResMut<LiveInventoryRecord>>,
    stored: Option<Res<StoredInventoryRecord>>,
    mut feedback: ResMut<InventoryPublishFeedback>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    mut state: Local<InventoryEditorState>,
    time: Res<Time>,
) {
    let (Some(live), Some(stored), Some(session), Some(refresh_ctx)) =
        (live.as_mut(), stored, session, refresh_ctx)
    else {
        return;
    };
    // Drag-to-place is only valid in rooms the signed-in user owns. In other
    // rooms the rows still render (the owner may just be browsing their stash)
    // but we skip the drag affordance so a release over the viewport can't
    // mutate a `RoomRecord` that doesn't belong to us.
    let can_drag_place = room_did
        .as_ref()
        .map(|r| r.0 == session.did)
        .unwrap_or(false);

    // `InventoryRecord` lacks `PartialEq`, so we diff through serde_json —
    // identical JSON means identical contents for our purposes. The two
    // values are small (at most 50 generators) so this is cheap per frame.
    let mut is_dirty = serde_json::to_value(&live.0).ok() != serde_json::to_value(&stored.0).ok();
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
            is_dirty = true;
        }
        if close {
            state.renaming_generator = None;
        } else {
            state.renaming_generator = Some((old_name, new_name));
        }
    }

    egui::Window::new("Inventory")
        .default_open(false)
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
                            if can_drag_place && is_placeable {
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
                                    // owner oriented while they hunt for a
                                    // ground spot — without it, the drag is
                                    // invisible once the pointer leaves the
                                    // row.
                                    egui::Tooltip::always_open(
                                        ui.ctx().clone(),
                                        ui.layer_id(),
                                        egui::Id::new(("inv_drag_tip", &name)),
                                        egui::PopupAnchor::Pointer,
                                    )
                                    .show(|ui| {
                                        ui.label(format!("Place “{name}”"));
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
                        is_dirty = true;
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                let publish_button =
                    egui::Button::new(egui::RichText::new("Publish to PDS").color(if is_dirty {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::GRAY
                    }));

                if ui.add_enabled(is_dirty, publish_button).clicked() {
                    *feedback = InventoryPublishFeedback::Publishing;

                    let session_clone = session.clone();
                    let refresh_clone = refresh_ctx.clone();
                    let record_clone = live.0.clone();
                    let pool = bevy::tasks::IoTaskPool::get();
                    let task = pool.spawn(async move {
                        let fut = async {
                            let client = crate::config::http::default_client();
                            crate::pds::publish_inventory_record(
                                &client,
                                &session_clone,
                                &refresh_clone,
                                &record_clone,
                            )
                            .await
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
                    commands.spawn(PublishInventoryTask(task));
                }
            });

            match feedback.as_ref() {
                InventoryPublishFeedback::Idle => {}
                InventoryPublishFeedback::Publishing => {
                    ui.colored_label(egui::Color32::from_rgb(220, 200, 80), "⟳ Publishing…");
                }
                InventoryPublishFeedback::Success { at_secs } => {
                    ui.colored_label(
                        egui::Color32::from_rgb(80, 200, 120),
                        format!(
                            "✓ Saved ({:.0}s ago)",
                            (time.elapsed_secs_f64() - at_secs).max(0.0)
                        ),
                    );
                }
                InventoryPublishFeedback::Failed { at_secs, message } => {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 90, 90),
                        format!(
                            "✗ Failed ({:.0}s ago): {}",
                            (time.elapsed_secs_f64() - at_secs).max(0.0),
                            message
                        ),
                    );
                }
            }
        });
}

pub fn poll_publish_inventory_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishInventoryTask)>,
    live: Option<Res<LiveInventoryRecord>>,
    mut stored: Option<ResMut<StoredInventoryRecord>>,
    mut feedback: ResMut<InventoryPublishFeedback>,
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
                info!("Inventory record saved to PDS");
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.0.clone();
                }
                *feedback = InventoryPublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to save inventory record: {}", e);
                *feedback = InventoryPublishFeedback::Failed {
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
