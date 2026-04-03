use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::AvatarFetchPending;
use crate::config;
use crate::protocol::{AirshipParams, OverlandsMessage};
use crate::rover::rebuild_airship_children;
use crate::state::{AppState, ChatHistory, DiagnosticsLog, LocalAirshipParams, LocalPlayer, RemotePeer};

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SymbiosMultiuserPlugin::<OverlandsMessage>::deferred())
            .add_systems(
                Update,
                (
                    handle_peer_connections,
                    handle_incoming_messages,
                    broadcast_local_state,
                    sync_mute_visibility,
                )
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

fn handle_incoming_messages(
    mut commands: Commands,
    mut queue: ResMut<NetworkQueue<OverlandsMessage>>,
    mut chat: ResMut<ChatHistory>,
    mut peers: Query<(Entity, &mut RemotePeer, &mut Transform, Option<&Children>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for msg in queue.drain() {
        match msg.payload {
            OverlandsMessage::Transform { position, rotation } => {
                for (_, peer, mut tf, _) in peers.iter_mut() {
                    if peer.peer_id == msg.sender {
                        tf.translation = Vec3::from_array(position);
                        tf.rotation = Quat::from_array(rotation);
                    }
                }
            }
            OverlandsMessage::Identity { did, handle, airship } => {
                for (entity, mut peer, _, children) in peers.iter_mut() {
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
                        );
                        if let Some(did_str) = &peer.did {
                            commands
                                .entity(entity)
                                .insert(AvatarFetchPending { did: did_str.clone() });
                        }
                        peer.airship = Some(airship.clone());
                    }
                }
            }
            OverlandsMessage::Chat { text } => {
                // Ignore messages from muted peers.
                let sender_muted = peers
                    .iter()
                    .find(|(_, peer, _, _)| peer.peer_id == msg.sender)
                    .map(|(_, peer, _, _)| peer.muted)
                    .unwrap_or(false);

                if !sender_muted {
                    let author = peers
                        .iter()
                        .find(|(_, peer, _, _)| peer.peer_id == msg.sender)
                        .and_then(|(_, peer, _, _)| peer.handle.clone())
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

