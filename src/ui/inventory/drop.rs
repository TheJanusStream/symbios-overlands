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

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::{Fp3, Fp4, Generator, Placement, TransformData};
use crate::protocol::OverlandsMessage;
use crate::state::{CurrentRoomDid, LiveInventoryRecord, LiveRoomRecord, PendingOutgoingOffers};
use crate::terrain::TerrainMesh;

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
/// * [`DropSource::Catalogue`] release over the 3D viewport — resolve
///   the slug and stamp a fresh deep-copied blueprint the same way.
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
    mut room: Option<ResMut<LiveRoomRecord>>,
    mut pending_offers: ResMut<PendingOutgoingOffers>,
    mut session_log: ResMut<SessionLog>,
    mut sender: SendMessage<OverlandsMessage>,
    mut chunk: crate::network::chunk::ChunkSend,
    // Bundled to stay under Bevy's 16-parameter ceiling.
    (time, keyboard, mut toasts, mut undo_labels): (
        Res<Time>,
        Res<ButtonInput<KeyCode>>,
        ResMut<crate::ui::toast::Toasts>,
        ResMut<crate::ui::undo::PendingUndoLabels>,
    ),
) {
    let Some(name) = pending.generator_name.clone() else {
        return;
    };
    let source = pending.source;
    let peer_target = pending.peer_target.clone();

    // Escape disarms the drag mid-flight (#831) — before this the only
    // ways out were releasing over a window or over the sky.
    if keyboard.just_pressed(KeyCode::Escape) {
        pending.generator_name = None;
        pending.peer_target = None;
        return;
    }

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
            DropSource::Catalogue => {
                crate::catalogue::by_slug(&name).map(|entry| entry.build(&sess.did))
            }
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
        // Route through the chunker (#717): a gifted `Generator` can be a
        // large Shape-grammar / L-system blueprint whose `generator_json`
        // pushes the offer past the 64 KiB WebRTC message ceiling, which would
        // otherwise fail silently and leave the recipient without the gift.
        chunk.broadcast(
            &mut sender,
            &mut session_log,
            now,
            OverlandsMessage::item_offer(offer_id, target.did.clone(), name.clone(), &generator),
        );
        session_log.info(
            now,
            EventPayload::ItemOfferSent {
                offer_id,
                target_did: target.did.clone(),
                item_name: name.clone(),
            },
        );
        // Sender-side feedback (#843): releasing on a peer row used to
        // confirm NOTHING — the offer's whole lifecycle lived in the
        // diagnostics log.
        toasts.success(
            format!("Offer sent to @{} — \"{}\".", target.handle, name),
            now,
        );
        info!(
            "Sent ItemOffer #{} \"{}\" to @{} ({})",
            offer_id, name, target.handle, target.did
        );
        // `sess` served its purpose as a session presence guard — silence
        // the unused warning without sprinkling `#[allow]` across the fn.
        let _ = sess;
        return;
    }

    // -----------------------------------------------------------------
    // Ground-placement branch from here on. Enforces room ownership and
    // requires a clean viewport release.
    // -----------------------------------------------------------------

    // Releasing over any egui area (notably the Inventory window itself) is
    // the standard "cancel" gesture — treat it as a silent no-op instead of
    // placing a generator under the user's UI. Checked BEFORE the ownership
    // gate (#831) so a visitor's deliberate window-cancel never draws the
    // "you can't place here" toast below.
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.is_pointer_over_area() {
        return;
    }

    // All gate checks run *before* we touch `room` mutably. `ResMut::as_mut()`
    // unconditionally flips the resource's change tick, so taking the mutable
    // borrow earlier would spam `RoomRecord::is_changed` (and therefore the
    // world rebuild + peer broadcast) every time a drag is cancelled.
    let (Some(session), Some(room_did)) = (session, room_did) else {
        return;
    };
    if session.did != room_did.0 {
        // A true viewport release in a room the user doesn't own — the
        // drag used to just vanish (#831).
        toasts.warn(
            "Only the overland's owner can place items here — drop on a \
             peer in the People list to gift instead.",
            time.elapsed_secs_f64(),
        );
        return;
    }
    // Inventory-sourced drops need the live stash to pull the
    // blueprint from; room-generator drops don't touch the inventory
    // resource at all; catalogue drops resolve a slug against the
    // code-shipped registry and never look at the inventory.
    let inventory = match source {
        DropSource::Inventory => {
            let Some(inv) = inventory else {
                return;
            };
            Some(inv)
        }
        DropSource::Catalogue => None,
    };

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
        // Sky / out-of-world release: indistinguishable from a successful
        // off-screen placement without this (#831).
        toasts.info(
            "Released over open sky — nothing placed.",
            time.elapsed_secs_f64(),
        );
        return;
    };
    let hit_point = ray.origin + *ray.direction * hit.distance;

    let Some(record) = room.as_mut() else {
        return;
    };
    // `room.as_mut()` already tripped the `LiveRoomRecord` change tick
    // (the late borrow above is deliberate — see the gate-order comment);
    // unwrap to the inner record for the field writes below.
    let record = &mut record.0;

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
        DropSource::Catalogue => {
            // `name` carries the catalogue entry's slug. Resolve to
            // its trait object and build a fresh generator tree on
            // every drop — the catalogue is a stamp library, not a
            // reference type, so each placement is fully
            // independent. Drops referencing an unknown slug (e.g.
            // pending state from a build that removed the entry)
            // silently no-op.
            let Some(entry) = crate::catalogue::by_slug(&name) else {
                return;
            };
            let generator = entry.build(&session.did);
            if !is_drop_placeable(&generator) {
                return;
            }
            let key = choose_room_generator_key(&record.generators, &name, &generator);
            record.generators.entry(key.clone()).or_insert(generator);
            key
        }
    };

    record.placements.push(Placement::Absolute {
        generator_ref: gen_key.clone(),
        transform: TransformData {
            translation: Fp3([hit_point.x, hit_point.y, hit_point.z]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        avoid_water: false,
        avoid_water_clearance: crate::pds::Fp(0.0),
        snap_to_terrain: false,
    });
    undo_labels.set_room(format!("drop of {gen_key}"));
    info!(
        "Placed generator '{}' (as '{}') from {:?} at ({:.2}, {:.2}, {:.2})",
        name, gen_key, source, hit_point.x, hit_point.y, hit_point.z
    );
    // No dirty flag to set — the World Editor derives "dirty" from
    // `records_differ(stored, live)`, and the push above mutated the
    // live record (the `room.as_mut()` borrow already set its change
    // tick, driving the recompile + peer broadcast).
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

/// Live ground preview for an armed drag (#831): a footprint ring + post
/// at the exact spot a release would place the item, re-raycast every
/// frame — before this the drop point was invisible until commit (the
/// raycast ran only in the release path), so "released over sky", "landed
/// behind that building" and "placed 40 m downhill" all looked identical
/// mid-drag. Green = a release here places; red = the ground can't take
/// it (visiting someone else's overland — gift on a People row instead).
/// Over egui areas nothing draws: the follow-cursor tooltip is the
/// feedback there, and a release there is the cancel gesture.
#[allow(clippy::too_many_arguments)]
pub fn preview_generator_drop(
    mut contexts: EguiContexts,
    pending: Res<PendingGeneratorDrop>,
    mut gizmos: Gizmos,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    spatial: SpatialQuery,
    terrain_q: Query<Entity, With<TerrainMesh>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
) {
    use crate::config::ui::drop_preview as cfg;

    let Some(name) = pending.generator_name.as_ref() else {
        return;
    };
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
    // Terrain-only, exactly like the release path — the preview must
    // show where the commit raycast will actually land.
    let filter = SpatialQueryFilter::default();
    let hit = spatial.cast_ray_predicate(ray.origin, ray.direction, 4096.0, true, &filter, &|e| {
        terrain_q.get(e).is_ok()
    });
    let Some(hit) = hit else {
        return; // Sky under the cursor — nothing to mark.
    };
    let hit_point = ray.origin + *ray.direction * hit.distance;

    let owns_room = matches!(
        (session.as_deref(), room_did.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    let [r, g, b, a] = if owns_room {
        cfg::VALID_COLOR
    } else {
        cfg::INVALID_COLOR
    };
    let color = Color::srgba(r, g, b, a);

    // Footprint radius: catalogue entries carry a clearance; inventory
    // blueprints (and degenerate zero clearances) fall back to a small
    // visible ring.
    let radius = match pending.source {
        DropSource::Catalogue => crate::catalogue::by_slug(name)
            .map(|entry| entry.footprint().clearance)
            .filter(|c| *c > 0.0)
            .unwrap_or(cfg::DEFAULT_RADIUS_M),
        DropSource::Inventory => cfg::DEFAULT_RADIUS_M,
    };

    // Flat ring on the ground (the gizmo circle lies in its isometry's
    // XY plane; pitch it back to horizontal) plus a post so the spot
    // reads from a distance and against clutter.
    let ring_pos = hit_point + Vec3::Y * 0.05;
    gizmos.circle(
        Isometry3d::new(
            ring_pos,
            Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        ),
        radius,
        color,
    );
    gizmos.line(hit_point, hit_point + Vec3::Y * cfg::POST_HEIGHT_M, color);
}
