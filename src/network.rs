use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::{AvatarFetchPending, AvatarMaterial};
use crate::config;
use crate::protocol::{AirshipParams, OverlandsMessage};
use crate::rover::rebuild_airship_children;
use crate::state::{
    AppState, ChatHistory, DiagnosticsLog, LocalAirshipParams, LocalPlayer, RemotePeer,
    TransformBuffer, TransformSample,
};

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SymbiosMultiuserPlugin::<OverlandsMessage>::deferred())
            .add_systems(
                Update,
                (
                    handle_peer_connections,
                    handle_incoming_messages,
                    smooth_remote_transforms,
                    broadcast_local_state,
                    sync_mute_visibility,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn handle_peer_connections(
    mut commands: Commands,
    mut peer_events: ResMut<PeerStateQueue<OverlandsMessage>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    peers: Query<(Entity, &RemotePeer)>,
) {
    for event in peer_events.drain() {
        match event.state {
            PeerConnectionState::Connected => {
                diagnostics.push(format!("[+] Peer {} connected", event.peer));
                let entity = commands
                    .spawn((
                        Transform::from_xyz(0.0, 10.0, 0.0),
                        Visibility::default(),
                        RemotePeer {
                            peer_id: event.peer,
                            did: None,
                            handle: None,
                            muted: false,
                            airship: None,
                        },
                        TransformBuffer::default(),
                    ))
                    .id();

                // Spawn default airship visuals until we receive their Identity.
                rebuild_airship_children(
                    &mut commands,
                    entity,
                    &AirshipParams::default(),
                    None,
                    &mut meshes,
                    &mut materials,
                    None,
                );
            }
            PeerConnectionState::Disconnected => {
                for (entity, peer) in peers.iter() {
                    if peer.peer_id == event.peer {
                        let label = peer
                            .handle
                            .as_deref()
                            .or(peer.did.as_deref())
                            .unwrap_or("unknown");
                        diagnostics
                            .push(format!("[-] Peer {} ({}) disconnected", event.peer, label));
                        commands.entity(entity).despawn();
                    }
                }
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn handle_incoming_messages(
    mut commands: Commands,
    mut queue: ResMut<NetworkQueue<OverlandsMessage>>,
    mut chat: ResMut<ChatHistory>,
    mut peers: Query<(
        Entity,
        &mut RemotePeer,
        &mut Transform,
        &mut TransformBuffer,
        Option<&Children>,
        Option<&AvatarMaterial>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for msg in queue.drain() {
        match msg.payload {
            OverlandsMessage::Transform { position, rotation } => {
                for (_, peer, _tf, mut buf, _, _) in peers.iter_mut() {
                    if peer.peer_id == msg.sender {
                        buf.samples.push_back(TransformSample {
                            position: Vec3::from_array(position),
                            rotation: Quat::from_array(rotation),
                            timestamp: now,
                        });
                        while buf.samples.len() > config::network::KINEMATIC_BUFFER_CAPACITY {
                            buf.samples.pop_front();
                        }
                    }
                }
            }
            OverlandsMessage::Identity {
                did,
                handle,
                airship,
            } => {
                for (entity, mut peer, _, _, children, avatar_mat) in peers.iter_mut() {
                    if peer.peer_id != msg.sender {
                        continue;
                    }

                    let did_changed = peer.did.as_deref() != Some(did.as_str());
                    let airship_changed = peer.airship.as_ref() != Some(&airship);

                    peer.handle = Some(handle.clone());

                    if did_changed {
                        info!("Peer {} identified as @{} ({})", msg.sender, handle, did);
                        commands
                            .entity(entity)
                            .insert(AvatarFetchPending { did: did.clone() });
                        peer.did = Some(did.clone());
                    }

                    if airship_changed {
                        // Rebuild the peer's vessel with the updated parameters.
                        let children_ref = children.map(|c| c as &Children);
                        rebuild_airship_children(
                            &mut commands,
                            entity,
                            &airship,
                            children_ref,
                            &mut meshes,
                            &mut materials,
                            avatar_mat.map(|m| &m.0),
                        );
                        peer.airship = Some(airship.clone());
                    }
                }
            }
            OverlandsMessage::Chat { text } => {
                // Ignore messages from muted peers.
                let sender_muted = peers
                    .iter()
                    .find(|(_, peer, _, _, _, _)| peer.peer_id == msg.sender)
                    .map(|(_, peer, _, _, _, _)| peer.muted)
                    .unwrap_or(false);

                if !sender_muted {
                    let author = peers
                        .iter()
                        .find(|(_, peer, _, _, _, _)| peer.peer_id == msg.sender)
                        .and_then(|(_, peer, _, _, _, _)| peer.handle.clone())
                        .unwrap_or_else(|| msg.sender.to_string());
                    chat.messages.push((author, text));
                }
            }
        }
    }
}

fn broadcast_local_state(
    query: Query<&Transform, With<LocalPlayer>>,
    session: Option<Res<AtprotoSession>>,
    ap: Res<LocalAirshipParams>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut tick: Local<u32>,
) {
    *tick = tick.wrapping_add(1);

    let Ok(tf) = query.single() else { return };

    writer.write(Broadcast {
        payload: OverlandsMessage::Transform {
            position: tf.translation.to_array(),
            rotation: tf.rotation.to_array(),
        },
        channel: ChannelKind::Unreliable,
    });

    if tick.is_multiple_of(config::network::IDENTITY_BROADCAST_INTERVAL_TICKS)
        && let Some(sess) = &session
    {
        writer.write(Broadcast {
            payload: OverlandsMessage::Identity {
                did: sess.did.clone(),
                handle: sess.handle.clone(),
                airship: ap.params.clone(),
            },
            channel: ChannelKind::Reliable,
        });
    }
}

/// Resolve each remote peer's displayed transform from the jitter buffer.
///
/// When `smooth_kinematics` is enabled we evaluate a cubic Hermite spline at
/// `now - KINEMATIC_RENDER_DELAY_SECS`, using central-difference tangents of
/// the buffered samples for the translation and `Quat::slerp` for the
/// rotation.  When disabled, we snap straight to the most recent sample — a
/// useful debugging mode for observing raw network latency.
fn smooth_remote_transforms(
    time: Res<Time>,
    ap: Res<LocalAirshipParams>,
    mut peers: Query<(&mut Transform, &mut TransformBuffer), With<RemotePeer>>,
) {
    let now = time.elapsed_secs_f64();
    let render_time = now - config::network::KINEMATIC_RENDER_DELAY_SECS;

    for (mut tf, mut buf) in peers.iter_mut() {
        if buf.samples.is_empty() {
            continue;
        }

        // Raw-snap mode — just follow the latest packet and keep the buffer
        // trimmed so a later mode flip doesn't jump back in time.
        if !ap.smooth_kinematics {
            if let Some(last) = buf.samples.back() {
                tf.translation = last.position;
                tf.rotation = last.rotation;
            }
            // Drop all but the most recent sample to bound memory.
            while buf.samples.len() > 1 {
                buf.samples.pop_front();
            }
            continue;
        }

        // Evict samples that are clearly older than render_time to avoid
        // unbounded growth while keeping at least one sample on either side.
        let prune_cutoff =
            render_time - 2.0 * config::network::KINEMATIC_RENDER_DELAY_SECS.max(0.05);
        while buf.samples.len() > 2
            && buf.samples.get(1).map(|s| s.timestamp).unwrap_or(f64::MAX) < prune_cutoff
        {
            buf.samples.pop_front();
        }

        // Find the segment [i, i+1] that brackets render_time.  If render_time
        // is before the first sample we simply snap to the earliest; if it's
        // past the last, we extrapolate by snapping to the latest.
        let samples = &buf.samples;
        if samples.len() == 1 || render_time <= samples.front().unwrap().timestamp {
            let s = samples.front().unwrap();
            tf.translation = s.position;
            tf.rotation = s.rotation;
            continue;
        }
        if render_time >= samples.back().unwrap().timestamp {
            let s = samples.back().unwrap();
            tf.translation = s.position;
            tf.rotation = s.rotation;
            continue;
        }

        // Walk to find the bracketing pair.
        let mut i = 0;
        while i + 1 < samples.len() && samples[i + 1].timestamp < render_time {
            i += 1;
        }
        let a = samples[i];
        let b = samples[i + 1];
        let dt = (b.timestamp - a.timestamp).max(1e-6);
        let t = ((render_time - a.timestamp) / dt).clamp(0.0, 1.0) as f32;

        // Estimate velocity tangents with a central difference.  Fall back to
        // forward/backward differences at the ends of the buffer so we always
        // have a well-defined tangent.
        let dt_f = dt as f32;
        let tangent_a = if i > 0 {
            let prev = samples[i - 1];
            let total = (b.timestamp - prev.timestamp).max(1e-6) as f32;
            (b.position - prev.position) / total * dt_f
        } else {
            b.position - a.position
        };
        let tangent_b = if i + 2 < samples.len() {
            let next = samples[i + 2];
            let total = (next.timestamp - a.timestamp).max(1e-6) as f32;
            (next.position - a.position) / total * dt_f
        } else {
            b.position - a.position
        };

        // Cubic Hermite basis.  Equivalent to bevy_math::CubicHermite over a
        // single segment but skips the Vec allocation and Result unwrapping.
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        tf.translation = a.position * h00 + tangent_a * h10 + b.position * h01 + tangent_b * h11;
        tf.rotation = a.rotation.slerp(b.rotation, t);
    }
}

/// Propagate each peer's mute flag to its `Visibility` component so that
/// muted vessels and their child meshes are hidden automatically.
fn sync_mute_visibility(mut peers: Query<(&RemotePeer, &mut Visibility)>) {
    for (peer, mut vis) in peers.iter_mut() {
        let desired = if peer.muted {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != desired {
            *vis = desired;
        }
    }
}
