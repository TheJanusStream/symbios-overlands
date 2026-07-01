//! Peer connect/disconnect plumbing, mute-visibility sync, and the
//! stale-offer-dialog evictor. State-management systems that don't fit
//! the inbound-dispatch / outbound-broadcast pair.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::protocol::OverlandsMessage;
use crate::state::{IncomingOfferDialog, PendingOutgoingOffers, RemotePeer};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_peer_connections(
    mut commands: Commands,
    mut peer_events: ResMut<PeerStateQueue<OverlandsMessage>>,
    mut session_log: ResMut<SessionLog>,
    peers: Query<(Entity, &RemotePeer)>,
    time: Res<Time>,
    session: Option<Res<AtprotoSession>>,
    mut sender: SendMessage<OverlandsMessage>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
) {
    let elapsed = time.elapsed_secs_f64();
    for event in peer_events.drain() {
        match event.state {
            PeerConnectionState::Connected => {
                session_log.info(
                    elapsed,
                    EventPayload::PeerJoined {
                        peer: event.peer.to_string(),
                    },
                );
                crate::diagnostics::samplers::peer_connected(&mut metrics, elapsed);
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
                    sender.broadcast(
                        OverlandsMessage::Identity {
                            did: sess.did.clone(),
                            handle: sess.handle.clone(),
                        },
                        ChannelKind::Reliable,
                    );
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
                        session_log.info(
                            elapsed,
                            EventPayload::PeerLeft {
                                peer: event.peer.to_string(),
                                label: label.to_string(),
                            },
                        );
                        crate::diagnostics::samplers::peer_disconnected(&mut metrics, elapsed);
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
    mut session_log: ResMut<SessionLog>,
    mut sender: SendMessage<OverlandsMessage>,
) {
    let Some(dialog) = dialog else {
        return;
    };
    let now = time.elapsed_secs_f64();
    if now - dialog.arrived_at_secs < config::network::OFFER_DIALOG_TIMEOUT_SECS {
        return;
    }
    // Targeted reply: the original sender's PeerId is on the dialog
    // resource (recorded when the offer arrived), so we can return the
    // auto-decline directly to that peer rather than broadcasting it for
    // the room to filter out.
    sender.to(
        dialog.sender_peer_id,
        OverlandsMessage::ItemOfferResponse {
            offer_id: dialog.offer_id,
            target_did: dialog.sender_did.clone(),
            accepted: false,
        },
        ChannelKind::Reliable,
    );
    session_log.info(
        now,
        EventPayload::ItemOfferDialogAutoDeclinedTimeout {
            offer_id: dialog.offer_id,
        },
    );
    commands.remove_resource::<IncomingOfferDialog>();
}

/// Sweep [`PendingOutgoingOffers`] entries older than
/// [`config::network::PENDING_OFFER_TIMEOUT_SECS`]. A peer that drops the
/// reply (offline, malicious client, network hiccup) would otherwise leak
/// the entry forever — across a long session, an attacker could provoke
/// the local user into spraying offers and tie up unbounded memory.
pub(super) fn sweep_stale_pending_offers(
    time: Res<Time>,
    mut pending: ResMut<PendingOutgoingOffers>,
    mut session_log: ResMut<SessionLog>,
) {
    let now = time.elapsed_secs_f64();
    let ttl = config::network::PENDING_OFFER_TIMEOUT_SECS;
    let before = pending.by_id.len();
    if before == 0 {
        return;
    }
    let mut expired: Vec<u64> = Vec::new();
    pending.by_id.retain(|&id, entry| {
        let alive = now - entry.sent_at_secs < ttl;
        if !alive {
            expired.push(id);
        }
        alive
    });
    for offer_id in expired {
        // Info, not Warn: a peer not answering a gift offer within the TTL is a
        // benign, expected social outcome (AFK / implicit decline / brief hiccup)
        // — it mirrors the incoming-side `ItemOfferDialogAutoDeclinedTimeout`
        // above and must not inflate the offline analyzer's warning verdict.
        session_log.info(now, EventPayload::PendingOfferTimedOut { offer_id });
    }
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
