//! Application-layer fragmentation for oversized reliable P2P messages (#716).
//!
//! A WebRTC data-channel message cannot exceed 65536 bytes (64 KiB): the
//! `webrtc-sctp` association rejects a larger whole message with
//! `ErrOutboundPacketTooLarge` *before* fragmentation, and neither
//! `matchbox_socket` nor `bevy_symbios_multiuser` raises, negotiates, or
//! chunks around that ceiling. The send is fire-and-forget — the failing
//! `channel.send` result is discarded deep in matchbox — so a full
//! `RoomStateUpdate` for a heavily-authored room silently stops reaching
//! guests with only a bare console `ERROR` line to show for it.
//!
//! This module splits a large reliable [`OverlandsMessage`] into
//! [`OverlandsMessage::ChunkedPayload`] fragments on the send side
//! ([`ChunkSend::broadcast`]) and reassembles them on the receive side
//! ([`ChunkReassembly::ingest`]). Fragments ride the ordered Reliable
//! channel, so they arrive in `seq` order and none is dropped; the receiver
//! buffers by `(sender, msg_id)` until all `total` fragments are in, then
//! decodes the concatenation back into the original message and dispatches it
//! as if it had arrived whole.
//!
//! Two guards keep a hostile or dead peer from exhausting memory: partial
//! reassemblies older than [`config::network::MAX_REASSEMBLY_AGE_SECS`] are
//! evicted, and the total buffered bytes are capped at
//! [`config::network::MAX_REASSEMBLY_BUFFER_BYTES`] (oldest-first eviction).

use std::collections::HashMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::diagnostics::event::EventPayload;
use crate::diagnostics::{MetricsRegistry, SessionLog, samplers};
use crate::pds::record_size::human_bytes;
use crate::protocol::OverlandsMessage;

/// Monotonic per-session counter that stamps each chunked message's `msg_id`.
/// `msg_id` only has to be unique among a sender's in-flight reassemblies, so
/// a plain wrapping counter suffices.
#[derive(Resource, Default)]
pub struct OutboundChunkSeq(pub u64);

/// One in-flight partial reassembly.
struct Partial {
    /// Fragment count declared by every fragment of this message.
    total: u16,
    /// Fragment slots, indexed by `seq`; `None` until that fragment arrives.
    chunks: Vec<Option<Vec<u8>>>,
    /// How many distinct slots are filled (completion is `received == total`).
    received: u16,
    /// Monotonic time the first fragment arrived — the age-eviction key.
    first_seen: f64,
    /// Bytes buffered for this partial — kept so eviction can decrement the
    /// running [`ChunkReassembly::total_bytes`] in O(1). Payload only; the
    /// bookkeeping cost is [`Partial::overhead`].
    bytes: usize,
}

impl Partial {
    /// What this reassembly costs beyond the payload it holds: the slot
    /// vector sized for the whole declared message, plus the struct and its
    /// map entry (#1114).
    fn overhead(&self) -> usize {
        self.total as usize * config::network::REASSEMBLY_SLOT_OVERHEAD_BYTES
            + config::network::REASSEMBLY_PARTIAL_OVERHEAD_BYTES
    }

    /// Total charge against the reassembly budget.
    fn footprint(&self) -> usize {
        self.bytes + self.overhead()
    }
}

/// Receive-side buffer of partial chunked messages, keyed by
/// `(sender, msg_id)`. Inserted as a Bevy resource by the network plugin.
#[derive(Resource, Default)]
pub struct ChunkReassembly {
    partials: HashMap<(PeerId, u64), Partial>,
    /// Running sum of [`Partial::footprint`] across `partials`, for the
    /// buffer cap.
    total_bytes: usize,
    /// When the stale sweep may next run (#1114) — see
    /// [`config::network::REASSEMBLY_SWEEP_INTERVAL_SECS`].
    next_sweep_at: f64,
}

impl ChunkReassembly {
    /// Ingest one fragment. Returns the fully reassembled
    /// [`OverlandsMessage`] on the fragment that completes its message, or
    /// `None` while the message is still incomplete (or the fragment was
    /// rejected as malformed / evicted under memory pressure).
    pub fn ingest(
        &mut self,
        sender: PeerId,
        msg_id: u64,
        seq: u16,
        total: u16,
        data: Vec<u8>,
        now: f64,
    ) -> Option<OverlandsMessage> {
        // On a timer, not per fragment (#1114): the sweep is O(partials),
        // and running it on every one let a flooding peer make the per-frame
        // queue drain quadratic in the number of partials it had opened.
        if now >= self.next_sweep_at {
            self.evict_stale(now);
            self.next_sweep_at = now + config::network::REASSEMBLY_SWEEP_INTERVAL_SECS;
        }

        // Reject nonsense before allocating a buffer: a `total` larger than a
        // ceiling-sized message could ever produce, an out-of-range `seq`, or
        // an oversized single fragment are all corrupt or hostile.
        let max_total = config::network::MAX_RELIABLE_PAYLOAD_BYTES
            .div_ceil(config::network::RELIABLE_CHUNK_DATA_BYTES) as u16;
        if total == 0
            || seq >= total
            || total > max_total
            || data.len() > config::network::RELIABLE_CHUNK_DATA_BYTES
        {
            return None;
        }

        let key = (sender, msg_id);
        // Opening a NEW reassembly is the expensive, attacker-controlled
        // step, so the count bounds are enforced here rather than on every
        // fragment (#1114).
        let opening = !self.partials.contains_key(&key);
        if opening {
            self.make_room_for_new_partial(sender);
        }
        let corrupt;
        {
            let entry = self.partials.entry(key).or_insert_with(|| Partial {
                total,
                chunks: vec![None; total as usize],
                received: 0,
                first_seen: now,
                bytes: 0,
            });
            // A fragment whose `total` disagrees with the one that opened this
            // reassembly is corrupt/spoofed — drop the whole partial.
            if entry.total != total {
                corrupt = true;
            } else {
                corrupt = false;
                if opening {
                    // Charge the bookkeeping the moment the reassembly opens,
                    // so a stream of one-byte fragments with fresh msg_ids
                    // reaches the budget instead of walking past it.
                    self.total_bytes += entry.overhead();
                }
                let slot = &mut entry.chunks[seq as usize];
                if slot.is_none() {
                    let n = data.len();
                    *slot = Some(data);
                    entry.received += 1;
                    entry.bytes += n;
                    self.total_bytes += n;
                }
                // else: a duplicate fragment — ignore it (reliable-ordered
                // delivery should not resend, but be defensive).
            }
        }
        if corrupt {
            self.remove(&key);
            return None;
        }

        self.enforce_budget();

        // Completion — reassemble in `seq` order and decode.
        let done = self
            .partials
            .get(&key)
            .is_some_and(|p| p.received == p.total);
        if !done {
            return None;
        }
        let p = self.partials.remove(&key)?;
        self.total_bytes = self.total_bytes.saturating_sub(p.footprint());
        let mut buf = Vec::with_capacity(p.bytes);
        for slot in p.chunks {
            // `received == total` guarantees every slot is filled; guard
            // rather than unwrap so a logic slip degrades to a dropped
            // message instead of a session-ending panic.
            buf.extend_from_slice(&slot?);
        }
        OverlandsMessage::from_chunk_bytes(&buf)
    }

    /// Remove a partial and decrement the byte accounting.
    fn remove(&mut self, key: &(PeerId, u64)) {
        if let Some(p) = self.partials.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(p.footprint());
        }
    }

    /// Make room for a reassembly `sender` is about to open, dropping the
    /// oldest partial when either count bound is already met (#1114).
    ///
    /// The per-peer bound is evicted first and evicts only that peer's own
    /// partials, so a flooding peer can never displace another peer's
    /// in-flight message — the reason a count bound is per-sender at all.
    fn make_room_for_new_partial(&mut self, sender: PeerId) {
        while self.count_for(sender) >= config::network::MAX_REASSEMBLIES_PER_PEER {
            match self.oldest(|(peer, _)| *peer == sender) {
                Some(key) => self.remove(&key),
                None => break,
            }
        }
        while self.partials.len() >= config::network::MAX_REASSEMBLIES_TOTAL {
            match self.oldest(|_| true) {
                Some(key) => self.remove(&key),
                None => break,
            }
        }
    }

    /// How many reassemblies this peer currently has open.
    fn count_for(&self, sender: PeerId) -> usize {
        self.partials
            .keys()
            .filter(|(peer, _)| *peer == sender)
            .count()
    }

    /// The oldest partial whose key passes `pick`, by first-fragment time.
    fn oldest(&self, pick: impl Fn(&(PeerId, u64)) -> bool) -> Option<(PeerId, u64)> {
        self.partials
            .iter()
            .filter(|(key, _)| pick(key))
            .min_by(|(_, a), (_, b)| {
                a.first_seen
                    .partial_cmp(&b.first_seen)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(key, _)| *key)
    }

    /// Drop partials whose first fragment arrived more than
    /// [`config::network::MAX_REASSEMBLY_AGE_SECS`] ago — a sender that
    /// vanished mid-message.
    fn evict_stale(&mut self, now: f64) {
        let cutoff = config::network::MAX_REASSEMBLY_AGE_SECS;
        let stale: Vec<(PeerId, u64)> = self
            .partials
            .iter()
            .filter(|(_, p)| now - p.first_seen > cutoff)
            .map(|(k, _)| *k)
            .collect();
        for k in stale {
            self.remove(&k);
        }
    }

    /// Evict oldest-first until the buffer is back under
    /// [`config::network::MAX_REASSEMBLY_BUFFER_BYTES`].
    fn enforce_budget(&mut self) {
        while self.total_bytes > config::network::MAX_REASSEMBLY_BUFFER_BYTES {
            match self.oldest(|_| true) {
                Some(k) => self.remove(&k),
                None => break,
            }
        }
    }
}

/// Bundles the outbound chunk counter and the metrics registry so any system
/// can chunk-broadcast a reliable message through one [`SystemParam`]. Beyond
/// tidiness, this keeps param-heavy senders (the drag-drop gift handler, the
/// room broadcaster) under Bevy's 16-parameter-per-system ceiling.
#[derive(SystemParam)]
pub struct ChunkSend<'w> {
    seq: ResMut<'w, OutboundChunkSeq>,
    metrics: ResMut<'w, MetricsRegistry>,
}

impl ChunkSend<'_> {
    /// Chunk-send `msg` to **every** peer over the Reliable channel. See
    /// [`send_chunked`] for the size policy.
    pub(crate) fn broadcast(
        &mut self,
        sender: &mut SendMessage<OverlandsMessage>,
        session_log: &mut SessionLog,
        now: f64,
        msg: OverlandsMessage,
    ) -> SendOutcome {
        send_chunked(
            sender,
            &mut self.seq,
            &mut self.metrics,
            session_log,
            ChunkDest::Broadcast,
            now,
            msg,
        )
    }
}

/// What actually happened to a [`send_chunked`] call (#1123).
///
/// The refusal branch used to `return ()`, so every caller was written as
/// though the send had happened: the room broadcaster kept clearing its
/// dirty flag, and the gift handler kept registering a pending offer for a
/// message no peer would ever see. The console learned; nobody else did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendOutcome {
    /// Handed to the transport, whole or as fragments.
    Sent,
    /// Past [`config::network::MAX_RELIABLE_PAYLOAD_BYTES`], so nothing was
    /// handed to the transport at all. Carries the measured size because
    /// "too big" is not actionable and "1.1 MiB against a 900 KiB limit" is.
    Refused { bytes: usize },
    /// The message could not be serialized, so there was nothing to send.
    /// A bug rather than a user condition, but it is still not `Sent`.
    NotSerialized,
}

impl SendOutcome {
    pub(crate) fn is_sent(self) -> bool {
        matches!(self, SendOutcome::Sent)
    }
}

/// Remembers which message kinds the owner has already been warned about,
/// so crossing the wire ceiling reports once per crossing rather than once
/// per send (#1123).
///
/// Latched on the KIND, not on a timer: `broadcast_room_state` fires every
/// [`config::network::ROOM_BROADCAST_MIN_INTERVAL_SECS`] while the record is
/// dirty, and a toast every interval would bury the queue it shares with
/// every other notification. Cleared on the next successful send of the same
/// kind, so an owner who trims the world back under the ceiling and later
/// crosses it again is told again — the news is the crossing.
#[derive(Resource, Default)]
pub struct OversizeNotices(std::collections::HashSet<&'static str>);

/// Report a refused send to the person who caused it, at most once per
/// crossing, and pass the outcome through.
///
/// `subject` is both the latch key and the noun in the sentence, so the
/// avatar and the world each get their own latch and their own wording.
pub(crate) fn warn_once_on_refusal(
    outcome: SendOutcome,
    notices: &mut OversizeNotices,
    toasts: &mut crate::ui::toast::Toasts,
    subject: &'static str,
    now: f64,
) -> SendOutcome {
    match outcome {
        SendOutcome::Refused { bytes } => {
            if notices.0.insert(subject) {
                toasts.warn(
                    format!(
                        "Live sync paused — your {subject} is {}, over the {} peer-sync \
                         limit. Guests see your last saved version until you trim it.",
                        human_bytes(bytes),
                        human_bytes(config::network::MAX_RELIABLE_PAYLOAD_BYTES),
                    ),
                    now,
                );
            }
        }
        SendOutcome::Sent => {
            notices.0.remove(subject);
        }
        SendOutcome::NotSerialized => {}
    }
    outcome
}

/// How many bytes `msg` would put on the wire — the exact quantity
/// [`send_chunked`] measures against
/// [`config::network::MAX_RELIABLE_PAYLOAD_BYTES`].
///
/// Exposed so a readout can show the owner the number the refusal is
/// actually decided on (#1123). The World Editor's existing gauge measures
/// the largest single PDS record, which is a different quantity against a
/// different limit — and a green record gauge beside a dead live sync is
/// exactly what made the drop unreadable.
///
/// `None` when the message will not serialize, which is the same condition
/// [`SendOutcome::NotSerialized`] reports.
pub fn wire_payload_bytes(msg: &OverlandsMessage) -> Option<usize> {
    msg.to_chunk_bytes().ok().map(|b| b.len())
}

/// Where a chunked reliable send is addressed — every peer, or one peer.
pub(crate) enum ChunkDest {
    Broadcast,
    To(PeerId),
}

fn emit_reliable(
    sender: &mut SendMessage<OverlandsMessage>,
    dest: &ChunkDest,
    msg: OverlandsMessage,
) {
    match dest {
        ChunkDest::Broadcast => sender.broadcast(msg, ChannelKind::Reliable),
        ChunkDest::To(peer) => sender.to(*peer, msg, ChannelKind::Reliable),
    }
}

/// Reliable chunked send, addressed by `dest`, splitting `msg` into
/// sub-ceiling [`OverlandsMessage::ChunkedPayload`] fragments when it is too
/// large to ride one WebRTC message.
///
/// * Under [`config::network::RELIABLE_CHUNK_DATA_BYTES`] → sent whole (no
///   fragmentation overhead for the common case).
/// * Over [`config::network::MAX_RELIABLE_PAYLOAD_BYTES`] → refused, counted
///   ([`samplers::broadcast_oversize_dropped`]) and logged as an
///   [`EventPayload::OutboundMessageOversize`] error rather than handed to a
///   send that would silently fail. The recipient does not receive it — and
///   the caller is told so ([`SendOutcome::Refused`]) rather than left to
///   assume it landed (#1123).
/// * In between → split into `ceil(len / chunk)` fragments, all sharing a
///   fresh `msg_id`.
///
/// A free function (rather than only a [`ChunkSend`] method) so a system that
/// already holds its own `seq`/`metrics`/`session_log` — the peer-connect
/// handler, whose param budget cannot also fit the `ChunkSend` bundle — can
/// reuse the identical path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn send_chunked(
    sender: &mut SendMessage<OverlandsMessage>,
    seq: &mut OutboundChunkSeq,
    metrics: &mut MetricsRegistry,
    session_log: &mut SessionLog,
    dest: ChunkDest,
    now: f64,
    msg: OverlandsMessage,
) -> SendOutcome {
    let bytes = match msg.to_chunk_bytes() {
        Ok(b) => b,
        Err(e) => {
            error!(
                "Failed to serialize {} for chunked send: {e}",
                variant_label(&msg)
            );
            return SendOutcome::NotSerialized;
        }
    };
    let len = bytes.len();
    samplers::broadcast_payload_bytes(metrics, len);

    if len > config::network::MAX_RELIABLE_PAYLOAD_BYTES {
        samplers::broadcast_oversize_dropped(metrics);
        session_log.error(
            now,
            EventPayload::OutboundMessageOversize {
                message_kind: variant_label(&msg).to_string(),
                bytes: len as u64,
                ceiling_bytes: config::network::MAX_RELIABLE_PAYLOAD_BYTES as u64,
            },
        );
        error!(
            "Refusing to send {} — {} exceeds the {} reliable-payload ceiling; \
             the recipient will not receive it. Reduce the amount of authored content.",
            variant_label(&msg),
            human_bytes(len),
            human_bytes(config::network::MAX_RELIABLE_PAYLOAD_BYTES),
        );
        return SendOutcome::Refused { bytes: len };
    }

    if len <= config::network::RELIABLE_CHUNK_DATA_BYTES {
        emit_reliable(sender, &dest, msg);
        return SendOutcome::Sent;
    }

    let chunk_size = config::network::RELIABLE_CHUNK_DATA_BYTES;
    let total = len.div_ceil(chunk_size) as u16;
    let msg_id = seq.0;
    seq.0 = seq.0.wrapping_add(1);
    for (i, chunk) in bytes.chunks(chunk_size).enumerate() {
        emit_reliable(
            sender,
            &dest,
            OverlandsMessage::ChunkedPayload {
                msg_id,
                seq: i as u16,
                total,
                data: chunk.to_vec(),
            },
        );
    }
    SendOutcome::Sent
}

/// Stable human label for a message variant, for logs and the oversize event.
fn variant_label(msg: &OverlandsMessage) -> &'static str {
    match msg {
        OverlandsMessage::Transform { .. } => "Transform",
        OverlandsMessage::Identity { .. } => "Identity",
        OverlandsMessage::Chat { .. } => "Chat",
        OverlandsMessage::RoomStateUpdate { .. } => "RoomStateUpdate",
        OverlandsMessage::AvatarStateUpdate { .. } => "AvatarStateUpdate",
        OverlandsMessage::ItemOffer { .. } => "ItemOffer",
        OverlandsMessage::ItemOfferResponse { .. } => "ItemOfferResponse",
        OverlandsMessage::ChunkedPayload { .. } => "ChunkedPayload",
        OverlandsMessage::AvatarRecordsPublished => "AvatarRecordsPublished",
        OverlandsMessage::Hello { .. } => "Hello",
        OverlandsMessage::WorldDigest { .. } => "WorldDigest",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mint a deterministic, distinct `PeerId` without naming the `uuid`
    /// crate: `PeerId` derives `Deserialize` and wraps a `Uuid`, which
    /// deserializes from its hyphenated string form.
    fn test_peer(n: u8) -> PeerId {
        let s = format!("00000000-0000-0000-0000-0000000000{n:02x}");
        serde_json::from_value(serde_json::Value::String(s)).expect("valid uuid")
    }

    /// A `RoomStateUpdate` whose bincode serialization exceeds the per-chunk
    /// budget, so it must be split. bincode encodes the `Vec<u8>` compactly
    /// (~1×), so 120 KiB of payload lands comfortably over
    /// `RELIABLE_CHUNK_DATA_BYTES` (48 KiB) yet well under the refuse ceiling.
    fn big_message(fill: u8) -> OverlandsMessage {
        OverlandsMessage::RoomStateUpdate {
            record_json: vec![fill; 120 * 1024],
        }
    }

    /// Split a message exactly as [`ChunkSend::broadcast`] does, so a round-trip
    /// test can drive [`ChunkReassembly::ingest`] with real fragments. Yields
    /// `(seq, total, data)` triples.
    fn fragments(msg: &OverlandsMessage) -> Vec<(u16, u16, Vec<u8>)> {
        let bytes = msg.to_chunk_bytes().unwrap();
        let chunk_size = config::network::RELIABLE_CHUNK_DATA_BYTES;
        assert!(bytes.len() > chunk_size, "test message must actually split");
        let total = bytes.len().div_ceil(chunk_size) as u16;
        bytes
            .chunks(chunk_size)
            .enumerate()
            .map(|(i, c)| (i as u16, total, c.to_vec()))
            .collect()
    }

    /// Drive `send_chunked` through a real `SendMessage`, returning both
    /// the outcome and what actually reached the transport.
    ///
    /// `SendMessage` is two `MessageWriter`s over `Broadcast`/`SendTo`, so a
    /// bare `App` with those two message types registered is enough to
    /// observe the send for real — no relay, no socket. Counting the emitted
    /// messages is what separates "refused" from "sent and lost".
    fn drive_send(msg: OverlandsMessage) -> (SendOutcome, usize) {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_message::<Broadcast<OverlandsMessage>>()
            .add_message::<SendTo<OverlandsMessage>>()
            .init_resource::<OutboundChunkSeq>()
            .init_resource::<MetricsRegistry>()
            .init_resource::<SessionLog>();
        let outcome = app
            .world_mut()
            .run_system_once(
                move |mut sender: SendMessage<OverlandsMessage>,
                      mut seq: ResMut<OutboundChunkSeq>,
                      mut metrics: ResMut<MetricsRegistry>,
                      mut log: ResMut<SessionLog>| {
                    send_chunked(
                        &mut sender,
                        &mut seq,
                        &mut metrics,
                        &mut log,
                        ChunkDest::Broadcast,
                        0.0,
                        msg.clone(),
                    )
                },
            )
            .expect("system runs");
        let emitted = app
            .world()
            .resource::<Messages<Broadcast<OverlandsMessage>>>()
            .iter_current_update_messages()
            .count();
        (outcome, emitted)
    }

    /// #1123 — the refusal has to be a fact the caller receives, not a line
    /// in a console the owner does not have.
    ///
    /// Before the fix `send_chunked` returned `()` on every path, so the
    /// room broadcaster cleared its dirty flag and the gift handler armed an
    /// offer timer for a message that never left the machine.
    #[test]
    fn an_oversize_payload_is_refused_with_its_size_and_nothing_is_sent() {
        let over = OverlandsMessage::RoomStateUpdate {
            record_json: vec![7u8; config::network::MAX_RELIABLE_PAYLOAD_BYTES + 1],
        };
        let (outcome, emitted) = drive_send(over);
        match outcome {
            SendOutcome::Refused { bytes } => assert!(
                bytes > config::network::MAX_RELIABLE_PAYLOAD_BYTES,
                "the refusal carries the measured size, not just a flag"
            ),
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_eq!(emitted, 0, "nothing reached the transport");
    }

    /// The other two paths report `Sent`, and a chunked send emits every
    /// fragment — so a caller keying bookkeeping on `is_sent` is not
    /// throwing away large-but-legal messages.
    #[test]
    fn sends_under_the_ceiling_report_sent() {
        let (small, emitted) = drive_send(OverlandsMessage::RoomStateUpdate {
            record_json: vec![1u8; 16],
        });
        assert_eq!(small, SendOutcome::Sent);
        assert_eq!(emitted, 1, "one whole message, unfragmented");

        let msg = big_message(3);
        let expected = msg.to_chunk_bytes().unwrap().len();
        let expected = expected.div_ceil(config::network::RELIABLE_CHUNK_DATA_BYTES);
        let (chunked, emitted) = drive_send(msg);
        assert_eq!(chunked, SendOutcome::Sent);
        assert_eq!(emitted, expected, "every fragment reached the transport");
    }

    /// The owner is told once per crossing, not once per send — and told
    /// again if they cross back over later.
    ///
    /// `broadcast_room_state` fires every `ROOM_BROADCAST_MIN_INTERVAL_SECS`
    /// while the record is dirty, so an unlatched toast would evict every
    /// other notification from a queue capped at a handful of entries.
    #[test]
    fn the_owner_is_warned_once_per_crossing() {
        let mut notices = OversizeNotices::default();
        let mut toasts = crate::ui::toast::Toasts::default();
        let refused = SendOutcome::Refused { bytes: 1_000_000 };

        warn_once_on_refusal(refused, &mut notices, &mut toasts, "world", 0.0);
        warn_once_on_refusal(refused, &mut notices, &mut toasts, "world", 0.1);
        warn_once_on_refusal(refused, &mut notices, &mut toasts, "world", 0.2);
        let shown = toasts.shown();
        assert_eq!(shown.len(), 1, "three refusals, one toast");
        assert_eq!(shown[0].0, crate::ui::toast::ToastKind::Warn);
        assert!(
            shown[0].1.contains("976.6 KiB") && shown[0].1.contains("900.0 KiB"),
            "the toast names the measured size and the limit: {}",
            shown[0].1
        );
        assert!(
            shown[0].1.contains("world"),
            "and which of the owner's records is stuck: {}",
            shown[0].1
        );

        // A different subject latches independently — the avatar and the
        // world cross the ceiling for different reasons.
        warn_once_on_refusal(refused, &mut notices, &mut toasts, "avatar", 0.3);
        assert_eq!(toasts.shown().len(), 2);

        // Trimming back under the ceiling re-arms the warning: the news is
        // the crossing, and an owner who crosses twice should hear twice.
        warn_once_on_refusal(SendOutcome::Sent, &mut notices, &mut toasts, "world", 1.0);
        warn_once_on_refusal(refused, &mut notices, &mut toasts, "world", 2.0);
        assert_eq!(toasts.shown().len(), 3);
    }

    /// A serialize failure is not a send. It is a bug rather than a user
    /// condition, so it carries no toast — but a caller must not treat it
    /// as delivery either.
    #[test]
    fn a_serialize_failure_is_not_reported_as_sent() {
        let mut notices = OversizeNotices::default();
        let mut toasts = crate::ui::toast::Toasts::default();
        assert!(!SendOutcome::NotSerialized.is_sent());
        warn_once_on_refusal(
            SendOutcome::NotSerialized,
            &mut notices,
            &mut toasts,
            "world",
            0.0,
        );
        assert!(toasts.shown().is_empty());
    }

    #[test]
    fn chunk_encoding_is_compact_not_bloated() {
        // Regression guard for the codec choice: `serde_json` encodes the
        // `record_json` Vec<u8> as a number array (~3.5×), which would
        // over-fragment every message and inflate the measured size past the
        // real transmitted size and the payload ceiling. bincode keeps it ~1×.
        let payload = 40 * 1024;
        let msg = OverlandsMessage::RoomStateUpdate {
            record_json: vec![0u8; payload],
        };
        let len = msg.to_chunk_bytes().unwrap().len();
        assert!(
            len < payload + 64,
            "chunk encoding must be compact (bincode), got {len} B for a {payload} B payload"
        );
        // A 40 KiB payload therefore stays under the 48 KiB direct-send
        // threshold and would ride one message unchunked.
        assert!(len <= config::network::RELIABLE_CHUNK_DATA_BYTES);
    }

    #[test]
    fn reassembles_a_split_message_in_order() {
        let mut r = ChunkReassembly::default();
        let peer = test_peer(1);
        let original = big_message(7);
        let frags = fragments(&original);
        let n = frags.len();

        let mut out = None;
        for (idx, (seq, total, data)) in frags.into_iter().enumerate() {
            let res = r.ingest(peer, 42, seq, total, data, 0.0);
            if idx + 1 < n {
                assert!(res.is_none(), "must not complete before the last fragment");
            } else {
                out = res;
            }
        }

        match out {
            Some(OverlandsMessage::RoomStateUpdate { record_json }) => {
                assert_eq!(record_json, vec![7u8; 120 * 1024]);
            }
            other => panic!("expected reassembled RoomStateUpdate, got {other:?}"),
        }
        // Buffer fully drained after completion.
        assert!(r.partials.is_empty());
        assert_eq!(r.total_bytes, 0);
    }

    #[test]
    fn duplicate_fragment_does_not_double_count_or_complete_early() {
        let mut r = ChunkReassembly::default();
        let peer = test_peer(2);
        let frags = fragments(&big_message(3));
        // Deliver fragment 0 twice — the second must be a no-op.
        let (s0, t0, d0) = frags[0].clone();
        assert!(r.ingest(peer, 1, s0, t0, d0.clone(), 0.0).is_none());
        let bytes_after_one = r.total_bytes;
        assert!(r.ingest(peer, 1, s0, t0, d0, 0.0).is_none());
        assert_eq!(
            r.total_bytes, bytes_after_one,
            "duplicate must not add bytes"
        );
        assert_eq!(r.partials.len(), 1);
    }

    #[test]
    fn stale_partials_are_evicted_by_age() {
        let mut r = ChunkReassembly::default();
        let peer = test_peer(3);
        let frags = fragments(&big_message(9));
        let (s0, t0, d0) = frags[0].clone();
        // First fragment at t=0, never completed.
        assert!(r.ingest(peer, 5, s0, t0, d0, 0.0).is_none());
        assert_eq!(r.partials.len(), 1);
        // A later ingest well past the age cutoff sweeps the abandoned partial.
        let (s1, t1, d1) = frags[1].clone();
        let late = config::network::MAX_REASSEMBLY_AGE_SECS + 1.0;
        r.ingest(peer, 5, s1, t1, d1, late);
        // The stale msg_id 5 partial from t=0 is gone; only the fresh fragment
        // (a new partial started at `late`) remains.
        assert!(
            r.partials.keys().all(|(_, id)| *id == 5),
            "only the just-restarted partial should remain"
        );
        assert_eq!(r.partials.len(), 1);
    }

    #[test]
    fn rejects_out_of_range_or_corrupt_fragments() {
        let mut r = ChunkReassembly::default();
        let peer = test_peer(4);
        // seq >= total.
        assert!(r.ingest(peer, 1, 3, 3, vec![0; 16], 0.0).is_none());
        // total == 0.
        assert!(r.ingest(peer, 1, 0, 0, vec![0; 16], 0.0).is_none());
        // Oversized single fragment.
        let too_big = config::network::RELIABLE_CHUNK_DATA_BYTES + 1;
        assert!(r.ingest(peer, 1, 0, 2, vec![0; too_big], 0.0).is_none());
        assert!(
            r.partials.is_empty(),
            "no partial created for bad fragments"
        );
    }

    #[test]
    fn mismatched_total_drops_the_partial() {
        let mut r = ChunkReassembly::default();
        let peer = test_peer(5);
        // Open a reassembly declaring total=3.
        assert!(r.ingest(peer, 1, 0, 3, vec![1; 16], 0.0).is_none());
        assert_eq!(r.partials.len(), 1);
        // A fragment for the same msg_id with a different total is corrupt.
        assert!(r.ingest(peer, 1, 1, 4, vec![1; 16], 0.0).is_none());
        assert!(r.partials.is_empty());
        assert_eq!(r.total_bytes, 0);
    }

    // -----------------------------------------------------------------
    // #1114: the flood a byte budget alone did not stop.
    // -----------------------------------------------------------------

    /// One-byte fragment of a message that declares many more, with a fresh
    /// `msg_id` each time — the cheapest way to make a guest allocate.
    fn sliver(state: &mut ChunkReassembly, peer: PeerId, msg_id: u64, now: f64) {
        state.ingest(peer, msg_id, 0, 19, vec![0u8], now);
    }

    #[test]
    fn a_flood_of_one_byte_fragments_cannot_open_unbounded_reassemblies() {
        // Before: the budget counted payload bytes only, so 100k slivers
        // charged 100 KB against a 4 MiB cap while actually holding a slot
        // vector, a Partial and a map entry apiece — the cap was
        // unreachable and the partial count grew until the ten-second age
        // sweep happened to catch it.
        let mut state = ChunkReassembly::default();
        let attacker = test_peer(1);
        for msg_id in 0..100_000 {
            sliver(&mut state, attacker, msg_id, 0.0);
        }
        assert!(
            state.partials.len() <= config::network::MAX_REASSEMBLIES_PER_PEER,
            "one peer held {} reassemblies open",
            state.partials.len()
        );
    }

    #[test]
    fn a_flooding_peer_only_ever_evicts_its_own_partials() {
        // The count bound is per-sender precisely so this holds: a legitimate
        // peer's half-delivered room push must survive somebody else's flood.
        let mut state = ChunkReassembly::default();
        let honest = test_peer(2);
        let attacker = test_peer(3);

        let frags = fragments(&big_message(7));
        // Deliver all but the last fragment of a real message.
        for (seq, total, data) in frags.iter().take(frags.len() - 1) {
            assert!(
                state
                    .ingest(honest, 1, *seq, *total, data.clone(), 0.0)
                    .is_none()
            );
        }

        for msg_id in 0..10_000 {
            sliver(&mut state, attacker, msg_id, 0.0);
        }

        let (seq, total, data) = frags.last().expect("multi-fragment").clone();
        match state.ingest(honest, 1, seq, total, data, 0.0) {
            Some(OverlandsMessage::RoomStateUpdate { record_json }) => {
                assert_eq!(
                    record_json,
                    vec![7u8; 120 * 1024],
                    "the honest peer's message still completes through the flood"
                );
            }
            other => panic!("honest message lost to another peer's flood: {other:?}"),
        }
    }

    #[test]
    fn the_budget_charges_the_bookkeeping_not_just_the_payload() {
        let mut state = ChunkReassembly::default();
        let peer = test_peer(4);
        sliver(&mut state, peer, 1, 0.0);

        // One payload byte, but a 19-slot vector plus the struct and entry.
        assert!(
            state.total_bytes > 1 + config::network::REASSEMBLY_PARTIAL_OVERHEAD_BYTES,
            "expected the slot vector to be charged, got {}",
            state.total_bytes
        );
    }

    #[test]
    fn the_accounting_returns_to_zero_when_a_message_completes() {
        // The overhead is charged once at open and released once at
        // completion; a leak here would ratchet the cap shut over a session.
        let mut state = ChunkReassembly::default();
        let peer = test_peer(5);
        let msg = big_message(9);
        for (seq, total, data) in fragments(&msg) {
            state.ingest(peer, 42, seq, total, data, 0.0);
        }
        assert_eq!(state.total_bytes, 0);
        assert!(state.partials.is_empty());
    }

    #[test]
    fn the_stale_sweep_runs_on_a_timer_not_on_every_fragment() {
        // The sweep is O(partials); running it per fragment made the drain
        // quadratic under flood. It must still happen — just not every time.
        let mut state = ChunkReassembly::default();
        let peer = test_peer(6);
        sliver(&mut state, peer, 1, 0.0);
        assert_eq!(state.partials.len(), 1);

        let stale_at = config::network::MAX_REASSEMBLY_AGE_SECS
            + config::network::REASSEMBLY_SWEEP_INTERVAL_SECS
            + 1.0;
        sliver(&mut state, peer, 2, stale_at);
        assert!(
            !state.partials.contains_key(&(peer, 1)),
            "the aged-out partial is still collected, on the next sweep"
        );
    }
}
