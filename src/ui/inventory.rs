//! Personal inventory stash UI.
//!
//! The Inventory window lists every `Generator` the owner has tucked aside
//! across editing sessions. Each entry can be renamed in place via a modal,
//! removed, or kept — and once the stash diverges from the PDS-persisted
//! copy, "Publish to PDS" pushes the new state through
//! `com.atproto.repo.putRecord`. The stash is loaded during `AppState::Loading`
//! (see `main.rs::start_inventory_record_fetch`) so a freshly-logged-in
//! owner always sees the previously-saved items the moment they land in-game.
//!
//! `InventoryRecord` does not derive `PartialEq` (the underlying `Generator`
//! enum doesn't either, because its variants carry types that themselves
//! would need full equality), so the dirty check round-trips through
//! `serde_json` — same pattern the world editor uses for its Reset button.
//!
//! Drag-to-place: each row is a drag source. When the owner releases a drag
//! over the 3D viewport while standing in their own room, [`handle_generator_drop`]
//! raycasts against the terrain and appends a fresh `Placement::Absolute` to
//! the live `RoomRecord`, copying the dragged generator into the room's
//! `generators` map on first use. The same handler also services drags
//! started from the World Editor's Generators tab — see [`DropSource`].

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use std::collections::HashMap;

use crate::pds::{Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::state::{
    CurrentRoomDid, InventoryPublishFeedback, LiveInventoryRecord, StoredInventoryRecord,
};
use crate::terrain::TerrainMesh;
use crate::ui::room::RoomEditorState;

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
/// on drag-start; [`handle_generator_drop`] consumes it on mouse release,
/// runs the raycast, and clears it — whether or not the release landed on a
/// valid ground hit.
#[derive(Resource, Default)]
pub struct PendingGeneratorDrop {
    pub generator_name: Option<String>,
    pub source: DropSource,
}

#[allow(clippy::too_many_arguments)]
pub fn inventory_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut live: Option<ResMut<LiveInventoryRecord>>,
    stored: Option<Res<StoredInventoryRecord>>,
    mut feedback: ResMut<InventoryPublishFeedback>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    mut state: Local<InventoryEditorState>,
    time: Res<Time>,
) {
    let (Some(live), Some(stored), Some(session)) = (live.as_mut(), stored, session) else {
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
            ui.label(format!("Stored Generators: {}/50", live.0.generators.len()));
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
                    let record_clone = live.0.clone();
                    let pool = bevy::tasks::IoTaskPool::get();
                    let task = pool.spawn(async move {
                        let fut = async {
                            let client = crate::config::http::default_client();
                            crate::pds::publish_inventory_record(
                                &client,
                                &session_clone,
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

/// Which `Generator` kinds can be point-placed via drag-and-drop.
/// Terrain + water describe whole-room scope (one heightmap / one water
/// plane) so a ground-level placement of them is nonsensical; they stay
/// editable via the World Editor tabs.
pub fn is_drop_placeable(generator: &Generator) -> bool {
    !matches!(
        generator,
        Generator::Terrain(_) | Generator::Water { .. } | Generator::Unknown
    )
}

/// Pick a key under which to store a dropped generator in the room's
/// `generators` map. Reuses the existing entry when its contents already
/// match the dragged blueprint (so repeated drops of the same inventory
/// item all share one room generator), and otherwise falls back to a
/// `_2`, `_3`, … suffix to avoid clobbering a pre-existing generator that
/// happens to share the inventory name.
///
/// Equality is checked through `serde_json::to_value` because `Generator`
/// doesn't derive `PartialEq` — same pattern the inventory's dirty diff uses.
fn choose_room_generator_key(
    existing: &HashMap<String, Generator>,
    inventory_name: &str,
    new_gen: &Generator,
) -> String {
    if let Some(existing_gen) = existing.get(inventory_name) {
        if serde_json::to_value(existing_gen).ok() == serde_json::to_value(new_gen).ok() {
            return inventory_name.to_string();
        }
    } else {
        return inventory_name.to_string();
    }
    for i in 2u32..u32::MAX {
        let candidate = format!("{inventory_name}_{i}");
        if !existing.contains_key(&candidate) {
            return candidate;
        }
    }
    inventory_name.to_string()
}

/// Drop handler. Runs every frame in `InGame`; cheap-out early unless a drag
/// has been armed via [`PendingGeneratorDrop`].
///
/// Two origin cases share this path (see [`DropSource`]):
/// * [`DropSource::Inventory`] — the source blueprint lives in the owner's
///   `LiveInventoryRecord`; we copy it into `RoomRecord.generators` under a
///   collision-safe key (reusing an identical existing entry when present).
/// * [`DropSource::RoomGenerators`] — the source generator is already a key
///   in `RoomRecord.generators`; we just append a new `Placement::Absolute`
///   that references it.
///
/// In both cases the system:
/// 1. Waits for the left mouse button to be released.
/// 2. Rejects the release if the cursor is over any egui area (so releasing
///    back onto the originating window simply cancels the drag).
/// 3. Raycasts the cursor into the world and accepts only hits against the
///    `TerrainMesh` entity, so the player chassis and other colliders can't
///    serve as drop targets.
/// 4. Appends a `Placement::Absolute` at the hit point with identity rotation
///    and `snap_to_terrain: false` — the recorded Y equals the raycast's
///    exact ground height, so re-snapping would only add jitter.
/// 5. Clears the pending slot on every exit path (release outside the
///    viewport, missed raycast, disallowed generator type) so the next drag
///    starts clean.
///
/// Owner gating is enforced twice — once on arm (in the UI) and once here on
/// release — so a malicious state transition (e.g. room DID changes mid-drag
/// via a portal) cannot end up mutating a room the user doesn't own.
#[allow(clippy::too_many_arguments)]
pub fn handle_generator_drop(
    mut contexts: EguiContexts,
    mut pending: ResMut<PendingGeneratorDrop>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    spatial: SpatialQuery,
    terrain_q: Query<Entity, With<TerrainMesh>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    inventory: Option<Res<LiveInventoryRecord>>,
    mut room: Option<ResMut<RoomRecord>>,
    mut editor_state: ResMut<RoomEditorState>,
) {
    let Some(name) = pending.generator_name.clone() else {
        return;
    };
    let source = pending.source;

    // The drag only commits on the frame the button is released. Every other
    // frame the mouse is either still held (drag in progress — we want to
    // keep the pending slot armed) or already up without a just-released
    // edge (stale state — clear it so a future drag starts clean).
    if mouse.just_released(MouseButton::Left) {
        // Fall through to the placement path below; release clears `pending`.
    } else if !mouse.pressed(MouseButton::Left) {
        pending.generator_name = None;
        return;
    } else {
        return;
    }

    // From here on, whatever happens, consume the pending slot so we don't
    // re-enter the placement path on the next frame.
    pending.generator_name = None;

    // All gate checks run *before* we touch `room` mutably. `ResMut::as_mut()`
    // unconditionally flips the resource's change tick, so taking the mutable
    // borrow earlier would spam `RoomRecord::is_changed` (and therefore the
    // world rebuild + peer broadcast) every time a drag is cancelled.
    let (Some(session), Some(room_did)) = (session, room_did) else {
        return;
    };
    if session.did != room_did.0 {
        return;
    }
    // Inventory-sourced drops need the live stash to pull the blueprint from;
    // room-generator drops don't touch the inventory resource at all.
    let inventory = match source {
        DropSource::Inventory => {
            let Some(inv) = inventory else {
                return;
            };
            Some(inv)
        }
        DropSource::RoomGenerators => None,
    };

    // Releasing over any egui area (notably the Inventory window itself) is
    // the standard "cancel" gesture — treat it as a no-op instead of placing
    // a generator under the user's UI.
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.is_pointer_over_area() {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_tf)) = cameras.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else {
        return;
    };

    // Only accept hits on the terrain. Other colliders (player chassis,
    // remote peers, prop sensors) are closer than the terrain in many
    // viewing angles and would otherwise "catch" the drop at waist height.
    let filter = SpatialQueryFilter::default();
    let hit = spatial.cast_ray_predicate(ray.origin, ray.direction, 4096.0, true, &filter, &|e| {
        terrain_q.get(e).is_ok()
    });
    let Some(hit) = hit else {
        return;
    };
    let hit_point = ray.origin + *ray.direction * hit.distance;

    let Some(record) = room.as_mut() else {
        return;
    };

    let gen_key = match source {
        DropSource::Inventory => {
            let Some(inv) = inventory.as_ref() else {
                return;
            };
            let Some(generator) = inv.0.generators.get(&name).cloned() else {
                return;
            };
            if !is_drop_placeable(&generator) {
                return;
            }
            let key = choose_room_generator_key(&record.generators, &name, &generator);
            record.generators.entry(key.clone()).or_insert(generator);
            key
        }
        DropSource::RoomGenerators => {
            let Some(generator) = record.generators.get(&name) else {
                return;
            };
            if !is_drop_placeable(generator) {
                return;
            }
            name.clone()
        }
    };

    record.placements.push(Placement::Absolute {
        generator_ref: gen_key.clone(),
        transform: TransformData {
            translation: Fp3([hit_point.x, hit_point.y, hit_point.z]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        snap_to_terrain: false,
    });
    info!(
        "Placed generator '{}' (as '{}') from {:?} at ({:.2}, {:.2}, {:.2})",
        name, gen_key, source, hit_point.x, hit_point.y, hit_point.z
    );
    editor_state.mark_dirty();
}
