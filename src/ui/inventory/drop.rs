//! Drag-and-drop handler that consumes a [`PendingGeneratorDrop`] on
//! mouse release: routes the drag into a peer-gift `ItemOffer`, an
//! inventory→room placement, or a room→room placement depending on
//! whether the cursor was over a peer row in the People panel and which
//! [`DropSource`] armed the drag.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::pds::{Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::protocol::OverlandsMessage;
use crate::state::{CurrentRoomDid, DiagnosticsLog, LiveInventoryRecord, PendingOutgoingOffers};
use crate::terrain::TerrainMesh;
use crate::ui::room::RoomEditorState;

use super::{DropSource, PendingGeneratorDrop, is_drop_placeable};

/// Drop handler. Runs every frame in `InGame`; cheap-out early unless a drag
/// has been armed via [`PendingGeneratorDrop`].
///
/// Three destinations share this path (see [`DropSource`]):
/// * Release over a peer row in the People window → routes into an
///   [`OverlandsMessage::ItemOffer`]. The target is resolved from
///   `pending.peer_target`, refreshed every frame by the People UI.
/// * [`DropSource::Inventory`] release over the 3D viewport — copy the
///   blueprint into `RoomRecord.generators` under a collision-safe key
///   (reusing an identical existing entry when present) and append a
///   `Placement::Absolute`.
/// * [`DropSource::RoomGenerators`] release over the 3D viewport — the
///   source generator is already a key in `RoomRecord.generators`; just
///   append a new `Placement::Absolute` that references it.
///
/// The peer branch fires even when the local user does not own the current
/// room — gifting is a personal transaction between two players and
/// doesn't touch the `RoomRecord`. The ground-placement branches still
/// enforce `session.did == room_did` so a malicious state transition
/// mid-drag (e.g. a portal that swaps the room DID) cannot mutate a room
/// the user doesn't own.
///
/// The handler clears `pending.generator_name` and `pending.peer_target`
/// on every exit path so the next drag starts clean.
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
    mut pending_offers: ResMut<PendingOutgoingOffers>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    time: Res<Time>,
) {
    let Some(name) = pending.generator_name.clone() else {
        return;
    };
    let source = pending.source;
    let peer_target = pending.peer_target.clone();

    // The drag only commits on the frame the button is released. Every other
    // frame the mouse is either still held (drag in progress — we want to
    // keep the pending slot armed) or already up without a just-released
    // edge (stale state — clear it so a future drag starts clean).
    if mouse.just_released(MouseButton::Left) {
        // Fall through to the placement path below; release clears `pending`.
    } else if !mouse.pressed(MouseButton::Left) {
        pending.generator_name = None;
        pending.peer_target = None;
        return;
    } else {
        return;
    }

    // From here on, whatever happens, consume the pending slot so we don't
    // re-enter the placement path on the next frame.
    pending.generator_name = None;
    pending.peer_target = None;

    // -----------------------------------------------------------------
    // Peer gift branch: if the People UI flagged a peer row under the
    // cursor last frame, route the drag into an ItemOffer instead of a
    // ground placement. This takes precedence over the egui-cancel check
    // below because releasing over the People window IS the valid target
    // here — the usual "released over egui = cancel" rule doesn't apply.
    // -----------------------------------------------------------------
    if let Some(target) = peer_target {
        // The sender still needs the source `Generator` in hand; pull it
        // from the same sources the ground-placement branches use so
        // both drop origins can gift.
        let Some(sess) = session.as_deref() else {
            return;
        };
        let generator_opt = match source {
            DropSource::Inventory => inventory
                .as_ref()
                .and_then(|inv| inv.0.generators.get(&name).cloned()),
            DropSource::RoomGenerators => room
                .as_deref()
                .and_then(|r| r.generators.get(&name).cloned()),
        };
        let Some(generator) = generator_opt else {
            warn!("Peer-gift drop: source generator '{}' not found", name);
            return;
        };
        if !is_drop_placeable(&generator) {
            // Same filter the UI uses to gate drag-start; defence-in-depth
            // in case a non-placeable kind slipped through.
            return;
        }

        let now = time.elapsed_secs_f64();
        let offer_id =
            pending_offers.register(target.did.clone(), target.handle.clone(), name.clone(), now);
        writer.write(Broadcast {
            payload: OverlandsMessage::item_offer(
                offer_id,
                target.did.clone(),
                name.clone(),
                &generator,
            ),
            channel: ChannelKind::Reliable,
        });
        diagnostics.push(
            now,
            format!(
                "Offered \"{}\" to @{} — awaiting response",
                name, target.handle
            ),
        );
        info!(
            "Sent ItemOffer #{} \"{}\" to @{} ({})",
            offer_id, name, target.handle, target.did
        );
        // `editor_state` is untouched on this branch (gifts don't mutate
        // the room record), and `sess` served its purpose as a session
        // presence guard — silence the unused warnings without sprinkling
        // `#[allow]` attributes across the function.
        let _ = &mut editor_state;
        let _ = sess;
        return;
    }

    // -----------------------------------------------------------------
    // Ground-placement branch from here on. Enforces room ownership and
    // requires a clean viewport release.
    // -----------------------------------------------------------------

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
