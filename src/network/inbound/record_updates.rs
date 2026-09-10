//! Record broadcasts: a peer's avatar, the room, and the digest that says
//! whether either is worth fetching (#1161).
//!
//! What these four share is that each carries, or points at, a whole
//! **record** — the same shapes the PDS stores — and so each is the place a
//! peer-supplied payload becomes local state. Every one of them therefore
//! decodes, sanitises and ownership-checks before it writes, and the
//! ownership rule differs per message: an avatar is its sender's to
//! replace, a room is only the ROOM OWNER's, and the owner's own other
//! session is a third case again (#1203).

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use super::{InboundBuffers, PeerParts, carry_resolution, forget_rig_resolution};
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::network::peer_cache::PeerAvatarCache;
use crate::network::presence::FetchState;
use crate::protocol::OverlandsMessage;
use crate::state::{CurrentRoomDid, LiveRoomRecord};
/// Replace a peer's avatar record with the one they just broadcast.
///
/// `from` rather than `sender`, because the arm binds that name to the
/// peer's own query row.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_avatar_state(
    from: PeerId,
    record_json: Vec<u8>,
    peers: &mut Query<PeerParts>,
    session_log: &mut SessionLog,
    avatar_cache: &mut PeerAvatarCache,
    bufs: &mut InboundBuffers,
    now: f64,
) {
    // Live preview nudge from a peer who is mid-edit.
    //
    // AUTHORITY FIRST, matching the `RoomStateUpdate` arm below
    // (#1126). This used to decode and sanitize up to ~900 KiB
    // of JSON and only then check whether the sender was a peer
    // we had authenticated — so a peer that never sent a valid
    // Identity could make every guest walk a maximal generator
    // tree once per frame and throw the result away. The
    // heaviest message in the protocol already resolves the
    // sender before touching the payload; the second-heaviest
    // now does too.
    let sender = peers.iter().find(|(_, peer, _, _)| peer.peer_id == from);
    // A muted peer does not get to run the second-heaviest
    // decode in the protocol on this client, once per keystroke
    // of their editor (#1219 f287). Checked in the same breath as
    // the authority resolve and BEFORE the size check, for the
    // same reason that one moved up: the envelope carries
    // everything the gate needs.
    if sender.is_some_and(|(_, peer, _, _)| peer.muted) {
        return;
    }
    let Some(sender_did) = sender.and_then(|(_, peer, _, _)| peer.did.clone()) else {
        debug!(
            "Deferring AvatarStateUpdate from {}: peer DID not yet known",
            from
        );
        return;
    };
    // Symmetric with `from_chunk_bytes`, which refuses to
    // reassemble past this ceiling: a payload that could never
    // have been legitimately broadcast is not worth decoding.
    if record_json.len() > crate::config::network::MAX_RELIABLE_PAYLOAD_BYTES {
        warn!(
            "Dropping AvatarStateUpdate from {:?}: {} bytes exceeds the wire ceiling",
            from,
            record_json.len()
        );
        return;
    }
    let Some(mut new_record) = OverlandsMessage::decode_avatar_state(&record_json) else {
        // Emit the typed decode-failure event (#634) so the
        // `net.silent_decode_failure` rule sees this arm too — it
        // already matches all three, but only ItemOffer was emitting.
        session_log.warn(
            now,
            EventPayload::AvatarStateDecodeFailed {
                peer: from.to_string(),
                reason: "payload failed to decode".into(),
            },
        );
        warn!(
            "Dropping AvatarStateUpdate from {:?}: payload failed to decode",
            from
        );
        return;
    };
    new_record.sanitize();

    for (entity, mut peer, _, _) in peers.iter_mut() {
        if peer.peer_id != from {
            continue;
        }
        // The DID was resolved above, before the decode — a peer
        // without one never reaches here.
        let peer_did = sender_did.clone();
        // A live preview IS the peer's real record, so it retires
        // any failed-fetch stand-in and the retry that would have
        // overwritten it (#1217 f323). This is one of the two
        // recoveries the finding noted; it now closes the failure
        // state instead of silently racing it.
        if let Ok(mut resolve) = bufs.resolve.get_mut(entity) {
            resolve.avatar = FetchState::Landed;
        }
        // Carry a still-valid resolution across the update
        // (#1113). `resolved` is `#[serde(skip)]`, so every
        // preview arrives unresolved; overwriting wholesale threw
        // away a resolution that was still correct and made each
        // debounced keystroke cost a DID-document lookup plus a
        // wardrobe record plus up to sixteen attachment records,
        // on every client in the room. The references name what
        // the resolution is *of*, so an unchanged reference set
        // means the fetched records are unchanged too.
        carry_resolution(peer.avatar.as_ref(), &mut new_record);
        // Refresh the cache so a future Identity from this DID
        // (e.g. reconnect within the session) restores the
        // live-preview state instead of the stale PDS record.
        avatar_cache.insert(peer_did, new_record.clone());
        peer.avatar = Some(new_record);
        break;
    }
}

/// Compare a peer's world digest against ours (#1146).
pub(super) fn handle_world_digest(
    sender: PeerId,
    record_fp: u64,
    theirs: u64,
    session_log: &mut SessionLog,
    bufs: &mut InboundBuffers,
    now: f64,
) {
    // A peer told us what it built (#1146). Compared only when we
    // agree on the RECORD: during an owner's slider drag the two
    // ends are legitimately a broadcast apart, and reporting that
    // as a desync would bury the real thing under noise.
    //
    // Nothing is refused, corrected or re-derived on the strength
    // of this. The event is the entire product: before it existed,
    // two clients expanding one record into two different worlds
    // (#51, #882) produced no evidence at all, so every such
    // report was a user describing two screens from memory.
    let Some(ours) = bufs.world_digest.combined() else {
        // We have not settled our own world yet, so we have
        // nothing to compare and no business calling anyone wrong.
        return;
    };
    if bufs.world_digest.record_fp != record_fp || ours == theirs {
        return;
    }
    // Once per (peer, disagreement): the sender only re-announces
    // when its digest moves, and a moved digest is a new fact.
    warn!(
        "World digest mismatch with {}: we built {:016x}, they built {:016x} from record {:016x}",
        sender, ours, theirs, record_fp
    );
    session_log.error(
        now,
        EventPayload::PeerWorldDigestMismatch {
            peer: sender.to_string(),
            record_fp,
            ours,
            theirs,
        },
    );
}

/// A peer saved their avatar: drop the resolution behind their unchanged
/// reference list so it is fetched afresh (#1122).
pub(super) fn handle_records_published(
    sender: PeerId,
    commands: &mut Commands,
    peers: &mut Query<PeerParts>,
) {
    // The sender saved their rigged body (#1122). Same rkeys,
    // new bytes behind them — so drop the resolution we are
    // carrying and let `spawn_peer_rig_resolutions` fetch the
    // published records. Without this the owner's Save reached
    // nobody: the re-broadcast preview names the same
    // references, `carry_resolution` keeps the pre-save body,
    // and peers stayed on it until the owner happened to edit
    // something that changed a reference.
    //
    // Deliberately does NOT clear `PeerRigResolveFloor` (#1126):
    // the floor is the per-peer cap on how often one identity can
    // make every guest fan out to hosts of its choosing, and a
    // message a peer sends at will must not lift it. The
    // reference-set backoff does go, because it records that
    // THESE references failed to resolve — which a publish is
    // precisely the news that invalidates.
    for (peer_entity, mut peer, _, _) in peers.iter_mut() {
        if peer.peer_id != sender {
            continue;
        }
        // Nothing to forget is nothing to do (#1224 f336).
        // `peer.avatar.as_mut()` raises the change tick
        // unconditionally — even for a peer with no record, and
        // even when the resolution is already `None` — so an
        // unguarded notice was a free way to dirty a peer and
        // re-run two `Changed<RemotePeer>` systems. Read first,
        // and take the mutable borrow only when the answer
        // actually moves.
        let carrying = peer
            .avatar
            .as_ref()
            .and_then(|record| record.body.rigged_ref())
            .is_some_and(|rig| rig.resolved.is_some());
        if carrying && let Some(record) = peer.avatar.as_mut() {
            forget_rig_resolution(record);
        }
        commands
            .entity(peer_entity)
            .remove::<crate::network::peer_cache::PeerRigResolveBackoff>();
        break;
    }
}

/// Replace the live room record — but only from the peer who owns it, and
/// only after the same-owner split (#1203) has decided whose copy wins.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_room_state(
    sender: PeerId,
    record_json: Vec<u8>,
    commands: &mut Commands,
    peers: &mut Query<PeerParts>,
    room_did: &Option<Res<CurrentRoomDid>>,
    room_record: &mut Option<ResMut<LiveRoomRecord>>,
    session: &Option<Res<AtprotoSession>>,
    session_log: &mut SessionLog,
    bufs: &mut InboundBuffers,
    now: f64,
) {
    // Authority check FIRST: decoding and sanitising up to ~1 MiB
    // of JSON per broadcast is expensive enough that a guest
    // spamming forged updates at 60 Hz would burn main-thread
    // cycles even though the result is ultimately discarded. By
    // resolving the sender's DID and comparing against the room
    // owner before touching `record_json`, a non-owner broadcast
    // short-circuits before the parse runs.
    let sender_did = peers
        .iter()
        .find(|(_, peer, _, _)| peer.peer_id == sender)
        .and_then(|(_, peer, _, _)| peer.did.clone());

    let is_owner = match (&sender_did, &room_did) {
        (Some(did), Some(rd)) => did == &rd.0,
        _ => false,
    };
    // The owner's OTHER session (#1203): same DID as this
    // session, so the gate below passes it like any owner
    // broadcast — but the local record may hold edits the
    // other session has never seen.
    let same_owner = match (&sender_did, session.as_deref()) {
        (Some(did), Some(session)) => did == &session.did,
        _ => false,
    };

    if !is_owner {
        // Dropped correctly, but silently until #1144 — and this
        // is the one inbound drop with a hostile reading: a guest
        // broadcasting forged room state at frame rate.
        session_log.warn(
            now,
            EventPayload::RoomStateRejected {
                sender_did: sender_did.clone().unwrap_or_default(),
                reason: String::from("sender does not own this room"),
            },
        );
        return;
    }

    // Decode the JSON payload shipped by the owner. The wire
    // format is JSON-in-bincode because `RoomRecord`'s tagged
    // enums are incompatible with bincode's streaming decoder —
    // see `OverlandsMessage::RoomStateUpdate` docs.
    let Some(mut new_record) = OverlandsMessage::decode_room_state(&record_json) else {
        // Typed decode-failure event (#634). `sender_did` is
        // guaranteed `Some` here — the `is_owner` gate above required
        // it — but default defensively rather than unwrap.
        session_log.warn(
            now,
            EventPayload::RoomStateDecodeFailed {
                sender_did: sender_did.clone().unwrap_or_default(),
                error: "failed to decode as RoomRecord".into(),
            },
        );
        warn!(
            "Dropping RoomStateUpdate from {:?}: payload failed to decode as RoomRecord",
            sender
        );
        return;
    };

    // Clamp every unbounded numeric field before the world
    // compiler touches the recipe — a malicious owner could
    // otherwise ship a grid_size or L-system iteration count
    // designed to OOM every guest.
    new_record.sanitize();

    // Replace the whole recipe. `world_builder::compile_room_record`
    // observes the resource change and rebuilds every compiled
    // entity (water, sun colour, scattered shapes) in one pass.
    if let Some(record) = room_record.as_mut() {
        if same_owner {
            use crate::state::{OtherSessionRoom, SameOwnerUpdate, classify_same_owner_update};
            let equals = !crate::state::records_differ(&record.0, &new_record);
            let dirty = bufs
                .stored_room
                .as_deref()
                .is_some_and(|stored| crate::state::records_differ(&record.0, &stored.0));
            match classify_same_owner_update(dirty, equals) {
                // The echo of our own broadcast (both sessions
                // rebroadcast on `is_changed`): applying it
                // would reset the undo ring and bounce the
                // same bytes straight back.
                SameOwnerUpdate::Ignore => return,
                SameOwnerUpdate::Hold => {
                    session_log.warn(
                        now,
                        EventPayload::RoomStateRejected {
                            sender_did: sender_did.clone().unwrap_or_default(),
                            reason: String::from(
                                "held: the owner's other session changed the world \
                                 while this session has unpublished edits — asking \
                                 which copy to keep",
                            ),
                        },
                    );
                    if bufs.held_room.is_none() {
                        bufs.toasts.warn(
                            "Your world changed in another session while you have \
                             unpublished edits here — choose which copy to keep.",
                            now,
                        );
                    }
                    commands.insert_resource(OtherSessionRoom { record: new_record });
                    return;
                }
                SameOwnerUpdate::Apply => {
                    let due = bufs
                        .other_session_toast_at
                        .is_none_or(|at| now - at >= 30.0);
                    if due {
                        bufs.toasts.info(
                            "Your world was updated from another session signed \
                             in as you.",
                            now,
                        );
                        *bufs.other_session_toast_at = Some(now);
                    }
                }
            }
        }
        // The one inbound outcome the diagnostics suite never
        // recorded (#1146): `RoomStateApplied` has been declared
        // since the suite was built and emitted nowhere, so a
        // captured log could show a room broadcast being REJECTED
        // or failing to DECODE but never being accepted. Two peers
        // comparing logs after a desync therefore could not
        // establish the first thing worth knowing — whether they
        // were even deriving the same recipe. The record
        // fingerprint here is the same one the world digest is
        // keyed by, so the two line up.
        session_log.info(
            now,
            EventPayload::RoomStateApplied {
                bytes: record_json.len() as u64,
                digest_of_record: crate::world_digest::record_fingerprint(&new_record),
            },
        );
        record.0 = new_record;
        // Foreign wholesale write (#862): mostly guests (whose
        // history is empty anyway), but a second session of
        // the SAME owner DID passes the is_owner gate too —
        // the local ring must reset rather than offer undos
        // across the other session's replacement.
        bufs.undo_signals.foreign = true;
        info!("Room state updated from owner broadcast");
    }
}
