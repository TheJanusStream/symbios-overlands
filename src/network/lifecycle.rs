//! Peer connect/disconnect plumbing, mute-visibility sync, and the
//! stale-offer-dialog evictor. State-management systems that don't fit
//! the inbound-dispatch / outbound-broadcast pair.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::protocol::OverlandsMessage;
use crate::state::{DiagnosticsLog, IncomingOfferDialog, RemotePeer, TransformBuffer};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_peer_connections(
    mut commands: Commands,
    mut peer_events: ResMut<PeerStateQueue<OverlandsMessage>>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    peers: Query<(Entity, &RemotePeer)>,
    time: Res<Time>,
    session: Option<Res<AtprotoSession>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    let elapsed = time.elapsed_secs_f64();
    for event in peer_events.drain() {
        match event.state {
            PeerConnectionState::Connected => {
                diagnostics.push(elapsed, format!("[+] Peer {} connected", event.peer));
                // Spawn the peer with no avatar yet — the hot-swap system in
                // `player.rs` will build visuals once the PDS fetch populates
                // `RemotePeer::avatar`. Leaving the vessel invisible until
                // then is deliberate: a guessed default would be indistinguishable
                // from a deliberately-minimal avatar and mislead the other
                // players about the peer's real appearance.
                commands.spawn((
                    Transform::from_xyz(0.0, 10.0, 0.0),
                    Visibility::default(),
                    RemotePeer {
                        peer_id: event.peer,
                        did: None,
                        handle: None,
                        muted: false,
                        avatar: None,
                    },
                    TransformBuffer::default(),
                ));

                // Proactively announce our identity to the newcomer.  Without
                // this, they only learn our DID on the next scheduled identity
                // broadcast (~1 s), during which a RoomStateUpdate from us
                // would fail the owner-DID check and be silently dropped.
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
            PeerConnectionState::Disconnected => {
                for (entity, peer) in peers.iter() {
                    if peer.peer_id == event.peer {
                        let label = peer
                            .handle
                            .as_deref()
                            .or(peer.did.as_deref())
                            .unwrap_or("unknown");
                        diagnostics.push(
                            elapsed,
                            format!("[-] Peer {} ({}) disconnected", event.peer, label),
                        );
                        commands.entity(entity).despawn();
                    }
                }
            }
        }
    }
}

/// Auto-decline and evict an [`IncomingOfferDialog`] that has been on
/// screen longer than [`config::network::OFFER_DIALOG_TIMEOUT_SECS`].
///
/// The busy-gate in `inbound::handle_incoming_messages` rejects further
/// offers while a dialog is active, so an attacker that ships a garbage
/// offer the user does not notice would otherwise lock the recipient out
/// of gifting for the rest of the session. Sending the responder
/// `ItemOfferResponse{accepted=false}` keeps the sender's pending state
/// in sync — without it, a benign sender's UI would sit waiting forever.
pub(super) fn evict_stale_offer_dialog(
    mut commands: Commands,
    dialog: Option<Res<IncomingOfferDialog>>,
    time: Res<Time>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    let Some(dialog) = dialog else {
        return;
    };
    let now = time.elapsed_secs_f64();
    if now - dialog.arrived_at_secs < config::network::OFFER_DIALOG_TIMEOUT_SECS {
        return;
    }
    writer.write(Broadcast {
        payload: OverlandsMessage::ItemOfferResponse {
            offer_id: dialog.offer_id,
            target_did: dialog.sender_did.clone(),
            accepted: false,
        },
        channel: ChannelKind::Reliable,
    });
    diagnostics.push(
        now,
        format!(
            "Auto-declined offer \"{}\" from @{} (timed out)",
            dialog.item_name, dialog.sender_handle
        ),
    );
    commands.remove_resource::<IncomingOfferDialog>();
}

/// Propagate each peer's mute flag to its `Visibility` component so that
/// muted vessels and their child meshes are hidden automatically.
pub(super) fn sync_mute_visibility(mut peers: Query<(&RemotePeer, &mut Visibility)>) {
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
