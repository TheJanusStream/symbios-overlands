//! Peer-to-peer gifting: the offer and the answer (#1161).
//!
//! The largest pair of arms in the protocol and the one with the most gates,
//! because an `ItemOffer` asks this client to put a stranger's generator on
//! screen and, if accepted, into the owner's stash. Between the wire and the
//! dialog sit: the relay's DID binding, the mute list, the inventory cap,
//! the busy-gate (one dialog at a time, #843/#1220 f288) and
//! `pds::inventory::is_drop_placeable` — the rule that decides whether a
//! stranger's generator is safe to place at all.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use super::{InboundBuffers, PeerParts};
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::network::presence::PeerLabel;
use crate::protocol::{DeclineReason, OverlandsMessage};
use crate::state::{IncomingOfferDialog, PendingOutgoingOffers};
/// Decide what to do with a stranger's gift: refuse it, auto-decline it, or
/// raise the dialog that asks the owner.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle(
    sender: PeerId,
    offer_id: u64,
    target_did: String,
    payload_json: Vec<u8>,
    commands: &mut Commands,
    peers: &mut Query<PeerParts>,
    peer_sessions: &PeerSessionMapRes,
    session: &Option<Res<AtprotoSession>>,
    session_log: &mut SessionLog,
    out: &mut SendMessage<OverlandsMessage>,
    metrics: &mut crate::diagnostics::MetricsRegistry,
    bufs: &mut InboundBuffers,
    now: f64,
    // Whether a dialog is already up (or staged) this frame. `Commands` do
    // not apply until end-of-system, so re-reading the resource would report
    // the stale pre-frame state and let a peer pack many offers into one
    // frame, bypassing the busy-gate.
    dialog_open: &mut bool,
) {
    // Broadcast-with-address: only the peer whose DID matches
    // `target_did` should act on the offer. Everyone else
    // silently drops it because `bevy_symbios_multiuser` has no
    // directed-send primitive.
    let Some(sess) = session.as_deref() else {
        return;
    };
    if sess.did != target_did {
        return;
    }

    // Authenticate the sender's DID against the relay-signed
    // PeerSessionMap — same defence the Identity handler uses.
    // A `None` lookup means the peer connected before its
    // session bound; defer by dropping the message (the sender
    // can retry).
    let Some(sender_did) = peer_sessions.session_id(&sender) else {
        debug!(
            "Deferring ItemOffer from {}: peer session not yet known",
            sender
        );
        return;
    };

    // Silent auto-decline for muted senders. The sender still
    // gets a response so their UI clears the pending state, but
    // no dialog is shown and no diagnostics entry is written —
    // muted senders should be invisible by design.
    let peer_lookup = peers.iter().find(|(_, peer, _, _)| peer.peer_id == sender);
    let sender_muted = peer_lookup
        .as_ref()
        .map(|(_, peer, _, _)| peer.muted)
        .unwrap_or(false);
    // The ONE ladder (#1218 f299): a handle if the profile
    // fetch has landed, the DID's head otherwise — and never the
    // DID dressed up as a name.
    let sender_label = PeerLabel::new(
        peer_lookup
            .as_ref()
            .and_then(|(_, peer, _, _)| peer.handle.as_deref()),
        Some(sender_did.as_str()),
    );
    let sender_name = sender_label.addressed();

    if sender_muted {
        out.to(
            sender,
            // Reported as a plain decline on purpose (#1220
            // f127): telling somebody they have been muted is a
            // privacy leak, and a muted sender should be unable
            // to tell a mute from a refusal.
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Declined,
            ),
            ChannelKind::Reliable,
        );
        return;
    }

    // Decode the payload. Deliberately AFTER the muted and
    // busy gates (#1184): this is a broadcast with an
    // address, so every peer in the room receives every
    // gift, and parsing a blueprint the recipient is about
    // to auto-decline is work nobody asked for. The envelope
    // carries everything those two gates need.
    //
    // A malformed payload — or an Unknown generator variant
    // — is a protocol error: auto-decline and log.
    let Some(payload) = OverlandsMessage::decode_item_offer(&payload_json) else {
        out.to(
            sender,
            // Not the recipient's choice: this client could not
            // read the gift, which most often means the two
            // builds disagree about the wire.
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Unavailable,
            ),
            ChannelKind::Reliable,
        );
        session_log.warn(
            now,
            EventPayload::ItemOfferDecodeFailed {
                reason: format!("from {sender_name}: failed to decode"),
            },
        );
        return;
    };
    let mut generator = payload.generator;

    // Clamp the wire-supplied item name *before* any
    // diagnostics or dialog state references it. The
    // protocol field is an unbounded `String`, so a hostile
    // sender can ship a 10 MiB blob; spamming such offers
    // at a busy victim would otherwise force the main
    // thread to allocate that string into every busy-gate
    // / rejection log line. Clamping up front guarantees
    // the rest of this handler only sees a bounded value.
    let item_name = {
        let mut n: String = payload
            .item_name
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        if n.is_empty() {
            n.push_str("(unnamed)");
        }
        n
    };

    // Busy-gate: a dialog is already up (or was staged earlier
    // this same frame), so an attacker can't queue-flood the
    // recipient with nested prompts. Decline and log so the
    // user knows someone tried.
    if *dialog_open {
        out.to(
            sender,
            // The finding's headline case (#1220 f127): a
            // mechanical throttle reaching the sender as "they
            // declined" misattributes it to a person AND teaches
            // them not to retry, in the one case where retrying
            // in ten seconds works.
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Busy,
            ),
            ChannelKind::Reliable,
        );
        session_log.info(now, EventPayload::ItemOfferAutoDeclinedBusy { offer_id });
        crate::diagnostics::samplers::offer_auto_declined_busy(metrics);
        // Counted for the dialog's closing note (#843) — the
        // decline itself stays invisible until then, preserving
        // the single-dialog anti-spam invariant.
        bufs.busy_declines.0 = bufs.busy_declines.0.saturating_add(1);
        return;
    }

    crate::pds::sanitize_generator(&mut generator);
    // Wear metadata (#1108) through the same clamp the inventory
    // and attachment records share. A `wear` the payload could
    // not read is already `None` by the time it gets here — the
    // payload's own lenient decoder degrades a bad one to decor
    // rather than refusing the gift (#1184).
    let wear = payload.wear.map(|mut meta| {
        meta.sanitize();
        meta
    });

    // Non-placeable kinds (terrain / water / Unknown) never
    // make sense as a gift — the sender UI already filters
    // these, but reject here too so a hand-crafted payload
    // can't stuff an unplaceable item into the recipient's
    // stash via the accept path.
    if !crate::pds::inventory::is_drop_placeable(&generator) {
        out.to(
            sender,
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Unavailable,
            ),
            ChannelKind::Reliable,
        );
        session_log.warn(
            now,
            EventPayload::ItemOfferRejected {
                offer_id,
                reason: format!("from {sender_name}: item kind not giftable"),
            },
        );
        return;
    }

    session_log.info(
        now,
        EventPayload::ItemOfferReceived {
            offer_id,
            sender_did: sender_did.clone(),
            item_name: item_name.clone(),
        },
    );
    commands.insert_resource(IncomingOfferDialog {
        offer_id,
        sender_peer_id: sender,
        sender_did,
        sender_label,
        item_name,
        generator,
        wear,
        arrived_at_secs: now,
        // The TTL and the countdown run on wall clock (#1216);
        // the virtual stamp above stays for the session log.
        arrived_at_epoch: crate::state::now_epoch_secs(),
    });
    // Slam the gate shut for the rest of this frame so any
    // further offers in the same drain auto-decline instead of
    // racing the deferred `insert_resource` above.
    *dialog_open = true;
}

/// Land the answer to a gift THIS client offered: toast the outcome and
/// retire the pending record.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_response(
    sender: PeerId,
    offer_id: u64,
    target_did: String,
    payload_json: Vec<u8>,
    peer_sessions: &PeerSessionMapRes,
    session: &Option<Res<AtprotoSession>>,
    session_log: &mut SessionLog,
    pending_offers: &mut PendingOutgoingOffers,
    bufs: &mut InboundBuffers,
    now: f64,
) {
    // Gate on the local DID first: a response broadcast is
    // carrying our own sender-side offer_id only when
    // `target_did` equals our DID. Other peers drop it.
    let Some(sess) = session.as_deref() else {
        return;
    };
    if sess.did != target_did {
        return;
    }

    // Authenticate the responder's DID against the relay map
    // so a third-party peer can't impersonate the real
    // recipient and spoof an "accepted" reply to steal
    // visibility into what we gifted.
    let Some(responder_did) = peer_sessions.session_id(&sender) else {
        return;
    };

    // Authenticate the responder's DID against the pending
    // offer's target BEFORE consuming the pending entry. A
    // prior implementation removed unconditionally, letting any
    // peer in the room race a spoofed "accepted" reply onto the
    // wire and silently delete the genuine target's pending
    // offer — permanently breaking gifting for the sender.
    match pending_offers.by_id.get(&offer_id) {
        Some(pending) if pending.target_did != responder_did => return,
        None => return,
        _ => {}
    }

    // Consume the pending entry now that the responder is
    // authenticated — its handle + item name feed the sender's
    // outcome toast (#843).
    let Some(pending) = pending_offers.by_id.remove(&offer_id) else {
        return;
    };

    // A payload this build cannot read is NOT an acceptance:
    // `ItemOfferResponsePayload::accepted` defaults to false
    // and an outright decode failure declines too (#1184), so
    // the worst a skewed peer can do is make a gift that
    // arrived look declined — never the reverse.
    let decoded = OverlandsMessage::decode_item_offer_response(&payload_json);
    let accepted = decoded.as_ref().is_some_and(|payload| payload.accepted);
    let reason = decoded.map(|payload| payload.reason).unwrap_or_default();

    session_log.info(
        now,
        EventPayload::ItemOfferResponseReceived { offer_id, accepted },
    );
    // The sender finally learns the outcome somewhere visible
    // (#843), and now learns WHICH outcome (#1220 f127): the
    // boolean used to render one sentence for a refusal, a
    // throttle, a mute and a timeout alike.
    if accepted {
        bufs.toasts.success(
            format!(
                "{} accepted \"{}\".",
                pending.target_label, pending.item_name
            ),
            now,
        );
    } else {
        bufs.toasts.info(
            crate::protocol::offer_refusal_line(reason, &pending.target_label, &pending.item_name),
            now,
        );
    }
}
