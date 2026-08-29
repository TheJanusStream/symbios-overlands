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
    mut notices: ResMut<super::chunk::OversizeNotices>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
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
                        build: None,
                        connected_at: elapsed,
                    },
                    TransformBuffer::default(),
                ));

                // Announce our wire layout to the newcomer immediately, for
                // the reason the identity announce below gives — except that
                // this one also matters in the failing direction: if we wait
                // for the scheduled broadcast, a peer whose build predates
                // #1121 and a peer whose Hello is merely in flight look
                // identical for a whole second.
                sender.broadcast(
                    OverlandsMessage::Hello {
                        protocol: crate::protocol::PROTOCOL_VERSION,
                        build: crate::protocol::build_id(),
                    },
                    ChannelKind::Reliable,
                );

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
                        if let Some(message) = OverlandsMessage::room_state_update(&record.0) {
                            // A refusal here is the same fact as the
                            // broadcaster's, learned on a different trigger
                            // (#1123) — this newcomer will see only the
                            // PDS-saved room. Shares the "world" latch, so
                            // an owner already warned by the broadcaster is
                            // not told again per arriving guest.
                            super::chunk::warn_once_on_refusal(
                                super::chunk::send_chunked(
                                    &mut sender,
                                    &mut seq,
                                    &mut metrics,
                                    &mut session_log,
                                    super::chunk::ChunkDest::To(event.peer),
                                    elapsed,
                                    message,
                                ),
                                &mut notices,
                                &mut toasts,
                                "world",
                                elapsed,
                            );
                        }
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
/// Report a peer that never announced a wire protocol (#1121).
///
/// This is the arm that catches the incompatibility that already exists.
/// Every build shipped before the `Hello` handshake announces nothing — and a
/// GitHub-Pages wasm app serves cached bundles for as long as a browser keeps
/// them, so "the other end is an older build" is not a migration window, it is
/// the steady state. A version field alone would never fire for those peers,
/// because the mismatch is precisely that they have no version to send.
///
/// One event per peer per session, and never one for a peer that is merely
/// still connecting: `Hello` goes out on connect and again every second, so
/// the grace period is several missed announcements, not one.
pub(super) fn flag_unannounced_peers(
    peers: Query<&RemotePeer>,
    time: Res<Time>,
    mut session_log: ResMut<SessionLog>,
    mut reported: Local<std::collections::HashSet<PeerId>>,
) {
    let now = time.elapsed_secs_f64();
    for peer in peers.iter() {
        if peer.compatibility(now) != crate::state::PeerCompatibility::Unannounced
            || !reported.insert(peer.peer_id)
        {
            continue;
        }
        warn!(
            "Peer {} announced no protocol within {}s — it is running a build from before \
             the wire handshake, so messages between us may not decode",
            peer.peer_id,
            config::network::PROTOCOL_ANNOUNCE_GRACE_SECS
        );
        session_log.error(
            now,
            EventPayload::PeerProtocolMismatch {
                peer: peer.peer_id.to_string(),
                ours: crate::protocol::PROTOCOL_VERSION,
                theirs: None,
                build: String::from("pre-handshake"),
            },
        );
    }
}

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
