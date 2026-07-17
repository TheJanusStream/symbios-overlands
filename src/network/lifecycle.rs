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
use crate::state::{
    CurrentRoomDid, IncomingOfferDialog, LiveRoomRecord, PendingOutgoingOffers, RemotePeer,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_peer_connections(
    mut commands: Commands,
    mut peer_events: ResMut<PeerStateQueue<OverlandsMessage>>,
    mut session_log: ResMut<SessionLog>,
    peers: Query<(Entity, &RemotePeer)>,
    time: Res<Time>,
    session: Option<Res<AtprotoSession>>,
    room_record: Option<Res<LiveRoomRecord>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut sender: SendMessage<OverlandsMessage>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut seq: ResMut<super::chunk::OutboundChunkSeq>,
    mut chat: ResMut<crate::state::ChatHistory>,
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
                crate::diagnostics::samplers::peer_connected(&mut metrics);
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

                    // If we own this room, push our current (possibly unsaved)
                    // room state to the newcomer so live edits made before they
                    // connected are visible immediately. Without this they only
                    // ever see the PDS-saved version loaded on entry, so a
                    // portal (or any generator) the owner added while the guest
                    // was away — or during a dropped connection — stays hidden
                    // until the owner saves *and* the guest reloads (#713).
                    // Targeted (not broadcast): existing peers already mirror
                    // it. Ordered after the `Identity` above on the reliable
                    // channel (`transmit_messages` runs before
                    // `transmit_directed_messages`), so the newcomer records our
                    // DID before it authenticates this update against the room
                    // owner — the exact reason the identity announce precedes it.
                    if let (Some(record), Some(rd)) = (&room_record, &room_did)
                        && sess.did == rd.0
                    {
                        // Chunked (#718): a large room's `room_state_update`
                        // exceeds the 64 KiB WebRTC message ceiling, and this
                        // directed push previously failed silently
                        // (`ErrOutboundPacketTooLarge`) — so a guest joining a
                        // large room never received it and saw only the stale
                        // PDS version (or nothing). Fragmenting it here is what
                        // makes the join actually deliver the live room.
                        super::chunk::send_chunked(
                            &mut sender,
                            &mut seq,
                            &mut metrics,
                            &mut session_log,
                            super::chunk::ChunkDest::To(event.peer),
                            elapsed,
                            OverlandsMessage::room_state_update(&record.0),
                        );
                    }
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
                        crate::diagnostics::samplers::peer_disconnected(&mut metrics);
                        // Presence line (#844) — the join side prints when
                        // the handle resolves (avatar.rs); departures print
                        // here with the best name we ever learned. A peer
                        // that never identified gets a generic line rather
                        // than a raw PeerId nobody recognises.
                        let name = match (peer.handle.as_deref(), peer.did.as_deref()) {
                            (Some(handle), _) => format!("@{handle}"),
                            (None, Some(did)) => {
                                let head: String = did.chars().take(16).collect();
                                format!("{head}…")
                            }
                            (None, None) => "A traveler".to_owned(),
                        };
                        chat.push(None, "system", format!("{name} left the room."));
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
    mut busy_declines: ResMut<crate::state::BusyAutoDeclines>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
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
    // The dialog is closing (#843): report anything the busy-gate turned
    // away while it sat unanswered, then reset the counter for the next
    // dialog. The eviction itself gets a line too — it used to vanish
    // invisibly mid-decision.
    toasts.info(
        format!(
            "Offer of \"{}\" from @{} expired unanswered — declined.",
            dialog.item_name, dialog.sender_handle
        ),
        now,
    );
    if busy_declines.0 > 0 {
        toasts.info(
            format!(
                "{} more offer{} arrived while it waited and {} auto-declined.",
                busy_declines.0,
                if busy_declines.0 == 1 { "" } else { "s" },
                if busy_declines.0 == 1 { "was" } else { "were" },
            ),
            now,
        );
        busy_declines.0 = 0;
    }
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
    mut toasts: ResMut<crate::ui::toast::Toasts>,
) {
    let now = time.elapsed_secs_f64();
    let ttl = config::network::PENDING_OFFER_TIMEOUT_SECS;
    let before = pending.by_id.len();
    if before == 0 {
        return;
    }
    // Handle + item ride along for the sender's expiry toast (#843).
    let mut expired: Vec<(u64, String, String)> = Vec::new();
    pending.by_id.retain(|&id, entry| {
        let alive = now - entry.sent_at_secs < ttl;
        if !alive {
            expired.push((id, entry.target_handle.clone(), entry.item_name.clone()));
        }
        alive
    });
    for (offer_id, handle, item) in expired {
        // Info, not Warn: a peer not answering a gift offer within the TTL is a
        // benign, expected social outcome (AFK / implicit decline / brief hiccup)
        // — it mirrors the incoming-side `ItemOfferDialogAutoDeclinedTimeout`
        // above and must not inflate the offline analyzer's warning verdict.
        session_log.info(now, EventPayload::PendingOfferTimedOut { offer_id });
        toasts.info(
            format!("Offer of \"{item}\" to @{handle} expired without an answer."),
            now,
        );
    }
}

/// Dismiss an open offer dialog whose sender was just muted (#844): the
/// People-window mute checkbox used to leave the dialog lingering — only
/// the dialog's own "Mute & Decline" button closed it. Runs on
/// `Changed<RemotePeer>` (the mute writes are already change-guarded, so
/// this reacts only to real flips) and returns the same authenticated
/// decline the other close paths send, keeping the sender's pending
/// state in sync.
pub(super) fn dismiss_offer_dialog_from_muted_sender(
    mut commands: Commands,
    dialog: Option<Res<IncomingOfferDialog>>,
    changed_peers: Query<&RemotePeer, Changed<RemotePeer>>,
    mut sender: SendMessage<OverlandsMessage>,
    mut session_log: ResMut<SessionLog>,
    time: Res<Time>,
) {
    let Some(dialog) = dialog else {
        return;
    };
    let sender_now_muted = changed_peers
        .iter()
        .any(|peer| peer.peer_id == dialog.sender_peer_id && peer.muted);
    if !sender_now_muted {
        return;
    }
    let now = time.elapsed_secs_f64();
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
        EventPayload::ItemOfferUserResponded {
            offer_id: dialog.offer_id,
            accepted: false,
        },
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
