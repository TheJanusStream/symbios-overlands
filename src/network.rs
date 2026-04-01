use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::protocol::OverlandsMessage;
use crate::state::{AppState, ChatHistory, DiagnosticsLog, LocalPlayer, RemotePeer};

const IDENTITY_BROADCAST_INTERVAL_TICKS: u32 = 60;

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
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::new(1.6, 0.4, 2.4))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgb(0.3, 0.3, 0.3),
                        ..default()
                    })),
                    Transform::from_xyz(0.0, 10.0, 0.0),
                    RemotePeer {
                        peer_id: event.peer,
                        did: None,
                        handle: None,
                    },
                ));
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
    mut peers: Query<(Entity, &mut RemotePeer, &mut Transform)>,
) {
    for msg in queue.drain() {
        match msg.payload {
            OverlandsMessage::Transform { position, rotation } => {
                for (_, peer, mut tf) in peers.iter_mut() {
                    if peer.peer_id == msg.sender {
                        tf.translation = Vec3::from_array(position);
                        tf.rotation = Quat::from_array(rotation);
                    }
                }
            }
            OverlandsMessage::Identity { did, handle } => {
                for (entity, mut peer, _) in peers.iter_mut() {
                    if peer.peer_id == msg.sender {
                        let did_changed = peer.did.as_deref() != Some(did.as_str());
                        peer.handle = Some(handle.clone());
                        if did_changed {
                            info!("Peer {} identified as @{} ({})", msg.sender, handle, did);
                            commands
                                .entity(entity)
                                .insert(crate::avatar::AvatarFetchPending { did: did.clone() });
                            peer.did = Some(did.clone());
                        }
                    }
                }
            }
            OverlandsMessage::Chat { text } => {
                let author = peers
                    .iter()
                    .find(|(_, peer, _)| peer.peer_id == msg.sender)
                    .and_then(|(_, peer, _)| peer.handle.clone())
                    .unwrap_or_else(|| msg.sender.to_string());
                chat.messages.push((author, text));
            }
        }
    }
}

fn broadcast_local_state(
    query: Query<&Transform, With<LocalPlayer>>,
    session: Option<Res<AtprotoSession>>,
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

    if tick.is_multiple_of(IDENTITY_BROADCAST_INTERVAL_TICKS) {
        if let Some(sess) = &session {
            writer.write(Broadcast {
                payload: OverlandsMessage::Identity {
                    did: sess.did.clone(),
                    handle: sess.handle.clone(),
                },
                channel: ChannelKind::Reliable,
            });
        }
    }
}
