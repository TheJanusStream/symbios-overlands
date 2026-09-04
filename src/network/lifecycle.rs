//! Peer connect/disconnect plumbing, mute-visibility sync, and the
//! stale-offer-dialog evictor. State-management systems that don't fit
//! the inbound-dispatch / outbound-broadcast pair.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::protocol::{DeclineReason, OverlandsMessage};
use crate::state::{
    CurrentRoomDid, HeldOffer, IncomingOfferDialog, LiveRoomRecord, PendingOutgoingOffers,
    RemotePeer,
};

use super::presence::{PeerLabel, PeerResolve};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_peer_connections(
    mut commands: Commands,
    mut peer_events: ResMut<PeerStateQueue<OverlandsMessage>>,
    mut session_log: ResMut<SessionLog>,
    peers: Query<(Entity, &RemotePeer, &PeerResolve)>,
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
    link: Res<super::LinkState>,
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
                // `RemotePeer::avatar`. Not guessing at their appearance is
                // still deliberate: a synthesised default is
                // indistinguishable from a deliberately-minimal avatar and
                // misleads the room about how someone really looks.
                //
                // What changed (#1217) is that "don't guess" stopped meaning
                // "draw nothing for several seconds". The peer wears a
                // translucent stand-in (`presence::dress_peer_placeholders`)
                // that cannot be mistaken for anybody, and the whole chassis
                // stays `Hidden` until a transform sample has actually
                // played out — the spawn pose below is the map centre ten
                // metres up, and it used to be drawn.
                commands.spawn((
                    Transform::from_xyz(0.0, 10.0, 0.0),
                    Visibility::Hidden,
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
                    PeerResolve::default(),
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
                for (entity, peer, resolve) in peers.iter() {
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
                        // the handle resolves (avatar.rs), or when the
                        // profile fetch fails and there is no better name
                        // coming. Departures print here with the best name we
                        // ever learned, off the SAME ladder (#1218 f338):
                        // this arm invented it and three other surfaces then
                        // invented worse ones.
                        let name =
                            PeerLabel::new(peer.handle.as_deref(), peer.did.as_deref()).addressed();
                        // A departure observed while OUR link is down is
                        // not attributable to the peer (#1213 f402): the
                        // one narrative the user ever got about a
                        // connectivity event was "everybody left", which is
                        // a social conclusion about the wrong actor. The
                        // single "Connection lost — rejoining…" line
                        // `link::narrate_link_state` pushes on the teardown
                        // edge replaces the whole run of them. The session
                        // log entry and the despawn happen either way.
                        //
                        // And only for someone the room was told about
                        // (#1218 f338): the presence log has to balance, and
                        // a farewell to a peer whose arrival was never
                        // announced was the one user-visible trace a
                        // nameless peer ever left.
                        if super::presence::should_announce_departure(
                            link.is_up(),
                            resolve.announced,
                            peer.muted,
                        ) {
                            chat.push(None, "system", format!("{name} left the room."));
                        }
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
    // Wall clock, not `Res<Time>` (#1216): the sender is counting the same
    // 90 seconds on their own machine, and the virtual clock stops while
    // this one is asleep or backgrounded. `now` above is still the session
    // log's clock, which every other stamp in this module shares.
    if crate::state::real_secs_since(dialog.arrived_at_epoch)
        < config::network::OFFER_DIALOG_TIMEOUT_SECS
    {
        return;
    }
    // Targeted reply: the original sender's PeerId is on the dialog
    // resource (recorded when the offer arrived), so we can return the
    // auto-decline directly to that peer rather than broadcasting it for
    // the room to filter out.
    sender.to(
        dialog.sender_peer_id,
        // Nobody said no — nobody said anything (#1220 f127). The sender's
        // own expiry sweep says the same thing from the other side, so the
        // two ends now agree about what happened.
        OverlandsMessage::item_offer_response(
            dialog.offer_id,
            dialog.sender_did.clone(),
            false,
            DeclineReason::Unanswered,
        ),
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
            "Offer of \"{}\" from {} expired unanswered — declined.",
            dialog.item_name,
            dialog.sender_label.addressed()
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
    link: Res<super::LinkState>,
) {
    let now = time.elapsed_secs_f64();
    let ttl = config::network::PENDING_OFFER_TIMEOUT_SECS;
    let before = pending.by_id.len();
    if before == 0 {
        return;
    }
    // Handle + item ride along for the sender's expiry toast (#843), and
    // `sent_at_secs` for the link check the wording now turns on (#1213).
    let mut expired: Vec<(u64, String, String, f64)> = Vec::new();
    pending.by_id.retain(|&id, entry| {
        // Wall clock (#1216) — the recipient's dialog is counting down on
        // theirs, and two machines that slept for different lengths must
        // not settle on opposite outcomes for one offer_id.
        let alive = crate::state::real_secs_since(entry.sent_at_epoch) < ttl;
        if !alive {
            expired.push((
                id,
                entry.target_label.clone(),
                entry.item_name.clone(),
                entry.sent_at_secs,
            ));
        }
        alive
    });
    for (offer_id, handle, item, sent_at) in expired {
        // Info, not Warn: a peer not answering a gift offer within the TTL is a
        // benign, expected social outcome (AFK / implicit decline / brief hiccup)
        // — it mirrors the incoming-side `ItemOfferDialogAutoDeclinedTimeout`
        // above and must not inflate the offline analyzer's warning verdict.
        session_log.info(now, EventPayload::PendingOfferTimedOut { offer_id });
        // The old wording asserted a fact about the RECIPIENT on a sweep
        // that checked nothing about connectivity (#1213 f404). "They
        // didn't answer" is a social signal the user acts on; when our own
        // link was down for any part of the offer's life the offer never
        // left this machine. `LinkState` owns both sentences.
        toasts.info(link.offer_expiry_line(sent_at, &item, &handle), now);
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
        // A mute reports as a plain decline (#1220 f127).
        OverlandsMessage::item_offer_response(
            dialog.offer_id,
            dialog.sender_did.clone(),
            false,
            DeclineReason::Declined,
        ),
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

pub(super) fn sync_mute_visibility(mut peers: Query<(&RemotePeer, &PeerResolve, &mut Visibility)>) {
    for (peer, resolve, mut vis) in peers.iter_mut() {
        // Two reasons a peer is not drawn, resolved in one place because
        // exactly one system may own `Visibility` (#1217 f329). The second
        // is the spawn pose: every peer is spawned at the map centre ten
        // metres up, and `smooth_remote_transforms` — which runs immediately
        // before this — overwrites it only once the jitter buffer can
        // produce a pose. Until then there is nothing true to draw.
        let desired = if peer.muted || !resolve.placed {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != desired {
            *vis = desired;
        }
    }
}

/// Drive a [`HeldOffer`] to one of its two ends (#1220 f288): back onto the
/// screen when a slot frees, or declined when the clock runs out.
///
/// The hold exists because the dialog's own "Open Inventory" button raised a
/// window its modal blocked — so the escape hatch on the failure path of a
/// core journey led nowhere, under a timer. Holding lets the Inventory
/// actually be used; this system is what makes the hold end.
///
/// The TTL is unchanged and runs on the wall clock (#1216), because the
/// SENDER is counting the same ninety seconds: a hold must not be a way to
/// keep an offer alive past the point where the other end has given up on it.
/// What should become of a [`HeldOffer`] this frame (#1220 f288).
///
/// Three-way, and the order is the point: the TTL wins over a freed slot,
/// because the SENDER is counting the same ninety seconds and a hold must
/// not be a way to keep an offer alive past the point where the other end
/// has given up on it. A dialog already on screen wins over both — the hold
/// is owed the screen, not competing for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldOfferOutcome {
    /// Another offer is on screen; the hold waits its turn.
    Wait,
    /// The clock ran out while the user was making room.
    Expire,
    /// A slot freed: put it back in front of them.
    Represent,
}

/// The pure half of [`resolve_held_offer`].
pub fn held_offer_outcome(dialog_open: bool, expired: bool, has_room: bool) -> HeldOfferOutcome {
    if expired {
        return HeldOfferOutcome::Expire;
    }
    if dialog_open {
        return HeldOfferOutcome::Wait;
    }
    if has_room {
        return HeldOfferOutcome::Represent;
    }
    HeldOfferOutcome::Wait
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_held_offer(
    mut commands: Commands,
    held: Option<Res<HeldOffer>>,
    dialog: Option<Res<IncomingOfferDialog>>,
    live_inventory: Option<Res<crate::state::LiveInventoryRecord>>,
    time: Res<Time>,
    mut session_log: ResMut<SessionLog>,
    mut sender: SendMessage<OverlandsMessage>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
) {
    let Some(held) = held else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let outcome = held_offer_outcome(
        dialog.is_some(),
        crate::state::real_secs_since(held.0.arrived_at_epoch)
            >= config::network::OFFER_DIALOG_TIMEOUT_SECS,
        live_inventory.as_deref().is_some_and(|live| {
            live.0.generators.len() < crate::config::state::MAX_INVENTORY_ITEMS
        }),
    );
    if outcome == HeldOfferOutcome::Expire {
        sender.to(
            held.0.sender_peer_id,
            // `Unavailable`, not `Unanswered` (#1220 f127): the recipient
            // did answer — they went to make room and did not manage it in
            // time. That is a different sentence for the sender, and an
            // actionable one.
            OverlandsMessage::item_offer_response(
                held.0.offer_id,
                held.0.sender_did.clone(),
                false,
                crate::protocol::DeclineReason::Unavailable,
            ),
            ChannelKind::Reliable,
        );
        session_log.info(
            now,
            EventPayload::ItemOfferDialogAutoDeclinedTimeout {
                offer_id: held.0.offer_id,
            },
        );
        toasts.info(
            format!(
                "\"{}\" from {} expired while you were making room — declined.",
                held.0.item_name,
                held.0.sender_label.addressed()
            ),
            now,
        );
        commands.remove_resource::<HeldOffer>();
        return;
    }
    if outcome == HeldOfferOutcome::Represent {
        commands.insert_resource(held.0.clone());
        commands.remove_resource::<HeldOffer>();
    }
}

#[cfg(test)]
mod liveness_tests {
    use super::*;

    /// #1224 f335. The sequence: a peer's browser tab is backgrounded and
    /// keeps its data channel open, so the transport never raises
    /// `Disconnected` — the ONE path that despawned a peer — and their body
    /// stands frozen indefinitely, counted in the roster, sorted into it,
    /// and offered as a gift and Visit target that will never answer. The
    /// client already knew: the jitter buffer stopped receiving. It just
    /// never asked.
    #[test]
    fn silence_is_read_in_two_tiers() {
        let quiet = config::network::PEER_QUIET_SECS;
        let ghost = config::network::PEER_GHOST_SECS;
        assert!(quiet < ghost, "the tiers have to be ordered to be tiers");

        assert_eq!(liveness(100.0, 100.0), Liveness::Live);
        assert_eq!(liveness(100.0, 100.0 + quiet - 0.001), Liveness::Live);
        assert_eq!(
            liveness(100.0, 100.0 + quiet),
            Liveness::Quiet,
            "say so on the row, but leave them standing"
        );
        assert_eq!(liveness(100.0, 100.0 + ghost - 0.001), Liveness::Quiet);
        assert_eq!(
            liveness(100.0, 100.0 + ghost),
            Liveness::Gone,
            "past anything a hiccup explains"
        );
    }

    /// A clock that steps backwards must not read as silence: `max(0.0)`
    /// means a rewound clock says Live, never Gone. Despawning a peer on a
    /// clock artefact would be the worst possible false positive.
    #[test]
    fn a_backwards_clock_never_sweeps_anybody() {
        assert_eq!(liveness(1000.0, 0.0), Liveness::Live);
    }

    /// A peer that has NEVER sent a transform ages from `connected_at`, so
    /// the case the transport most often fails to report — a peer that
    /// arrives and then wedges — is swept on the same clock as one that
    /// stops mid-conversation.
    #[test]
    fn a_peer_that_never_speaks_ages_from_its_arrival() {
        let resolve = PeerResolve::default();
        let connected_at = 10.0;
        let last_heard = resolve.last_sample_at.unwrap_or(connected_at);
        assert_eq!(last_heard, connected_at);
        assert_eq!(
            liveness(last_heard, connected_at + config::network::PEER_GHOST_SECS),
            Liveness::Gone,
        );
    }
}

#[cfg(test)]
mod held_offer_tests {
    use super::*;

    /// #1220 f288. The sequence: your stash is full, a friend gifts you
    /// something, and the dialog's own "Open Inventory" button raises the
    /// Inventory UNDER a modal that swallows every click on it — while the
    /// countdown declines the gift out from under you. The only way to
    /// reach the Inventory was to Decline first, which is the outcome the
    /// button exists to avoid.
    #[test]
    fn a_held_offer_comes_back_when_a_slot_frees() {
        assert_eq!(
            held_offer_outcome(false, false, true),
            HeldOfferOutcome::Represent,
        );
        assert_eq!(
            held_offer_outcome(false, false, false),
            HeldOfferOutcome::Wait,
            "still full: keep holding rather than declining on the user's behalf"
        );
    }

    /// The TTL outranks a freed slot, because the SENDER is counting the
    /// same ninety seconds: a hold must not become a way to keep an offer
    /// alive past the point where the other end has given up on it and
    /// toasted that nobody answered.
    #[test]
    fn a_held_offer_still_expires_on_the_senders_clock() {
        assert_eq!(
            held_offer_outcome(false, true, true),
            HeldOfferOutcome::Expire,
            "a slot freeing one frame too late does not revive the offer"
        );
        assert_eq!(
            held_offer_outcome(true, true, true),
            HeldOfferOutcome::Expire
        );
    }

    /// A dialog already on screen outranks a freed slot: the hold is owed
    /// the screen, not competing for it, and re-presenting underneath would
    /// let one answer land on the other offer.
    #[test]
    fn a_held_offer_waits_for_the_screen() {
        assert_eq!(
            held_offer_outcome(true, false, true),
            HeldOfferOutcome::Wait
        );
    }
}

/// What a peer's silence means at `now` (#1224 f335).
///
/// `last_sample_at` is `None` until a peer's first transform packet, so a
/// peer that never speaks ages from `connected_at` and is swept on the same
/// clock as one that stops — which is the case the transport most often
/// fails to report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
    /// Packets are arriving.
    Live,
    /// Nothing for a while — say so on their row, but leave them standing.
    Quiet,
    /// Long past anything a hiccup explains: they are a ghost.
    Gone,
}

/// The pure half of [`sweep_quiet_peers`].
pub fn liveness(last_heard: f64, now: f64) -> Liveness {
    let silence = (now - last_heard).max(0.0);
    if silence >= config::network::PEER_GHOST_SECS {
        Liveness::Gone
    } else if silence >= config::network::PEER_QUIET_SECS {
        Liveness::Quiet
    } else {
        Liveness::Live
    }
}

/// Notice peers the transport never told us about (#1224 f335).
///
/// A peer entity was despawned by exactly one path — the transport's
/// `Disconnected` arm above — and the client had no liveness check of its
/// own, so a wedged data channel or a suspended browser tab left a body
/// standing frozen indefinitely: counted in the roster, sorted into it, and
/// offered as a gift and Visit target that would never answer. A ghost is
/// worse than an absence, because it makes the room look occupied.
///
/// Runs on `Res<Time>` — the VIRTUAL clock — deliberately, and this is the
/// one deadline in this module that should not use the wall clock. Nobody
/// on the other end is counting it: it measures OUR silence. If this
/// machine sleeps, the virtual clock barely advances and no peer is falsely
/// aged, which is exactly right — we were not listening.
pub(super) fn sweep_quiet_peers(
    mut commands: Commands,
    mut peers: Query<(Entity, &RemotePeer, &mut PeerResolve)>,
    time: Res<Time>,
    link: Res<super::LinkState>,
    mut chat: ResMut<crate::state::ChatHistory>,
    mut session_log: ResMut<SessionLog>,
) {
    // Our own outage is not their silence (#1213 f402), and sweeping the
    // room while the socket is down would narrate a connectivity event as
    // everybody leaving — the exact conclusion about the wrong actor that
    // `link::narrate_link_state` exists to replace.
    if !link.is_up() {
        return;
    }
    let now = time.elapsed_secs_f64();
    for (entity, peer, mut resolve) in peers.iter_mut() {
        let last_heard = resolve.last_sample_at.unwrap_or(peer.connected_at);
        match liveness(last_heard, now) {
            Liveness::Live => PeerResolve::set_quiet(&mut resolve, false),
            Liveness::Quiet => PeerResolve::set_quiet(&mut resolve, true),
            Liveness::Gone => {
                let label = PeerLabel::new(peer.handle.as_deref(), peer.did.as_deref());
                session_log.info(
                    now,
                    EventPayload::PeerLeft {
                        peer: peer.peer_id.to_string(),
                        label: label.name(),
                    },
                );
                // The same sentence the disconnect path writes, under the
                // same rule (#1218 f338 / #1219 f289): a room told about an
                // arrival is told about the departure, and a muted person is
                // told about neither.
                if super::presence::should_announce_departure(
                    link.is_up(),
                    resolve.announced,
                    peer.muted,
                ) {
                    chat.push(
                        None,
                        "system",
                        format!("{} left the room.", label.addressed()),
                    );
                }
                commands.entity(entity).despawn();
            }
        }
    }
}
