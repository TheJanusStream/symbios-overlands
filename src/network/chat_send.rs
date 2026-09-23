//! Sending one line of chat as the local player (#1417).
//!
//! The one path both the chat window and the agent client take, so what a
//! line becomes on its way out - its length, its control characters, the
//! sender's own copy of it, the gesture it sets off - cannot differ between a
//! person typing and an agent speaking.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::network::ChatDelivery;
use crate::player::emote::EmoteRequest;
use crate::protocol::OverlandsMessage;
use crate::state::ChatHistory;

/// A line as it will be sent, or `None` when nothing is left to send.
///
/// Trimmed, then held to a strict per-message cap *before* it is broadcast:
/// otherwise a peer could paste an 800 KiB junk string (well under the 1 MiB
/// packet limit) and every guest would word-wrap it in egui on every frame -
/// an instant room-wide DoS. The cap is in CHARACTERS, not bytes (#1264
/// f362): a byte cap gave a CJK writer a third of everyone else's message.
/// The chat window's `char_limit` means this clip cannot fire on anything a
/// person typed or pasted there; it stays as the invariant's enforcement,
/// because what is broadcast must be what the limit says however the line
/// got here - and an agent's line got here some other way.
///
/// Then every control character but tab becomes a space. The receiver runs
/// the same filter defensively, so skipping it here left the sender's own
/// row showing a multi-line paste while every peer saw one line - a
/// permanent visual desync on the sender's HUD.
pub fn outgoing_chat_line(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .take(crate::config::ui::chat::MAX_MESSAGE_CHARS)
            .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
            .collect(),
    )
}

/// Send `text` - already through [`outgoing_chat_line`] - as `sender`.
///
/// Three things, always together:
///
/// * The sender's own copy goes into `chat`, capped and wall-clock stamped
///   (#846), and stamped with `delivery` (#1213): the sender's history used
///   to show a message that reached nobody exactly like one that was
///   delivered.
/// * The keyword gesture plays on the sender's own body (`chassis`) exactly
///   as every peer's copy of it does (#1068). Without it the sender is the
///   one person in the room who never sees their own gesture, which reads as
///   the feature being broken. Same `request_for` as the inbound path, so the
///   two cannot drift.
/// * The line goes to every peer on the reliable channel.
pub fn send_chat_line(
    text: String,
    sender: Option<&AtprotoSession>,
    delivery: ChatDelivery,
    chassis: Option<Entity>,
    chat: &mut ChatHistory,
    emotes: &mut MessageWriter<EmoteRequest>,
    broadcasts: &mut MessageWriter<Broadcast<OverlandsMessage>>,
) {
    let (did, author) = match sender {
        Some(s) => (Some(s.did.clone()), s.handle.clone()),
        None => (None, "me".to_owned()),
    };
    chat.push_sent(did, author, text.clone(), delivery);
    if let Some(request) = chassis.and_then(|c| crate::player::emote::request_for(c, &text)) {
        emotes.write(request);
    }
    broadcasts.write(Broadcast {
        payload: OverlandsMessage::Chat { text },
        channel: ChannelKind::Reliable,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_trimmed_and_flattened_to_one_row() {
        assert_eq!(
            outgoing_chat_line("  hello\nthere\r\n\tfriend  ").as_deref(),
            Some("hello there  \tfriend")
        );
    }

    #[test]
    fn nothing_but_space_is_nothing_to_send() {
        assert_eq!(outgoing_chat_line(" \n\t "), None);
    }

    /// Characters, not bytes: a CJK line gets as many characters as a Latin
    /// one before the cut.
    #[test]
    fn the_cap_counts_characters() {
        let cap = crate::config::ui::chat::MAX_MESSAGE_CHARS;
        let long: String = "語".repeat(cap + 10);
        let sent = outgoing_chat_line(&long).expect("something to send");
        assert_eq!(sent.chars().count(), cap);
    }
}
