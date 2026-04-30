//! Outbound broadcast systems: continuous transform throttling on
//! `FixedUpdate`, plus the live-preview avatar/room broadcasts that fire
//! the same frame an editor slider changes the resource.

use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::pds::RoomRecord;
use crate::protocol::OverlandsMessage;
use crate::state::{CurrentRoomDid, LiveAvatarRecord, LocalPlayer};

pub(super) fn broadcast_local_state(
    query: Query<(&Transform, &LinearVelocity, &AngularVelocity), With<LocalPlayer>>,
    session: Option<Res<AtprotoSession>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut tick: Local<u32>,
    mut was_stationary: Local<bool>,
) {
    *tick = tick.wrapping_add(1);

    let Ok((tf, lin_vel, ang_vel)) = query.single() else {
        return;
    };

    // Throttle transform broadcasts when nearly stationary: drop from ~60 Hz
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
        writer.write(Broadcast {
            payload: OverlandsMessage::Transform {
                position: tf.translation.to_array(),
                rotation: tf.rotation.to_array(),
            },
            channel: ChannelKind::Unreliable,
        });
    }

    if tick.is_multiple_of(config::network::IDENTITY_BROADCAST_INTERVAL_TICKS)
        && let Some(sess) = &session
    {
        writer.write(Broadcast {
            payload: OverlandsMessage::Identity {
                did: sess.did.clone(),
                handle: sess.handle.clone(),
            },
            channel: ChannelKind::Reliable,
        });
    }
}

/// Broadcast a live-preview `AvatarStateUpdate` whenever the local avatar
/// resource changes, so peers can mirror the edit before the author commits
/// to Publish. Runs in `Update` so that UI mutations observed this frame
/// propagate the same frame, bypassing the fixed-timestep throttle used for
/// continuous transforms.
pub(super) fn broadcast_avatar_state(
    live: Res<LiveAvatarRecord>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    if !live.is_changed() {
        return;
    }
    writer.write(Broadcast {
        payload: OverlandsMessage::avatar_state_update(&live.0),
        channel: ChannelKind::Reliable,
    });
}

/// Broadcast a live-preview `RoomStateUpdate` whenever the owner mutates
/// the `RoomRecord` resource so guests mirror the edit the same frame the
/// slider moves. The `session.did == room_did.0` gate ensures guests
/// (whose `RoomRecord` is also rewritten by inbound `RoomStateUpdate`
/// handling) do not echo the owner's broadcast back to the relay.
pub(super) fn broadcast_room_state(
    record: Option<Res<RoomRecord>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    let (Some(record), Some(session), Some(room_did)) = (record, session, room_did) else {
        return;
    };
    if session.did != room_did.0 {
        return;
    }
    if !record.is_changed() {
        return;
    }
    writer.write(Broadcast {
        payload: OverlandsMessage::room_state_update(&record),
        channel: ChannelKind::Reliable,
    });
}
