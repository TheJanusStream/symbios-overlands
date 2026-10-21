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

#[allow(clippy::too_many_arguments)]
pub(super) fn broadcast_local_state(
    query: Query<(&Transform, &LinearVelocity, &AngularVelocity), With<LocalPlayer>>,
    session: Option<Res<AtprotoSession>>,
    first_person: Option<Res<crate::camera::FirstPerson>>,
    peers: Query<&crate::state::RemotePeer>,
    mut sender: SendMessage<OverlandsMessage>,
    mut tick: Local<u32>,
    mut was_stationary: Local<bool>,
    mut sent_rotation: Local<Option<Quat>>,
) {
    *tick = tick.wrapping_add(1);

    let Ok((tf, lin_vel, ang_vel)) = query.single() else {
        return;
    };

    // Throttle transform broadcasts when nearly stationary: drop from ~64 Hz
    // to ~2 Hz (every 30th tick) to save WebRTC bandwidth and WASM CPU.
    // Check both linear *and* angular velocity so a spinning-in-place rover
    // still streams smooth rotation updates to peers - and the turn since the
    // last pose sent, for a body turned by writing its rotation (#1746).
    let turned = sent_rotation.is_none_or(|sent| {
        sent.angle_between(tf.rotation) > config::network::STATIONARY_TURN_RADIANS
    });
    let stationary = lin_vel.0.length() <= config::network::STATIONARY_SPEED_THRESHOLD
        && ang_vel.0.length() <= config::network::STATIONARY_ANGULAR_THRESHOLD
        && !turned;
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
        *sent_rotation = Some(tf.rotation);
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
        // Looking out of our own eyes is said again on this cadence while it
        // lasts (#1746), so a peer that met us since the switch learns it
        // within a second; `broadcast_first_person` says each switch at once.
        // Only to peers whose `Hello` said they read it: an older build logs
        // a decode warning for every copy.
        if looks_out(first_person.as_deref()) {
            for peer in &peers {
                if peer
                    .build
                    .as_ref()
                    .is_some_and(|build| build.protocol >= crate::protocol::FIRST_PERSON_SINCE)
                {
                    sender.to(
                        peer.peer_id,
                        OverlandsMessage::FirstPerson { on: true },
                        ChannelKind::Reliable,
                    );
                }
            }
        }
    }
}

/// Whether the local body looks out of its own eyes, on foot (#1746).
fn looks_out(first_person: Option<&crate::camera::FirstPerson>) -> bool {
    first_person.is_some_and(|fp| fp.looks_out_on_foot())
}

/// Tell peers when we look out of our own eyes and when we stop (#1746), so
/// they draw our body's stride off its travel against its facing - see
/// [`OverlandsMessage::FirstPerson`]. Compared against what we last SENT, as
/// the world digest is, so each switch of the view goes out once and nothing
/// else does.
pub(super) fn broadcast_first_person(
    first_person: Option<Res<crate::camera::FirstPerson>>,
    mut sender: SendMessage<OverlandsMessage>,
    mut last_sent: Local<bool>,
) {
    let on = looks_out(first_person.as_deref());
    if on == *last_sent {
        return;
    }
    *last_sent = on;
    sender.broadcast(OverlandsMessage::FirstPerson { on }, ChannelKind::Reliable);
}

/// Announce our world digest whenever it changes (#1146).
///
/// Sent on the reliable channel, once per settled derivation rather than on a
/// timer: the digest only moves when the terrain, the splat map or the compile
/// moves, and a peer that joins later gets the next one (the room is
/// re-derived on entry, and an owner's edit re-derives it for everyone).
///
/// Nothing is broadcast until all three parts of the digest have landed, so a
/// peer never announces a digest of a world it is still building - see
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
    mut notices: ResMut<super::chunk::OversizeNotices>,
    mut toasts: ResMut<crate::notify::Toasts>,
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
    // A refusal here means the owner's live avatar edits stop reaching
    // guests while the size gauge in the Avatar editor stays green - the
    // per-record budget and the wire ceiling are different limits (#1123).
    let now = time.elapsed_secs_f64();
    super::chunk::warn_once_on_refusal(
        chunk.broadcast(&mut sender, &mut session_log, now, message),
        &mut notices,
        &mut toasts,
        "avatar",
        now,
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
    mut notices: ResMut<super::chunk::OversizeNotices>,
    mut toasts: ResMut<crate::notify::Toasts>,
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
    // The refusal that used to be console-only (#1123). The owner is the
    // one person who can act on it and the only one who cannot see it: the
    // World Editor's size gauge measures the largest single PDS record,
    // which stays comfortably green while the whole-room broadcast - a
    // different quantity against a different ceiling - is being dropped.
    super::chunk::warn_once_on_refusal(
        chunk.broadcast(&mut sender, &mut session_log, now, message),
        &mut notices,
        &mut toasts,
        super::chunk::LIVE_WORLD,
        now,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Each switch of the view goes out once, and nothing else does**
    /// (#1746): into the eyes says `FirstPerson { on: true }` on the
    /// Reliable channel, a frame with nothing changed says nothing, and out
    /// again says `on: false`. With no camera there is nothing to say.
    #[test]
    fn each_switch_of_the_view_is_said_once() {
        let mut app = App::new();
        app.add_message::<Broadcast<OverlandsMessage>>()
            .add_message::<SendTo<OverlandsMessage>>()
            .add_systems(Update, broadcast_first_person);
        let said = |app: &mut App| -> Vec<bool> {
            app.update();
            app.world()
                .resource::<Messages<Broadcast<OverlandsMessage>>>()
                .iter_current_update_messages()
                .filter_map(|sent| match sent.payload {
                    OverlandsMessage::FirstPerson { on } => {
                        assert!(matches!(sent.channel, ChannelKind::Reliable));
                        Some(on)
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(said(&mut app), Vec::<bool>::new(), "no camera");
        app.insert_resource(crate::camera::FirstPerson::riding());
        assert_eq!(said(&mut app), Vec::<bool>::new(), "a vehicle's nose");
        app.insert_resource(crate::camera::FirstPerson::looking());
        assert_eq!(said(&mut app), vec![true], "into the eyes");
        assert_eq!(said(&mut app), Vec::<bool>::new(), "said once");
        app.insert_resource(crate::camera::FirstPerson::default());
        assert_eq!(said(&mut app), vec![false], "out again");
    }

    /// The local body's state broadcaster in an app of its own, a body at
    /// rest standing at the origin: the app and the body.
    fn local_state_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_message::<Broadcast<OverlandsMessage>>()
            .add_message::<SendTo<OverlandsMessage>>()
            .add_systems(Update, broadcast_local_state);
        let body = app
            .world_mut()
            .spawn((
                LocalPlayer,
                Transform::default(),
                LinearVelocity::default(),
                AngularVelocity::default(),
            ))
            .id();
        (app, body)
    }

    /// **A body turning where it stands streams its turn** (#1746). In first
    /// person a walker turns with the eyes by writing its rotation while
    /// avian's velocities stay at rest, and the rest rate - a pose every
    /// thirtieth tick - showed that turn to peers half a second late. Turning,
    /// it sends a pose every tick; still again, it falls back to the rest rate
    /// after one pose that lands the turn.
    #[test]
    fn a_body_turning_where_it_stands_streams_its_turn() {
        let (mut app, body) = local_state_app();
        let poses = |app: &mut App| {
            app.update();
            app.world()
                .resource::<Messages<Broadcast<OverlandsMessage>>>()
                .iter_current_update_messages()
                .filter(|sent| matches!(sent.payload, OverlandsMessage::Transform { .. }))
                .count()
        };
        // The first pose a body ever has goes out; then it rests.
        poses(&mut app);
        let resting: usize = (0..10).map(|_| poses(&mut app)).sum();
        assert!(resting <= 1, "at rest, {resting} poses in ten ticks");
        let mut turning = 0;
        for tick in 1..=10 {
            app.world_mut()
                .get_mut::<Transform>(body)
                .expect("a body")
                .rotation = Quat::from_rotation_y(0.05 * tick as f32);
            turning += poses(&mut app);
        }
        assert_eq!(turning, 10, "turning, a pose every tick");
        let landed = poses(&mut app);
        let after: usize = (0..10).map(|_| poses(&mut app)).sum();
        assert!(
            landed + after <= 2,
            "still again, {} poses in eleven ticks",
            landed + after
        );
    }

    /// **The once-a-second repeat goes only to peers that read it** (#1746):
    /// a build older than [`crate::protocol::FIRST_PERSON_SINCE`] drops the
    /// message with a decode warning in its log, and a peer whose `Hello`
    /// has not come yet is told on the next round.
    #[test]
    fn the_repeat_goes_only_to_peers_that_read_it() {
        let (mut app, _) = local_state_app();
        app.insert_resource(crate::camera::FirstPerson::looking());
        let peer = |n: u8, protocol: Option<u16>| crate::state::RemotePeer {
            peer_id: serde_json::from_str(&format!("\"00000000-0000-0000-0000-0000000000{n:02}\""))
                .expect("a well-formed uuid"),
            did: None,
            handle: None,
            muted: false,
            avatar: None,
            build: protocol.map(|protocol| crate::state::PeerBuild {
                protocol,
                build: String::new(),
            }),
            connected_at: 0.0,
        };
        let reader = peer(1, Some(crate::protocol::FIRST_PERSON_SINCE));
        let to = reader.peer_id;
        app.world_mut().spawn(reader);
        app.world_mut()
            .spawn(peer(2, Some(crate::protocol::FIRST_PERSON_SINCE - 1)));
        app.world_mut().spawn(peer(3, None));
        let mut told = Vec::new();
        for _ in 0..config::network::IDENTITY_BROADCAST_INTERVAL_TICKS {
            app.update();
            told.extend(
                app.world()
                    .resource::<Messages<SendTo<OverlandsMessage>>>()
                    .iter_current_update_messages()
                    .filter(|sent| {
                        matches!(sent.payload, OverlandsMessage::FirstPerson { on: true })
                    })
                    .map(|sent| sent.target),
            );
        }
        assert_eq!(told, vec![to], "one round, to the one peer that reads it");
    }
}
