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
    /// running [`ChunkReassembly::total_bytes`] in O(1).
    bytes: usize,
}

/// Receive-side buffer of partial chunked messages, keyed by
/// `(sender, msg_id)`. Inserted as a Bevy resource by the network plugin.
#[derive(Resource, Default)]
pub struct ChunkReassembly {
    partials: HashMap<(PeerId, u64), Partial>,
    /// Running sum of `Partial::bytes` across `partials`, for the buffer cap.
    total_bytes: usize,
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
        self.evict_stale(now);

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
        self.total_bytes -= p.bytes;
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
            self.total_bytes -= p.bytes;
        }
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
            let oldest = self
                .partials
                .iter()
                .min_by(|(_, a), (_, b)| {
                    a.first_seen
                        .partial_cmp(&b.first_seen)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(k, _)| *k);
            match oldest {
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
    /// Broadcast `msg` over the Reliable channel, splitting it into
    /// sub-ceiling [`OverlandsMessage::ChunkedPayload`] fragments if its
    /// serialized form is too large to ride one WebRTC message.
    ///
    /// * Under [`config::network::RELIABLE_CHUNK_DATA_BYTES`] → sent whole (no
    ///   fragmentation overhead for the common case).
    /// * Over [`config::network::MAX_RELIABLE_PAYLOAD_BYTES`] → refused,
    ///   counted ([`samplers::broadcast_oversize_dropped`]) and logged as an
    ///   [`EventPayload::OutboundMessageOversize`] error rather than handed to
    ///   a send that would silently fail. The recipient does not receive it —
    ///   but the drop is now visible instead of an unobservable SCTP error.
    /// * In between → split into `ceil(len / chunk)` fragments, all sharing a
    ///   fresh `msg_id`.
    pub(crate) fn broadcast(
        &mut self,
        sender: &mut SendMessage<OverlandsMessage>,
        session_log: &mut SessionLog,
        now: f64,
        msg: OverlandsMessage,
    ) {
        let bytes = match msg.to_chunk_bytes() {
            Ok(b) => b,
            Err(e) => {
                error!(
                    "Failed to serialize {} for chunked broadcast: {e}",
                    variant_label(&msg)
                );
                return;
            }
        };
        let len = bytes.len();
        samplers::broadcast_payload_bytes(&mut self.metrics, len);

        if len > config::network::MAX_RELIABLE_PAYLOAD_BYTES {
            samplers::broadcast_oversize_dropped(&mut self.metrics);
            session_log.error(
                now,
                EventPayload::OutboundMessageOversize {
                    message_kind: variant_label(&msg).to_string(),
                    bytes: len as u64,
                    ceiling_bytes: config::network::MAX_RELIABLE_PAYLOAD_BYTES as u64,
                },
            );
            error!(
                "Refusing to broadcast {} — {} exceeds the {} reliable-payload ceiling; \
                 the recipient will not receive it. Reduce the amount of authored content.",
                variant_label(&msg),
                human_bytes(len),
                human_bytes(config::network::MAX_RELIABLE_PAYLOAD_BYTES),
            );
            return;
        }

        if len <= config::network::RELIABLE_CHUNK_DATA_BYTES {
            sender.broadcast(msg, ChannelKind::Reliable);
            return;
        }

        let chunk_size = config::network::RELIABLE_CHUNK_DATA_BYTES;
        let total = len.div_ceil(chunk_size) as u16;
        let msg_id = self.seq.0;
        self.seq.0 = self.seq.0.wrapping_add(1);
        for (i, chunk) in bytes.chunks(chunk_size).enumerate() {
            sender.broadcast(
                OverlandsMessage::ChunkedPayload {
                    msg_id,
                    seq: i as u16,
                    total,
                    data: chunk.to_vec(),
                },
                ChannelKind::Reliable,
            );
        }
    }
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
}
