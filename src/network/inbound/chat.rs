//! `Chat` — a peer's text, and everything that has to be true before it
//! reaches the log (#1161).
//!
//! The most adversarial message in the protocol after `ItemOffer`, and the
//! only one a peer can send at will with arbitrary content. Four gates run
//! before a line is kept: the sender must have resolved a DID, must not be
//! muted, must be within their per-sender budget (#1222 f296), and the text
//! itself is clipped before it is stored.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use super::{InboundBuffers, PeerParts};
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::state::ChatHistory;
/// Accept, throttle or drop one peer's chat line.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle(
    sender: PeerId,
    text: String,
    chat: &mut ChatHistory,
    peers: &mut Query<PeerParts>,
    peer_sessions: &PeerSessionMapRes,
    session_log: &mut SessionLog,
    bufs: &mut InboundBuffers,
    now: f64,
) {
    // Authenticate the sender before anything else (#1218 f290).
    // This arm was, uniquely among the inbound arms, unauthenticated:
    // `Identity`, `AvatarStateUpdate`, `RoomStateUpdate`, `ItemOffer`
    // and `ItemOfferResponse` all resolve the sender against the
    // relay-signed session map, and chat alone fell back to
    // `sender.to_string()` — a raw PeerId UUID — as the author.
    // A peer that never identified therefore got an author name,
    // a chat channel, and a mute that could not be made durable:
    // the cheapest possible griefing posture is to say nothing.
    //
    // Deferring costs a legitimate early message nothing it was
    // not already going to pay — the peer re-broadcasts on its
    // identity cadence and the map catches up within a frame or
    // two, exactly as the `Identity` arm above assumes.
    let Some(sender_did) = peer_sessions.session_id(&sender) else {
        debug!("Dropping Chat from {}: peer session not yet known", sender);
        return;
    };

    // Flood control BEFORE anything else this arm does (#1222
    // f296). `Chat` is not in the coalescing set that protects
    // the three heavy variants from bursts, and the rolling
    // 500-entry history cap is exactly what makes a flood
    // destructive: 500 messages evict the room's whole prior
    // conversation, permanently, while the remedy is two windows
    // away. Charged per authenticated DID's peer id, so a
    // flooder cannot buy a fresh budget by re-sending.
    match bufs.chat_budgets.charge(sender, now) {
        crate::network::presence::ChatVerdict::Allow => {}
        crate::network::presence::ChatVerdict::Drop => return,
        crate::network::presence::ChatVerdict::DropAndReport { dropped } => {
            session_log.warn(
                now,
                EventPayload::ChatThrottled {
                    sender_did: sender_did.clone(),
                    dropped,
                },
            );
            return;
        }
    }

    // Ignore messages from muted peers.
    let sender_muted = peers
        .iter()
        .find(|(_, peer, _, _)| peer.peer_id == sender)
        .map(|(_, peer, _, _)| peer.muted)
        .unwrap_or(false);

    let sender_did_for_log = sender_did.clone();
    if sender_muted {
        // The mute worked — but silently, so a log could not tell
        // "nobody spoke" from "the person you muted did" (#1144).
        session_log.info(
            now,
            EventPayload::ChatDroppedMuted {
                sender_did: sender_did_for_log.clone(),
            },
        );
    }
    if !sender_muted {
        // Defend against over-long chat payloads from a malicious
        // peer: the local sender throttles via the chat UI, but a
        // hand-crafted packet could still ship an 800 KiB string
        // and lock every guest's renderer trying to word-wrap it.
        // Bytes, and deliberately so: this is the wire
        // backstop against a payload nobody typed (#1264
        // f362). The length a PERSON is held to is counted in
        // characters, and it is enforced in the composer.
        let max = crate::config::ui::chat::MAX_MESSAGE_BYTES;
        let clipped = if text.len() <= max {
            text
        } else {
            let mut end = max;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            text[..end].to_string()
        };
        // Strip ASCII control bytes (newlines, carriage returns,
        // form feeds, etc.) so a peer cannot inject multi-line
        // payloads that impersonate another author's rows in the
        // HUD log.
        let clipped: String = clipped
            .chars()
            .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
            .collect();
        let sender_peer = peers.iter().find(|(_, peer, _, _)| peer.peer_id == sender);
        // The relay-authenticated DID and the ONE naming ladder
        // (#1218 f290/f300) — the sender was already resolved
        // above, so this cannot be `None` here. The renderer
        // re-resolves the name by DID every frame
        // (`ui::chat::author_now`), so the string stamped here is
        // only the floor.
        let Some((did, author)) = crate::network::presence::chat_attribution(
            Some(sender_did.as_str()),
            sender_peer.and_then(|(_, peer, _, _)| peer.handle.as_deref()),
        ) else {
            return;
        };
        let did = Some(did);
        // Chat-keyword emotes (#1068): the sender's own body plays
        // the gesture their words asked for. Read off the CLIPPED,
        // control-stripped text — the same string the room is shown
        // — so a hostile peer cannot smuggle a trigger past the
        // sanitiser, and driven from the sender's chassis entity so
        // a muted peer stays silent in gesture as well as in text.
        if let Some((chassis, ..)) = sender_peer
            && let Some(request) = crate::player::emote::request_for(chassis, &clipped)
        {
            bufs.emotes.write(request);
        }
        // Length, never the text: the session log is an artefact
        // handed to an agent, and a room's conversation is not
        // diagnostic data (#1144).
        session_log.info(
            now,
            EventPayload::ChatReceived {
                sender_did: sender_did_for_log,
                text_len: clipped.len() as u32,
                muted: false,
            },
        );
        // Capped + wall-clock-stamped (#846).
        chat.push(did, author, clipped);
        // With the window closed this message would be
        // invisible — count it for the toolbar badge (#835).
        if !bufs.panels.chat {
            chat.unread += 1;
        }
    }
}
