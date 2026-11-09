//! Drag-and-drop handler that consumes a [`PendingGeneratorDrop`] on
//! mouse release: routes the drag into a peer-gift `ItemOffer`, an
//! inventory→room placement, or a room→room placement depending on
//! whether the cursor was over a peer row in the People panel and which
//! [`DropSource`] armed the drag.
//!
//! A whole avatar or world (#1844) arms a gift-only drag: released on a
//! People row it is offered like a thing - an avatar by value, a world by
//! its head, and a world only once every part it names is in the saved
//! stash - while the ground preview never draws for it and a release over
//! the world view places nothing and says so.

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
use crate::protocol::{GiftItem, GiftKind, OverlandsMessage};
use crate::state::{
    CurrentRoomDid, LiveInventoryRecord, LiveRoomRecord, PendingOutgoingOffers,
    StoredInventoryRecord,
};

use crate::pds::inventory::is_drop_placeable;

use super::{DropSource, PendingGeneratorDrop};

/// Drop handler. Runs every frame in `InGame`; cheap-out early unless a drag
/// has been armed via [`PendingGeneratorDrop`].
///
/// Three destinations share this path (see [`DropSource`]):
/// * Release over a peer row in the People window → routes into an
///   [`OverlandsMessage::ItemOffer`]. The target is resolved from
///   `pending.peer_target`, refreshed every frame by the People UI.
/// * [`DropSource::Inventory`] release over the 3D viewport - copy the
///   blueprint into `RoomRecord.generators` under a collision-safe key
///   (reusing an identical existing entry when present) and append a
///   `Placement::Absolute`.
/// * [`DropSource::Catalogue`] release over the 3D viewport - resolve
///   the slug and stamp a fresh deep-copied blueprint the same way.
///
/// A whole avatar or world (#1844) takes the first path only: released
/// over the viewport it places nothing and one toast says so.
///
/// The peer branch fires even when the local user does not own the current
/// room - gifting is a personal transaction between two players and
/// doesn't touch the `RoomRecord`. The ground-placement branches still
/// enforce `session.did == room_did` so a malicious state transition
/// mid-drag (e.g. a portal that swaps the room DID) cannot mutate a room
/// the user doesn't own.
///
/// The handler clears `pending.generator_name` and `pending.peer_target`
/// on every exit path so the next drag starts clean.
///
/// Escape disarms the drag mid-flight (#831), but the key is NOT read
/// here: it is [`EscStep::DragToPlace`](crate::ui::shortcuts::EscStep), a
/// rung of the shared back-out ladder (#1236 f37). Reading it here meant
/// two systems answered the same press in the same frame - nothing
/// consumes `ButtonInput` - so one Escape disarmed the drag *and* cleared
/// the selection underneath it, against the ladder's own "one step per
/// press" contract.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_generator_drop(
    mut contexts: EguiContexts,
    mut pending: ResMut<PendingGeneratorDrop>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), crate::camera::IsWorldCamera>,
    spatial: SpatialQuery,
    terrain_q: Query<Entity, With<crate::terrain::TerrainMesh>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    inventory: Option<Res<LiveInventoryRecord>>,
    mut room: Option<ResMut<LiveRoomRecord>>,
    mut pending_offers: ResMut<PendingOutgoingOffers>,
    mut session_log: ResMut<SessionLog>,
    mut sender: SendMessage<OverlandsMessage>,
    mut chunk: crate::network::chunk::ChunkSend,
    // Bundled to stay under Bevy's 16-parameter ceiling. The stored stash
    // says whether a world is saved, and so can be given (#1844).
    (time, mut toasts, mut undo_labels, stored_inventory): (
        Res<Time>,
        ResMut<crate::notify::Toasts>,
        ResMut<crate::ui::undo::PendingUndoLabels>,
        Option<Res<StoredInventoryRecord>>,
    ),
) {
    let Some(name) = pending.generator_name.clone() else {
        return;
    };
    let source = pending.source;
    let kind = pending.kind;
    let peer_target = pending.peer_target.clone();

    // The drag only commits on the frame the button is released. Every other
    // frame the mouse is either still held (drag in progress - we want to
    // keep the pending slot armed) or already up without a just-released
    // edge (stale state - clear it so a future drag starts clean).
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
    // here - the usual "released over egui = cancel" rule doesn't apply.
    // -----------------------------------------------------------------
    if let Some(target) = peer_target {
        // An ineligible row now SAYS so (#1220 f330). This used to be
        // unreachable - an ineligible row was never recorded as a target, so
        // the release fell past the gift branch into the silent egui-cancel
        // below, which exists for drops over the Inventory window. The
        // sentence is the same one the row's hover carries, from
        // `people::gift_block_reason`, so the drag and the drop cannot tell
        // different stories.
        if let Some(reason) = target.blocked {
            toasts.info(
                format!("Couldn't give \"{name}\" to {} - {reason}", target.label),
                time.elapsed_secs_f64(),
            );
            return;
        }
        // The sender still needs the item in hand; pull it from the same
        // sources the ground-placement branches use so both drop origins
        // can gift.
        let Some(sess) = session.as_deref() else {
            return;
        };
        let Some(gift) = gift_contents(
            source,
            &name,
            inventory.as_deref().map(|inv| &inv.0),
            &sess.did,
        ) else {
            warn!("Peer-gift drop: source item '{}' not found", name);
            return;
        };
        match &gift {
            // Same filters the UI uses to gate drag-start; defence-in-depth
            // in case an item that cannot be given slipped through.
            GiftItem::Generator { generator, .. } if !is_drop_placeable(generator) => return,
            GiftItem::Avatar(avatar) if !avatar.is_readable() => return,
            // A world goes from the saved copy (#1844), and a save may have
            // moved under the drag: asked again on the release, and said.
            GiftItem::World(head) => {
                if let Some(reason) = crate::pds::inventory::world::gift_blocked_reason(
                    head,
                    stored_inventory.as_deref().map(|stored| &stored.0.worlds),
                ) {
                    toasts.info(
                        format!("Couldn't give \"{name}\" to {} - {reason}", target.label),
                        time.elapsed_secs_f64(),
                    );
                    return;
                }
            }
            _ => {}
        }
        send_gift_offer(
            &target.did,
            &target.label,
            &name,
            &gift,
            &mut GiftSending {
                pending_offers: &mut pending_offers,
                session_log: &mut session_log,
                sender: &mut sender,
                chunk: &mut chunk,
                toasts: &mut toasts,
            },
            time.elapsed_secs_f64(),
        );
        return;
    }

    // -----------------------------------------------------------------
    // Ground-placement branch from here on. Enforces room ownership and
    // requires a clean viewport release.
    // -----------------------------------------------------------------

    // Releasing over any egui area (notably the Inventory window itself) is
    // the standard "cancel" gesture - treat it as a silent no-op instead of
    // placing a generator under the user's UI. Checked BEFORE the ownership
    // gate (#831) so a visitor's deliberate window-cancel never draws the
    // "you can't place here" toast below.
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.is_pointer_over_egui() {
        return;
    }
    // A whole avatar or world is given, never placed (#1844): the drag that
    // carries one is gift-only, so a release over the world view says so
    // rather than vanishing - in any world, the owner's own included.
    if let Some(line) = whole_release_refusal(kind, &name) {
        toasts.info(line, time.elapsed_secs_f64());
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
        // A true viewport release in a room the user doesn't own - the
        // drag used to just vanish (#831).
        toasts.warn(
            "Only this world's owner can place items here - drop on a \
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
            "Released over open sky - nothing placed.",
            time.elapsed_secs_f64(),
        );
        return;
    };
    let hit_point = ray.origin + *ray.direction * hit.distance;

    let Some(record) = room.as_mut() else {
        return;
    };
    // `room.as_mut()` already tripped the `LiveRoomRecord` change tick
    // (the late borrow above is deliberate - see the gate-order comment);
    // unwrap to the inner record for the field writes below.
    let record = &mut record.0;

    // The caps, before any insert (#1210): a drop at the placement cap
    // used to land, get selected, and be truncated by the next flush; a
    // drop that needed a new generator at the generator cap cost whichever
    // generator sorted last.
    {
        use crate::ui::room::caps::Cap;
        if Cap::Placements.is_full(record.placements.len()) {
            toasts.warn(Cap::Placements.full_reason(), time.elapsed_secs_f64());
            return;
        }
        if Cap::Generators.is_full(record.generators.len()) {
            toasts.warn(Cap::Generators.full_reason(), time.elapsed_secs_f64());
            return;
        }
    }

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
            // every drop - the catalogue is a stamp library, not a
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

    place_drop(record, &gen_key, hit_point);
    undo_labels.set_room(format!("drop of {gen_key}"));
    info!(
        "Placed generator '{}' (as '{}') from {:?} at ({:.2}, {:.2}, {:.2})",
        name, gen_key, source, hit_point.x, hit_point.y, hit_point.z
    );
    // No dirty flag to set - the World Editor derives "dirty" from
    // `records_differ(stored, live)`, and the push above mutated the
    // live record (the `room.as_mut()` borrow already set its change
    // tick, driving the recompile + peer broadcast).
}

/// Stand the generator `gen_key` where the drop's ray hit the ground, and
/// settle the room: the drop is the gesture's end, so the raycast point -
/// and a fresh catalogue tree - land on the values every peer holds
/// (#1707).
fn place_drop(record: &mut crate::pds::RoomRecord, gen_key: &str, hit_point: Vec3) {
    record.placements.push(Placement::Absolute {
        generator_ref: gen_key.to_string(),
        transform: TransformData {
            translation: Fp3([hit_point.x, hit_point.y, hit_point.z]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        avoid_water: false,
        avoid_water_clearance: crate::pds::Fp(0.0),
        snap_to_terrain: false,
        seed: None,
        params: Default::default(),
    });
    crate::ui::room::settle_edit(record);
}

/// What a release over the world view says about a drag of `kind` (#1844):
/// a whole avatar or world places nothing, so the one toast names the
/// gesture that does give it. `None` for a thing, which the placement path
/// takes from here.
pub(crate) fn whole_release_refusal(kind: GiftKind, name: &str) -> Option<String> {
    let what = match kind {
        GiftKind::Generator => return None,
        GiftKind::Avatar => "an avatar",
        GiftKind::World => "a world",
    };
    Some(format!(
        "\"{name}\" is {what}, so nothing was placed - drop it on a person in the People list \
         to give it."
    ))
}

/// What a gift of `name` from `source` gives (#1844): a stash item's tree
/// and the wear metadata in its side table (#1108), a whole avatar by value,
/// or a whole world's head; or a catalogue entry's tree built fresh for
/// `did`, with wear minted from its declaration - the same two sources the
/// local Copy-to-inventory and Wear flows read. Catalogue avatars and worlds
/// are not given (the recipient has the same Catalogue), so a slug names a
/// thing or nothing. `None` when there is no such item. Shared by the drag
/// and the agent client's `gift give` (#1423).
pub(crate) fn gift_contents(
    source: DropSource,
    name: &str,
    inventory: Option<&crate::pds::InventoryRecord>,
    did: &str,
) -> Option<GiftItem> {
    match source {
        DropSource::Inventory => {
            let inv = inventory?;
            if let Some(generator) = inv.generators.get(name) {
                return Some(GiftItem::Generator {
                    generator: Box::new(generator.clone()),
                    wear: inv.wear.get(name).cloned(),
                });
            }
            if let Some(avatar) = inv.avatars.get(name) {
                return Some(GiftItem::Avatar(Box::new(avatar.clone())));
            }
            inv.worlds
                .get(name)
                .map(|world| GiftItem::World(world.head().clone()))
        }
        DropSource::Catalogue => crate::catalogue::by_slug(name).map(|entry| {
            let wear = entry
                .wear_socket()
                .map(|socket| crate::pds::inventory::WearMeta::for_entry(socket, entry.wear_fit()));
            GiftItem::Generator {
                generator: Box::new(entry.build(did)),
                wear,
            }
        }),
    }
}

/// What sending a gift writes to: the pending offers it joins, the log,
/// the transport and the sender's toasts.
pub(crate) struct GiftSending<'a, 'm, 'c> {
    pub pending_offers: &'a mut PendingOutgoingOffers,
    pub session_log: &'a mut SessionLog,
    pub sender: &'a mut SendMessage<'m, OverlandsMessage>,
    pub chunk: &'a mut crate::network::chunk::ChunkSend<'c>,
    pub toasts: &'a mut crate::notify::Toasts,
}

/// Offer `gift` to `target_did` as `item_name`: send it and keep the
/// offer pending until it is answered or lapses - the one path the drag and
/// the agent client's `gift give` share (#1423). The offer's id once it has
/// gone out; `None` when the transport refused it, which a toast says.
///
/// Routed through the chunker (#717): a gifted `Generator` can be a large
/// Shape-grammar / L-system blueprint, and a whole avatar (#1844) carries
/// every prop it wears, either pushing the offer past the 64 KiB WebRTC
/// message ceiling, which would otherwise fail silently and leave the
/// recipient without the gift. Past the chunker's own reliable cap the send
/// is refused, and the sender's toast says the item is too large.
pub(crate) fn send_gift_offer(
    target_did: &str,
    target_label: &str,
    item_name: &str,
    gift: &GiftItem,
    to: &mut GiftSending<'_, '_, '_>,
    now: f64,
) -> Option<u64> {
    let offer_id = to.pending_offers.peek_next_id();
    let outcome = to.chunk.broadcast(
        to.sender,
        to.session_log,
        now,
        OverlandsMessage::item_offer(offer_id, target_did.to_owned(), item_name.to_owned(), gift),
    );
    let sent = outcome.is_sent();
    record_gift_outcome(
        outcome,
        offer_id,
        target_did,
        target_label,
        item_name,
        to.pending_offers,
        to.session_log,
        to.toasts,
        now,
    );
    sent.then_some(offer_id)
}

/// Pick a key under which to store a dropped generator in the room's
/// `generators` map. Reuses the existing entry when its contents already
/// match the dragged blueprint (so repeated drops of the same inventory
/// item all share one room generator), and otherwise falls back to a
/// `_2`, `_3`, … suffix to avoid clobbering a pre-existing generator that
/// happens to share the inventory name - and a suffixed key already holding
/// the very same blueprint is reused too (#1855), so the second drop of an
/// item that had to be keyed apart shares its first drop's generator
/// rather than adding a `_3` copy of it.
///
/// Equality is checked through `serde_json::to_value` because `Generator`
/// doesn't derive `PartialEq` - same pattern the inventory's dirty diff uses.
///
/// Crate-visible for the agent's `place` (#1422, #1855), which is a drop
/// with no mouse - of a catalogue entry or an inventory thing - and keys
/// what it places by this same rule.
pub(crate) fn choose_room_generator_key(
    existing: &HashMap<String, Generator>,
    inventory_name: &str,
    new_gen: &Generator,
) -> String {
    use crate::pds::{limits::MAX_GENERATOR_NAME_CHARS, sanitize::names::with_suffix};
    let same =
        |held: &Generator| serde_json::to_value(held).ok() == serde_json::to_value(new_gen).ok();
    // The name as the sanitiser keeps it (#1717): a raw name free here can
    // be cleaned onto a part's after this check, and the drop lost.
    let name = with_suffix(inventory_name, "", MAX_GENERATOR_NAME_CHARS);
    match existing.get(&name) {
        Some(held) if same(held) => return name,
        Some(_) => {}
        None => return name,
    }
    for i in 2u32..u32::MAX {
        // Cut to leave the suffix room under the cap, or the sanitiser cuts
        // it back onto the name it was made unique from, and drops it
        // (#1717).
        let candidate = with_suffix(&name, &format!("_{i}"), MAX_GENERATOR_NAME_CHARS);
        match existing.get(&candidate) {
            Some(held) if same(held) => return candidate,
            Some(_) => {}
            None => return candidate,
        }
    }
    name
}

/// Live ground preview for an armed drag (#831): a footprint ring + post
/// at the exact spot a release would place the item, re-raycast every
/// frame - before this the drop point was invisible until commit (the
/// raycast ran only in the release path), so "released over sky", "landed
/// behind that building" and "placed 40 m downhill" all looked identical
/// mid-drag. Green = a release here places; red = the ground can't take
/// it (visiting someone else's overland - gift on a People row instead).
/// Over egui areas nothing draws: the follow-cursor tooltip is the
/// feedback there, and a release there is the cancel gesture. Nor for a
/// whole avatar or world (#1844), which no release places.
#[allow(clippy::too_many_arguments)]
pub(crate) fn preview_generator_drop(
    mut contexts: EguiContexts,
    pending: Res<PendingGeneratorDrop>,
    mut gizmos: Gizmos,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), crate::camera::IsWorldCamera>,
    spatial: SpatialQuery,
    terrain_q: Query<Entity, With<crate::terrain::TerrainMesh>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
) {
    use crate::config::ui::drop_preview as cfg;

    let Some(name) = pending.generator_name.as_ref() else {
        return;
    };
    // A whole avatar or world places nothing (#1844), so there is no spot
    // to mark: the follow-cursor tooltip says where it goes instead.
    if pending.kind.is_whole() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.is_pointer_over_egui() {
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
    // Terrain-only, exactly like the release path - the preview must
    // show where the commit raycast will actually land.
    let filter = SpatialQueryFilter::default();
    let hit = spatial.cast_ray_predicate(ray.origin, ray.direction, 4096.0, true, &filter, &|e| {
        terrain_q.get(e).is_ok()
    });
    let Some(hit) = hit else {
        return; // Sky under the cursor - nothing to mark.
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

/// Book the aftermath of a gift send: what the sender is told, what the
/// diagnostics log records, and whether the offer's expiry timer is armed
/// at all (#1123).
///
/// Split out of [`handle_generator_drop`] because that system's dozen
/// `SystemParam`s - an egui context, a window, a camera, a spatial query -
/// put the branch that matters out of reach of a test, and this branch is
/// precisely the one that was wrong: the offer used to be registered and
/// logged as `ItemOfferSent` BEFORE the send, so a message the chunker
/// refused still armed the expiry timer. The sender saw "Offer sent to
/// @them", then minutes later "expired without an answer" - an accusation
/// against a peer who was never asked for anything.
///
/// Bookkeeping now follows the send. The `offer_id` was only *peeked*, so
/// the refused path consumes nothing and the next gift reuses it.
#[allow(clippy::too_many_arguments)]
fn record_gift_outcome(
    outcome: crate::network::chunk::SendOutcome,
    offer_id: u64,
    target_did: &str,
    target_label: &str,
    item_name: &str,
    pending_offers: &mut PendingOutgoingOffers,
    session_log: &mut SessionLog,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) {
    if !outcome.is_sent() {
        toasts.error(
            format!(
                "Couldn't send \"{item_name}\" to {target_label} - the item is \
                 too large for a peer-to-peer transfer."
            ),
            now,
        );
        warn!(
            "ItemOffer \"{item_name}\" to {target_label} ({target_did}) was not sent: {outcome:?}"
        );
        return;
    }
    pending_offers.register(
        offer_id,
        target_did.to_string(),
        target_label.to_string(),
        item_name.to_string(),
        now,
    );
    session_log.info(
        now,
        EventPayload::ItemOfferSent {
            offer_id,
            target_did: target_did.to_string(),
            item_name: item_name.to_string(),
        },
    );
    // Sender-side feedback (#843): releasing on a peer row used to
    // confirm NOTHING - the offer's whole lifecycle lived in the
    // diagnostics log.
    toasts.success(
        format!("Offer sent to {target_label} - \"{item_name}\"."),
        now,
    );
    info!("Sent ItemOffer #{offer_id} \"{item_name}\" to {target_label} ({target_did})");
}

#[cfg(test)]
mod gift_outcome_tests {
    use super::*;
    use crate::network::chunk::SendOutcome;
    use crate::notify::ToastKind;

    fn book(outcome: SendOutcome) -> (PendingOutgoingOffers, SessionLog, crate::notify::Toasts) {
        let mut offers = PendingOutgoingOffers::default();
        let mut log = SessionLog::default();
        let mut toasts = crate::notify::Toasts::default();
        let id = offers.peek_next_id();
        record_gift_outcome(
            outcome,
            id,
            "did:plc:them",
            "@them.test",
            "brass lantern",
            &mut offers,
            &mut log,
            &mut toasts,
            0.0,
        );
        (offers, log, toasts)
    }

    /// #1123 - the sequence that produced the lie: drag an item onto a peer
    /// row, the chunker refuses the `ItemOffer` for exceeding the wire
    /// ceiling, and the sender is congratulated for a gift that never left
    /// the machine - then blamed on the recipient when it "expires".
    #[test]
    fn a_refused_offer_is_not_registered_logged_or_congratulated() {
        let (offers, log, toasts) = book(SendOutcome::Refused { bytes: 1_000_000 });

        assert!(
            offers.by_id.is_empty(),
            "no expiry timer for a message nobody received"
        );
        assert_eq!(
            offers.peek_next_id(),
            0,
            "and the id is not burned - the next gift reuses it"
        );
        assert!(
            !log.iter()
                .any(|e| matches!(e.payload, EventPayload::ItemOfferSent { .. })),
            "ItemOfferSent is a claim about the wire, not about the click"
        );
        let shown = toasts.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, ToastKind::Error);
        assert!(
            shown[0].1.contains("brass lantern") && shown[0].1.contains("them.test"),
            "the failure names the item and the peer: {}",
            shown[0].1
        );
    }

    /// #1218 f299. The recipient's name arrives already addressed off the
    /// shared ladder, so the toasts must not glue an `@` on top of it: for a
    /// peer whose profile has not resolved the label is a DID head, and
    /// `@did:plc:z72i7hdy…` is the exact defect the gift modal had - a sigil
    /// promising a name over an identifier.
    #[test]
    fn a_recipient_with_no_resolved_handle_is_not_toasted_at_as_an_at_sign_did() {
        let mut offers = PendingOutgoingOffers::default();
        let mut log = SessionLog::default();
        let mut toasts = crate::notify::Toasts::default();
        let id = offers.peek_next_id();
        record_gift_outcome(
            SendOutcome::Sent,
            id,
            "did:plc:z72i7hdynmk6r22z27h6tvur",
            // What `PeerLabel::addressed` yields for an unresolved profile.
            "did:plc:z72i7hdy…",
            "brass lantern",
            &mut offers,
            &mut log,
            &mut toasts,
            0.0,
        );
        let shown = toasts.shown();
        assert_eq!(shown.len(), 1);
        assert!(
            !shown[0].1.contains("@did:"),
            "the sigil belongs to the handle tier alone: {}",
            shown[0].1
        );
        assert!(shown[0].1.contains("did:plc:z72i7hdy…"), "{}", shown[0].1);
    }

    /// The success path is unchanged - the fix must not have cost the
    /// sender-side confirmation #843 added.
    #[test]
    fn a_sent_offer_still_registers_logs_and_confirms() {
        let (offers, log, toasts) = book(SendOutcome::Sent);

        assert_eq!(offers.by_id.len(), 1);
        assert_eq!(offers.by_id[&0].target_label, "@them.test");
        assert_eq!(offers.peek_next_id(), 1, "the id is consumed");
        assert!(
            log.iter()
                .any(|e| matches!(e.payload, EventPayload::ItemOfferSent { .. }))
        );
        let shown = toasts.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, ToastKind::Success);
    }

    /// A message that would not serialize is the same news to the sender as
    /// one that was too big: it did not go.
    #[test]
    fn an_unserializable_offer_is_treated_as_not_sent() {
        let (offers, _, toasts) = book(SendOutcome::NotSerialized);
        assert!(offers.by_id.is_empty());
        assert_eq!(toasts.shown()[0].0, ToastKind::Error);
    }
}

#[cfg(test)]
mod place_drop_tests {
    use super::*;

    /// #1844. A whole avatar or world released over the world view places
    /// nothing - in the owner's own world too - and the one toast names the
    /// gesture that gives it; a thing goes on to the placement path.
    #[test]
    fn a_whole_item_released_over_the_world_places_nothing_and_says_so() {
        for (kind, what) in [
            (GiftKind::Avatar, "an avatar"),
            (GiftKind::World, "a world"),
        ] {
            let line = whole_release_refusal(kind, "harbour").expect("refused");
            assert!(line.contains("\"harbour\""), "{line}");
            assert!(line.contains(what), "{line}");
            assert!(line.contains("nothing was placed"), "{line}");
            assert!(line.contains("People list"), "{line}");
        }
        assert_eq!(whole_release_refusal(GiftKind::Generator, "lamp"), None);
    }

    /// #1844. What a stash gives, by kind: a thing with its wear, an avatar
    /// by value, a world by its head alone.
    #[test]
    fn a_stash_gives_each_kind_as_itself() {
        use crate::pds::inventory::avatar_item_tests::generator_record;
        let mut stash = crate::pds::InventoryRecord::default();
        stash.put_item(String::from("lamp"), Generator::default_cuboid(), None);
        let avatar =
            crate::pds::inventory::AvatarItem::from_record(&generator_record()).expect("copies");
        stash.put_avatar(String::from("ride"), avatar.clone());
        let world = crate::pds::inventory::WorldItem::snapshot(
            &crate::pds::inventory::world::tests::seeded_world(81),
        )
        .expect("copies");
        stash.put_world(String::from("harbour"), world.clone());
        let give = |name| gift_contents(DropSource::Inventory, name, Some(&stash), "did:plc:me");
        assert!(matches!(give("lamp"), Some(GiftItem::Generator { .. })));
        assert!(matches!(give("ride"), Some(GiftItem::Avatar(a)) if *a == avatar));
        assert!(matches!(give("harbour"), Some(GiftItem::World(h)) if &h == world.head()));
        assert!(give("nothing").is_none());
    }

    /// A drop lands where every peer will read it (#1707): the raycast
    /// point is off the wire's grid, and the placement stands on the grid.
    #[test]
    fn a_drop_lands_on_the_values_its_peers_hold() {
        let mut record = crate::pds::RoomRecord::default();
        record.generators.insert(
            "crate".to_string(),
            Generator::from_kind(crate::ui::room::construct::make_default_for_kind("Cuboid")),
        );
        let hit = Vec3::new(12.345_67, 0.000_04, -3.000_06);
        place_drop(&mut record, "crate", hit);

        let Some(Placement::Absolute {
            generator_ref,
            transform,
            ..
        }) = record.placements.last()
        else {
            panic!("the drop placed nothing");
        };
        assert_eq!(generator_ref, "crate");
        let held = Fp3(hit.to_array()).on_wire();
        assert_ne!(held.0, hit.to_array(), "the hit is off the grid");
        assert_eq!(transform.translation, held);
    }

    /// THE SEQUENCE (#1717): a stash item whose name is at the 256-character
    /// cap, dropped into a world already holding another part of that name,
    /// took the key NAME_2 - past the cap - and the drop's settle cut it back
    /// onto NAME and dropped it, leaving its placement pointing at nothing.
    #[test]
    fn a_drop_named_at_the_cap_lands_beside_its_namesake() {
        let name = "w".repeat(crate::pds::limits::MAX_GENERATOR_NAME_CHARS);
        let mut record = crate::pds::RoomRecord::default();
        record.generators.clear();
        record.placements.clear();
        let kind = crate::ui::room::construct::make_default_for_kind;
        record
            .generators
            .insert(name.clone(), Generator::from_kind(kind("Cuboid")));
        let dropped = Generator::from_kind(kind("Sphere"));
        let key = choose_room_generator_key(&record.generators, &name, &dropped);
        assert_ne!(
            key, name,
            "the namesake differs, so the drop needs a key of its own"
        );
        record.generators.insert(key.clone(), dropped.clone());
        place_drop(&mut record, &key, Vec3::ZERO);

        assert_eq!(
            record.generators.get(&name).map(|g| g.kind.clone()),
            Some(kind("Cuboid"))
        );
        assert_eq!(record.generators.get(&key), Some(&dropped), "the drop kept");
        let Some(Placement::Absolute { generator_ref, .. }) = record.placements.last() else {
            panic!("the drop placed nothing");
        };
        assert_eq!(generator_ref, &key, "and its placement finds it");
    }

    /// #1855: an item keyed apart from a namesake it differs from goes in as
    /// `NAME_2`, and its second drop shares that generator - it used to
    /// see `NAME_2` taken and add a `_3` copy of the same tree.
    #[test]
    fn a_second_drop_of_an_item_keyed_apart_shares_its_generator() {
        let kind = crate::ui::room::construct::make_default_for_kind;
        let mut existing = HashMap::new();
        existing.insert("Cottage".to_string(), Generator::from_kind(kind("Cuboid")));
        let dropped = Generator::from_kind(kind("Sphere"));

        let first = choose_room_generator_key(&existing, "Cottage", &dropped);
        assert_eq!(first, "Cottage_2");
        existing.insert(first.clone(), dropped.clone());
        assert_eq!(
            choose_room_generator_key(&existing, "Cottage", &dropped),
            first,
            "the second drop reuses the first's key"
        );
        let other = Generator::from_kind(kind("Cylinder"));
        assert_eq!(
            choose_room_generator_key(&existing, "Cottage", &other),
            "Cottage_3",
            "a third, different tree still gets a key of its own"
        );
    }
}
