//! Outbound broadcast systems: continuous transform throttling on
//! `FixedUpdate`, plus the live-preview avatar/room broadcasts that fire
//! the same frame an editor slider changes the resource.

use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::protocol::OverlandsMessage;
use crate::state::{CurrentRoomDid, LiveAvatarRecord, LiveRoomRecord, LocalPlayer};

pub(super) fn broadcast_local_state(
    query: Query<(&Transform, &LinearVelocity, &AngularVelocity), With<LocalPlayer>>,
    session: Option<Res<AtprotoSession>>,
    mut sender: SendMessage<OverlandsMessage>,
    mut tick: Local<u32>,
    mut was_stationary: Local<bool>,
) {
    *tick = tick.wrapping_add(1);

    let Ok((tf, lin_vel, ang_vel)) = query.single() else {
        return;
    };

    // Throttle transform broadcasts when nearly stationary: drop from ~64 Hz
    // to ~2 Hz (every 30th tick) to save WebRTC bandwidth and WASM CPU.
    // Check both linear *and* angular velocity so a spinning-in-place rover
    // still streams smooth rotation updates to peers.
    let stationary = lin_vel.0.length() <= config::network::STATIONARY_SPEED_THRESHOLD
        && ang_vel.0.length() <= config::network::STATIONARY_ANGULAR_THRESHOLD;
    // Force one final broadcast on the tick we cross into rest so peers land
    // on the true parked pose instead of interpolating toward a stale sample.
    let just_came_to_rest = stationary && !*was_stationary;
    *was_stationary = stationary;
    let should_send = !stationary
        || just_came_to_rest
        || tick.is_multiple_of(config::network::STATIONARY_BROADCAST_DIVISOR);

    if should_send {
        sender.broadcast(
            OverlandsMessage::Transform {
                position: tf.translation.to_array(),
                rotation: tf.rotation.to_array(),
            },
            ChannelKind::Unreliable,
        );
    }

    if tick.is_multiple_of(config::network::IDENTITY_BROADCAST_INTERVAL_TICKS) {
        // Announce the wire layout on the same cadence as identity, but NOT
        // behind the session gate (#1121): what a peer's messages decode as
        // does not depend on either end being logged in, and a peer that has
        // not authenticated is exactly the one whose silence we would
        // otherwise misread as a stale build.
        sender.broadcast(
            OverlandsMessage::Hello {
                protocol: crate::protocol::PROTOCOL_VERSION,
                build: crate::protocol::build_id(),
            },
            ChannelKind::Reliable,
        );
        if let Some(sess) = &session {
            sender.broadcast(
                OverlandsMessage::Identity {
                    did: sess.did.clone(),
                    handle: sess.handle.clone(),
                },
                ChannelKind::Reliable,
            );
        }
    }
}

/// Announce our world digest whenever it changes (#1146).
///
/// Sent on the reliable channel, once per settled derivation rather than on a
/// timer: the digest only moves when the terrain, the splat map or the compile
/// moves, and a peer that joins later gets the next one (the room is
/// re-derived on entry, and an owner's edit re-derives it for everyone).
///
/// Nothing is broadcast until all three parts of the digest have landed, so a
/// peer never announces a digest of a world it is still building — see
/// [`crate::world_digest::WorldDigest::combined`].
pub(super) fn broadcast_world_digest(
    digest: Res<crate::world_digest::WorldDigest>,
    mut sender: SendMessage<OverlandsMessage>,
    mut last_sent: Local<Option<(u64, u64)>>,
) {
    let Some(combined) = digest.combined() else {
        return;
    };
    // Compared against what we last SENT rather than gated on `is_changed`:
    // the resource is written by three producers, each of which may rewrite
    // the same value, and re-announcing an unchanged digest is pure noise on
    // an ordered channel that gifts and room updates share.
    if *last_sent == Some((digest.record_fp, combined)) {
        return;
    }
    *last_sent = Some((digest.record_fp, combined));
    sender.broadcast(
        OverlandsMessage::WorldDigest {
            record_fp: digest.record_fp,
            digest: combined,
        },
        ChannelKind::Reliable,
    );
}

/// Broadcast a live-preview `AvatarStateUpdate` whenever the local avatar
/// resource changes, so peers can mirror the edit before the author commits
/// to Publish. Runs in `Update` so that UI mutations observed this frame
/// propagate the same frame, bypassing the fixed-timestep throttle used for
/// continuous transforms.
///
/// Routed through [`super::chunk::ChunkSend::broadcast`] (#716): a maxed-out
/// avatar record can serialize past the 64 KiB WebRTC message ceiling, which
/// would otherwise fail silently and leave peers on a stale avatar.
pub(super) fn broadcast_avatar_state(
    live: Res<LiveAvatarRecord>,
    mut sender: SendMessage<OverlandsMessage>,
    mut chunk: super::chunk::ChunkSend,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    time: Res<Time>,
) {
    if !live.is_changed() {
        return;
    }
    // A body this build cannot re-serialize (a newer client's kind, kept as
    // `Unknown`) is not broadcastable; peers get the authoritative record
    // from the PDS instead, decoded by whatever build understands it. The
    // body marker is checked first so the common case reports the reason it
    // knows; `avatar_state_update` then catches an `Unknown` anywhere deeper
    // in the record (#1111) and declines the same way.
    if live.0.body.wire_ready().is_err() {
        return;
    }
    let Some(message) = OverlandsMessage::avatar_state_update(&live.0) else {
        return;
    };
    chunk.broadcast(
        &mut sender,
        &mut session_log,
        time.elapsed_secs_f64(),
        message,
    );
}

/// Broadcast a live-preview `RoomStateUpdate` whenever the owner mutates
/// the `RoomRecord` resource so guests mirror the edit the same frame the
/// slider moves. The `session.did == room_did.0` gate ensures guests
/// (whose `RoomRecord` is also rewritten by inbound `RoomStateUpdate`
/// handling) do not echo the owner's broadcast back to the relay.
///
/// The whole record is re-serialized per broadcast, so this is **debounced**
/// to at most one send per [`crate::config::network::ROOM_BROADCAST_MIN_INTERVAL_SECS`]
/// (#716): a slider drag rewrites `LiveRoomRecord` every frame, and without
/// the throttle a large room would re-chunk and re-send the full record
/// ~60×/s, saturating the ordered Reliable channel and stalling every other
/// reliable message behind it. `dirty` remembers a change seen mid-interval
/// so the *final* drag state is always flushed once the interval elapses.
/// The send itself goes through [`super::chunk::ChunkSend::broadcast`], which
/// splits the record across sub-ceiling fragments or refuses it if it is past
/// the hard payload ceiling.
#[allow(clippy::too_many_arguments)]
pub(super) fn broadcast_room_state(
    record: Option<Res<LiveRoomRecord>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut sender: SendMessage<OverlandsMessage>,
    mut chunk: super::chunk::ChunkSend,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    time: Res<Time>,
    mut dirty: Local<bool>,
    mut last_sent: Local<Option<f64>>,
) {
    let (Some(record), Some(session), Some(room_did)) = (record, session, room_did) else {
        return;
    };
    if session.did != room_did.0 {
        return;
    }
    if record.is_changed() {
        *dirty = true;
    }
    if !*dirty {
        return;
    }
    let now = time.elapsed_secs_f64();
    let due = match *last_sent {
        None => true,
        Some(prev) => now - prev >= config::network::ROOM_BROADCAST_MIN_INTERVAL_SECS,
    };
    if !due {
        return;
    }
    // A room holding a union arm this build cannot write back is not
    // broadcast (#1111): guests read the owner's PDS record instead, which
    // whatever build understands it decodes in full.
    let Some(message) = OverlandsMessage::room_state_update(&record.0) else {
        return;
    };
    *dirty = false;
    *last_sent = Some(now);
    chunk.broadcast(&mut sender, &mut session_log, now, message);
}
