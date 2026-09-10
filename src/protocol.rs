//! Wire protocol for `OverlandsMessage`. The message enum is Serde-tagged
//! and rides the `bevy_symbios_multiuser` data channels; each variant's
//! docstring records which channel it is expected to travel over.
//!
//! Avatar records are **not** broadcast inline — Identity carries just the
//! peer's DID/handle, and the receiver fetches the signed `AvatarRecord`
//! from the owner's PDS directly. The lightweight `AvatarStateUpdate`
//! variant nudges peers to re-fetch after a live edit. `RoomStateUpdate`
//! uses the same preview-then-publish pattern for the owner's room recipe
//! so guests mirror mid-slider tweaks before the author presses "Save to
//! PDS".
//!
//! That preview is whole for a **generator** body, whose payload is the
//! record, and partial for a **rigged** one, whose payload lives in
//! separate wardrobe and attachment records that `AvatarStateUpdate` can
//! only name (#1122). A peer resolves those names against the owner's PDS,
//! so what it renders is the owner's last SAVED body — and
//! `AvatarRecordsPublished` is how it learns that the bytes behind those
//! names have moved.
//!
//! The wire is versioned only from #1121 on: [`OverlandsMessage::Hello`]
//! announces [`PROTOCOL_VERSION`] alongside Identity, and the byte layout of
//! every variant is pinned by a test in this file. Neither makes two
//! disagreeing builds compatible — nothing can, once a layout has moved — but
//! together they turn a layout change from a silently dropped packet into a
//! failing test before release and a labelled peer after it.
//!
//! Peer-to-peer inventory gifts travel as an [`OverlandsMessage::ItemOffer`]
//! / [`OverlandsMessage::ItemOfferResponse`] pair: both are broadcast over
//! the Reliable channel and addressed by the recipient DID inside the
//! payload, because `bevy_symbios_multiuser` has no directed-send primitive
//! — non-targets authenticate the DID and drop the message on receipt.

use serde::{Deserialize, Serialize};

use crate::pds::{AvatarRecord, Generator, RoomRecord};

/// All messages exchanged over the P2P network.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OverlandsMessage {
    /// Physics transform broadcast at ~64 Hz over the Unreliable channel.
    Transform {
        position: [f32; 3],
        rotation: [f32; 4],
    },
    /// Reliable identity announcement sent on join and periodically
    /// thereafter. Carries only the peer's DID/handle — the avatar record
    /// itself is pulled directly from the DID's PDS, so bad actors cannot
    /// spoof another user's vessel by broadcasting a forged payload.
    Identity { did: String, handle: String },
    /// Chat message sent over the Reliable channel.
    Chat { text: String },
    /// Room owner broadcast their updated environment settings over Reliable.
    ///
    /// The payload is a JSON-serialised [`RoomRecord`] rather than the
    /// record itself, because `RoomRecord` contains internally-tagged enums
    /// (`#[serde(tag = "$type")]` on `Generator`, `Placement`, and
    /// `ScatterBounds`) that require `serde::Deserializer::deserialize_any`
    /// — and bincode, which `bevy_symbios_multiuser` uses for its data
    /// channels, explicitly does not support that method. Guests would
    /// otherwise see "Bincode does not support the
    /// serde::Deserializer::deserialize_any method" every time the owner
    /// edited a room setting, and never receive the update. JSON has no
    /// such limitation, so we pay one allocation to wrap the record in a
    /// byte buffer that bincode can shuttle verbatim.
    RoomStateUpdate { record_json: Vec<u8> },
    /// Hot update for the sender's own avatar. The payload is a
    /// JSON-serialised [`AvatarRecord`] — same rationale as
    /// `RoomStateUpdate` (bincode cannot handle the `#[serde(tag = "$type")]`
    /// open union on `AvatarBody`). Sent over the Reliable channel as a
    /// live preview of the peer's editor state, so other players see edits
    /// immediately without waiting for a Publish round-trip.
    AvatarStateUpdate { record_json: Vec<u8> },
    /// Peer-to-peer inventory gift. The sender drags a generator from their
    /// Inventory (or World Editor Generators tab) onto a peer row in the
    /// People window; the engine broadcasts this message and only the peer
    /// whose authenticated DID matches `target_did` acts on it.
    ///
    /// Broadcast-with-address is used because
    /// [`bevy_symbios_multiuser::messages::Broadcast`] has no directed-send primitive —
    /// non-targets drop the message on receipt after the DID check. The
    /// `generator_json` payload is a JSON-serialised [`Generator`] for the
    /// same reason [`Self::RoomStateUpdate`] ships JSON-in-bincode:
    /// `Generator` is a `#[serde(tag = "$type")]` open union that bincode's
    /// streaming decoder cannot handle.
    ///
    /// `offer_id` is a sender-chosen token echoed by the recipient in
    /// [`Self::ItemOfferResponse`] so the sender can correlate accept/decline
    /// outcomes with the originating drag. It only has to be unique within
    /// one sender's session.
    ///
    /// # Envelope and payload (#1184)
    ///
    /// Everything the item *is* — its name, its blueprint, its wear
    /// metadata — lives in `payload_json` as one JSON
    /// [`ItemOfferPayload`]. This is the variant that already broke once:
    /// 59ff989 appended `wear_json` beside four bincode fields with no
    /// version bump, and every gift across that boundary has failed
    /// silently since, because bincode identifies fields by position and
    /// a peer on the other side decodes an error it can only drop. Inside
    /// a JSON payload a new field is additive — serde skips what it does
    /// not know and defaults what is missing — so the variant leaves the
    /// layout-break class entirely. Same shape [`Self::RoomStateUpdate`]
    /// and [`Self::AvatarStateUpdate`] already use.
    ///
    /// `offer_id` and `target_did` stay OUTSIDE the payload on purpose.
    /// They are the envelope, not the content: this is a broadcast with an
    /// address (`bevy_symbios_multiuser` has no directed-send primitive),
    /// so every peer in the room receives every gift and all but one of
    /// them drop it. Reading the address without parsing a blueprint that
    /// peer is about to discard is worth an envelope, and the two
    /// auto-decline paths — muted sender, busy recipient — answer with
    /// `offer_id` before any decode as well. Neither field is one that
    /// grows; the item is.
    ///
    /// `offer_id` is a sender-chosen token echoed by the recipient in
    /// [`Self::ItemOfferResponse`] so the sender can correlate
    /// accept/decline outcomes with the originating drag. It only has to
    /// be unique within one sender's session.
    ItemOffer {
        offer_id: u64,
        target_did: String,
        payload_json: Vec<u8>,
    },
    /// Reply to an [`Self::ItemOffer`]. The `target_did` is the *sender* of
    /// the original offer so non-originators can drop the response on
    /// receipt.
    ///
    /// Same envelope/payload split as [`Self::ItemOffer`] and for the same
    /// reason (#1184): the address is read by every peer, the answer by
    /// one. [`ItemOfferResponsePayload`] currently carries only
    /// `accepted` — `true` means the recipient added the item to their
    /// inventory, `false` covers decline / mute / busy / full /
    /// over-capacity — and a decline *reason* is the obvious next field,
    /// which is exactly the addition this shape makes free.
    ItemOfferResponse {
        offer_id: u64,
        target_did: String,
        payload_json: Vec<u8>,
    },
    /// One fragment of a larger reliable message that exceeded the 64 KiB
    /// WebRTC data-channel ceiling and was split by the network chunk layer
    /// (#716). The `data` bytes are a slice of the bincode serialization of
    /// the *original* [`OverlandsMessage`] (see [`Self::to_chunk_bytes`]); the
    /// receiver buffers fragments by `(sender, msg_id)` and, once all `total`
    /// are in, concatenates them in `seq` order and decodes the result back
    /// into an `OverlandsMessage` that is then dispatched as if it had
    /// arrived whole. Always sent on the ordered Reliable channel so `seq`
    /// order is preserved and no fragment is lost. `msg_id` is unique per
    /// sender only — a monotonic counter — so it need not be globally unique.
    ChunkedPayload {
        msg_id: u64,
        seq: u16,
        total: u16,
        data: Vec<u8>,
    },
    /// The sender just committed their rigged body to their PDS (#1122):
    /// the records their [`Self::AvatarStateUpdate`] references now hold
    /// different CONTENT at the same rkeys, so any resolution a peer is
    /// carrying for them is stale and must be re-fetched.
    ///
    /// It carries nothing — the sender is the message's identity, and the
    /// references are already on the peer's copy of the record. It exists
    /// because references alone cannot express "same rkey, new bytes": a
    /// re-broadcast `AvatarStateUpdate` names the same wardrobe and
    /// attachment rkeys, so `carry_resolution` (#1113, the fix for
    /// re-resolving on every keystroke) correctly carries the old
    /// resolution forward and the peer never re-fetches.
    ///
    /// **Added last on purpose.** The wire has no protocol version (#1121),
    /// and bincode encodes a variant by index — so a new arm may only be
    /// appended, where an older build meets an unknown discriminant and
    /// drops the message rather than mis-reading an existing one. An older
    /// peer therefore keeps today's behaviour (it sees the saved body on
    /// its next resolution) instead of decoding something else.
    AvatarRecordsPublished,
    /// Build handshake (#1121). Broadcast on the Reliable channel alongside
    /// [`Self::Identity`], on the same cadence, so a peer learns what wire
    /// layout the other end speaks before anything depends on it.
    ///
    /// The wire had no version at all until this arm. `OverlandsMessage` is
    /// externally tagged and rides a fixint bincode codec that rejects both
    /// trailing bytes and a short read, so ADDING A FIELD to an existing
    /// variant is a layout break as total as reordering the enum: `ItemOffer`
    /// gained `wear_json` in 59ff989, and a gift between a build from either
    /// side of that commit decodes to an error, is dropped after a
    /// rate-limited warn, and leaves the sender waiting forever for an
    /// `ItemOfferResponse` that cannot come. Neither user learns why. This
    /// variant does not FIX that — nothing can make two layouts compatible
    /// after the fact — it makes it say so, on the peer's row in the People
    /// window and in the session log.
    ///
    /// `protocol` is [`PROTOCOL_VERSION`]; `build` is the human-readable
    /// version+sha, carried only so a bug report can name the two builds that
    /// disagreed. Both are peer-supplied and therefore advisory: nothing
    /// gates on them, so a peer that lies gets a wrong chip on its own row
    /// and nothing else.
    ///
    /// Appended last, for the reason [`Self::AvatarRecordsPublished`] gives:
    /// a build that predates this arm meets an unknown discriminant and drops
    /// the message, which is exactly the outcome we want — it cannot announce
    /// a version it does not have, and its SILENCE is the signal (see
    /// [`crate::config::network::PROTOCOL_ANNOUNCE_GRACE_SECS`]).
    Hello { protocol: u16, build: String },
    /// Content digest of the world this peer built, and of the record it built
    /// it from (#1146). Broadcast on the Reliable channel after each full
    /// derivation; sixteen bytes, so the cost is nil next to the identity
    /// announce it follows.
    ///
    /// The premise of the whole thin-client model is that every peer derives
    /// the SAME world from the same record. Nothing measured that until this
    /// message existed, which is why both desyncs in this project's history
    /// (#51 terrain, #882 lots and roads) are user anecdotes rather than
    /// captured facts. `record_fp` is what makes the comparison meaningful: a
    /// difference in `digest` is only news when both peers agree they were
    /// deriving the same recipe, and during an owner's slider drag they
    /// routinely do not.
    ///
    /// Advisory in the strongest sense — the receiver logs a mismatch and
    /// changes NOTHING. A peer that sends a wrong digest gets a wrong line in
    /// somebody's log, which is the correct ceiling for a diagnostic that
    /// arrives over an unauthenticated channel.
    ///
    /// Appended last, and [`PROTOCOL_VERSION`] bumped with it — the pattern
    /// [`Self::Hello`] documents.
    WorldDigest { record_fp: u64, digest: u64 },
}

/// What an [`OverlandsMessage::ItemOffer`] is offering (#1184).
///
/// JSON inside the message rather than bincode fields beside it, so a
/// field added here is additive on the wire: serde ignores members it does
/// not know and `#[serde(default)]` fills members that are absent, which
/// means a build that grows this struct can still trade gifts with one
/// that has not. Every field is `default`ed for that reason — a decoder
/// that refuses a payload for a missing member would give the version
/// skew back.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct ItemOfferPayload {
    /// The stash name the sender knows the item by. The recipient does not
    /// have to honour it — [`crate::ui::inventory`] picks a free key.
    pub item_name: String,
    /// The item's blueprint. JSON all the way down because `Generator` is
    /// an internally tagged open union (`#[serde(tag = "$type")]`), which
    /// bincode's streaming decoder cannot handle at all.
    pub generator: Generator,
    /// Wear metadata (#1108) — socket, fit and saved offset — so a gifted
    /// wearable arrives wearable rather than as decor. Absent for plain
    /// decor.
    ///
    /// Read leniently: see [`lenient_wear`].
    #[serde(
        deserialize_with = "lenient_wear",
        skip_serializing_if = "Option::is_none"
    )]
    pub wear: Option<crate::pds::inventory::WearMeta>,
}

/// Deserialize [`ItemOfferPayload::wear`] leniently: a value that will not
/// decode yields `None` instead of failing the whole payload.
///
/// #1108's decision, preserved through the payload merge (#1184). Before
/// the merge the wear metadata was its own `Vec<u8>` with its own decoder,
/// so a malformed one degraded the gift to decor and the item still
/// arrived. Folding it into one struct would have made a bad `wear` refuse
/// the gift outright — a strictly worse outcome, since the blueprint is
/// intact and the placement is a convenience the recipient can redo.
fn lenient_wear<'de, D>(
    deserializer: D,
) -> Result<Option<crate::pds::inventory::WearMeta>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// Why an [`OverlandsMessage::ItemOffer`] was refused (#1220 f127).
///
/// The sender's only feedback used to be a boolean, so a mechanical
/// throttle and a person's choice reached them as the same sentence —
/// "@them declined" — which misattributes the throttle AND teaches the
/// sender not to retry in the one case where retrying works.
///
/// A muted sender is deliberately NOT given its own arm: telling somebody
/// they have been muted is a privacy leak, and [`Self::Declined`] is what
/// they should see.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeclineReason {
    /// A person answered no — or was muted, which reports the same way.
    #[default]
    Declined,
    /// The single-dialog anti-spam gate turned the offer away because
    /// another was already on screen. Retrying in a moment works.
    Busy,
    /// The recipient could not take it: their stash was full when the
    /// offer's time ran out.
    Unavailable,
    /// Nobody answered before the offer expired on the recipient's side.
    Unanswered,
}

/// The sentence the SENDER sees when a gift is refused (#1220 f127).
///
/// One place, because there are four reasons and they used to share one
/// sentence — "@them declined" — which misattributed a mechanical throttle
/// to a person's choice and, worse, taught the sender not to retry in the
/// one case where retrying works. A muted sender reads `Declined`
/// deliberately: telling somebody they have been muted is a privacy leak.
///
/// It lived in `ui::inventory` and had exactly one non-test caller —
/// `network::inbound::item_offer`, which is not `ui` at all (#1297).
/// The sentence explains a [`DeclineReason`]; it belongs beside the
/// enum whose four variants it exists to tell apart.
pub fn offer_refusal_line(reason: DeclineReason, who: &str, item: &str) -> String {
    match reason {
        DeclineReason::Declined => format!("{who} declined \"{item}\"."),
        DeclineReason::Busy => {
            format!("{who} was answering another offer — try \"{item}\" again in a moment.")
        }
        DeclineReason::Unavailable => {
            format!("{who} couldn't take \"{item}\" — their inventory is full.")
        }
        DeclineReason::Unanswered => format!("{who} didn't answer about \"{item}\" in time."),
    }
}

/// The answer to an [`OverlandsMessage::ItemOffer`] (#1184).
///
/// The second field arrived as predicted (#1220 f127) and cost no protocol
/// bump: `PROTOCOL_VERSION` covers the bincode variant layout, and this is a
/// key inside the JSON payload that #1184 introduced precisely so a field
/// like `reason` would be additive. An older peer sends no `reason` and
/// `#[serde(default)]` supplies `Declined`; an older peer READING this
/// payload ignores the key it does not know.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct ItemOfferResponsePayload {
    /// `true` when the recipient added the item to their inventory.
    /// Defaults to `false`, so a payload this build cannot fully read is
    /// treated as a decline rather than as a silent acceptance.
    pub accepted: bool,
    /// Why, when `accepted` is `false`. Meaningless when it is `true`.
    ///
    /// Read leniently: see [`lenient_reason`]. A reason a FUTURE build
    /// invents must degrade to `Declined` rather than fail the whole
    /// response — a response that will not decode leaves the sender's
    /// pending offer untouched until it expires three minutes later, which
    /// is a worse outcome than a slightly vague sentence.
    #[serde(deserialize_with = "lenient_reason")]
    pub reason: DeclineReason,
}

/// Deserialize [`ItemOfferResponsePayload::reason`] leniently: a value this
/// build does not recognise yields [`DeclineReason::Declined`].
///
/// The same shape as [`lenient_wear`], for the same reason — one unreadable
/// field must not cost the whole message.
fn lenient_reason<'de, D>(deserializer: D) -> Result<DeclineReason, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Version of the [`OverlandsMessage`] byte layout this build speaks.
///
/// Bump it in the same commit as any change to that layout: a new field on an
/// existing variant, a field's type, a field's order, or a new variant. (A new
/// variant appended at the end is the one change an older peer survives — it
/// drops the unknown discriminant — but it still changes what the two ends can
/// do together, so it still earns a bump.)
///
/// Starts at 1 rather than 0 so that "the peer sent no [`OverlandsMessage::Hello`]"
/// and "the peer announced version 0" are never the same reading.
///
/// History: 1 introduced the handshake itself; 2 appended
/// [`OverlandsMessage::WorldDigest`] (#1146); 3 moved
/// [`OverlandsMessage::ItemOffer`] and
/// [`OverlandsMessage::ItemOfferResponse`] onto single JSON payloads
/// (#1184) — a deliberate break, so that the *next* field either of them
/// grows is not one.
pub const PROTOCOL_VERSION: u16 = 3;

/// This build's human-readable identity for [`OverlandsMessage::Hello`]:
/// crate version plus the short git sha `build.rs` bakes in. Only ever
/// displayed and logged — never compared.
pub fn build_id() -> String {
    format!(
        "{}+{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("SYMBIOS_GIT_SHA").unwrap_or("unknown")
    )
}

/// Serialize a record for the wire, or `None` with one log line naming the
/// record kind. Shared by the two state-update constructors so they cannot
/// drift in how a refusal is reported.
fn serialize_for_wire<T: serde::Serialize>(record: &T, kind: &str) -> Option<Vec<u8>> {
    match serde_json::to_vec(record) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            bevy::log::warn!("{kind} not broadcast: {e}");
            None
        }
    }
}

impl OverlandsMessage {
    /// Package a [`RoomRecord`] for broadcast over the P2P channel.
    ///
    /// `None` when the record cannot be serialized — in practice when it
    /// holds a union arm this build decoded as `Unknown` and therefore must
    /// not write back (#1111). The caller skips the broadcast: peers read
    /// the authoritative record from the owner's PDS, decoded by whatever
    /// build understands it. Broadcasting an EMPTY payload instead, as this
    /// used to, sent every peer a record that decoded to nothing.
    pub fn room_state_update(record: &RoomRecord) -> Option<Self> {
        Some(Self::RoomStateUpdate {
            record_json: serialize_for_wire(record, "RoomRecord")?,
        })
    }

    /// Attempt to decode a [`RoomRecord`] from a `RoomStateUpdate` payload.
    /// Returns `None` if the bytes are not valid JSON or the schema drifted
    /// incompatibly — the caller should log and ignore rather than crash.
    pub fn decode_room_state(bytes: &[u8]) -> Option<RoomRecord> {
        match serde_json::from_slice(bytes) {
            Ok(r) => Some(r),
            Err(e) => {
                bevy::log::warn!("RoomRecord decode error: {}", e);
                None
            }
        }
    }

    /// Package an [`AvatarRecord`] for broadcast over the P2P channel.
    /// Same policy as [`Self::room_state_update`].
    pub fn avatar_state_update(record: &AvatarRecord) -> Option<Self> {
        Some(Self::AvatarStateUpdate {
            record_json: serialize_for_wire(record, "AvatarRecord")?,
        })
    }

    /// Decode an [`AvatarRecord`] from an `AvatarStateUpdate` payload.
    pub fn decode_avatar_state(bytes: &[u8]) -> Option<AvatarRecord> {
        match serde_json::from_slice(bytes) {
            Ok(r) => Some(r),
            Err(e) => {
                bevy::log::warn!("AvatarRecord decode error: {}", e);
                None
            }
        }
    }

    /// Package an [`ItemOffer`](Self::ItemOffer): the address stays on the
    /// message, the item goes into one JSON [`ItemOfferPayload`] (#1184).
    ///
    /// `wear` is the item's wear metadata when it is wearable (#1108);
    /// `None` gifts plain decor.
    pub fn item_offer(
        offer_id: u64,
        target_did: String,
        item_name: String,
        generator: &Generator,
        wear: Option<&crate::pds::inventory::WearMeta>,
    ) -> Self {
        let payload = ItemOfferPayload {
            item_name,
            generator: generator.clone(),
            wear: wear.cloned(),
        };
        Self::ItemOffer {
            offer_id,
            target_did,
            payload_json: serialize_for_wire(&payload, "ItemOffer").unwrap_or_default(),
        }
    }

    /// Package an [`ItemOfferResponse`](Self::ItemOfferResponse).
    ///
    /// `target_did` is the *original sender*, which is who the answer is
    /// addressed to.
    pub fn item_offer_response(
        offer_id: u64,
        target_did: String,
        accepted: bool,
        reason: DeclineReason,
    ) -> Self {
        let payload = ItemOfferResponsePayload { accepted, reason };
        Self::ItemOfferResponse {
            offer_id,
            target_did,
            payload_json: serialize_for_wire(&payload, "ItemOfferResponse").unwrap_or_default(),
        }
    }

    /// Decode an [`ItemOfferPayload`] from an `ItemOffer`.
    pub fn decode_item_offer(bytes: &[u8]) -> Option<ItemOfferPayload> {
        match serde_json::from_slice(bytes) {
            Ok(payload) => Some(payload),
            Err(e) => {
                bevy::log::warn!("ItemOffer payload decode error: {}", e);
                None
            }
        }
    }

    /// Decode an [`ItemOfferResponsePayload`].
    ///
    /// A payload that will not decode is **not** read as an acceptance:
    /// the caller gets `None` and leaves its pending offer alone rather
    /// than telling the sender a gift landed.
    pub fn decode_item_offer_response(bytes: &[u8]) -> Option<ItemOfferResponsePayload> {
        match serde_json::from_slice(bytes) {
            Ok(payload) => Some(payload),
            Err(e) => {
                bevy::log::warn!("ItemOfferResponse payload decode error: {}", e);
                None
            }
        }
    }

    /// Serialize a whole message to the byte form the chunker splits and the
    /// receiver reassembles ([`Self::ChunkedPayload`]). Uses `bincode` — the
    /// same compact codec the multiuser data channel uses on the wire — rather
    /// than JSON, because `serde_json` encodes the `record_json` /
    /// `generator_json` `Vec<u8>` payloads as a number array (~3.5× bloat),
    /// which would over-fragment every message and inflate the measured size
    /// far past the true transmitted size. `OverlandsMessage` is externally
    /// tagged, so bincode encodes it cleanly (unlike the internally-tagged
    /// `RoomRecord` inside `record_json`, which is why *that* stays JSON).
    pub fn to_chunk_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        use bincode::Options;
        bincode::DefaultOptions::new().serialize(self)
    }

    /// Decode a reassembled [`OverlandsMessage`] from concatenated
    /// [`Self::ChunkedPayload`] fragments. Bounded by
    /// [`crate::config::network::MAX_RELIABLE_PAYLOAD_BYTES`] so a hostile peer
    /// cannot craft a length prefix that provokes a huge allocation. `None` on
    /// malformed bytes — the caller logs and drops rather than crashing.
    pub fn from_chunk_bytes(bytes: &[u8]) -> Option<Self> {
        use bincode::Options;
        let opts = bincode::DefaultOptions::new()
            .with_limit(crate::config::network::MAX_RELIABLE_PAYLOAD_BYTES as u64);
        match opts.deserialize(bytes) {
            Ok(m) => Some(m),
            Err(e) => {
                bevy::log::warn!("Reassembled ChunkedPayload decode error: {}", e);
                None
            }
        }
    }
}

#[cfg(test)]
mod item_offer_tests {
    use super::*;
    use crate::pds::inventory::WearMeta;

    /// #1220 f127. The sequence: you gift two friends in quick succession,
    /// the second one's client is still showing the first dialog, and you
    /// are told "@second declined" — a mechanical throttle reported as a
    /// person's choice, and phrasing that teaches you not to retry in the
    /// one case where retrying works.
    #[test]
    fn each_refusal_reads_as_the_thing_that_actually_happened() {
        let declined = offer_refusal_line(DeclineReason::Declined, "@them", "lantern");
        assert!(declined.contains("declined"), "{declined}");

        let busy = offer_refusal_line(DeclineReason::Busy, "@them", "lantern");
        assert!(
            !busy.contains("declined"),
            "a throttle is not a refusal: {busy}"
        );
        assert!(busy.contains("again"), "and it must invite a retry: {busy}");

        let full = offer_refusal_line(DeclineReason::Unavailable, "@them", "lantern");
        assert!(full.contains("full"), "{full}");
        assert!(!full.contains("declined"), "{full}");

        let quiet = offer_refusal_line(DeclineReason::Unanswered, "@them", "lantern");
        assert!(quiet.contains("didn't answer"), "{quiet}");
        assert!(!quiet.contains("declined"), "{quiet}");
    }

    /// Every sentence names the item and the person, because the sender may
    /// have several offers out at once and a toast that says only "declined"
    /// is unattributable.
    #[test]
    fn every_refusal_names_the_person_and_the_item() {
        for reason in [
            DeclineReason::Declined,
            DeclineReason::Busy,
            DeclineReason::Unavailable,
            DeclineReason::Unanswered,
        ] {
            let line = offer_refusal_line(reason, "@them", "lantern");
            assert!(line.contains("@them"), "{line}");
            assert!(line.contains("lantern"), "{line}");
        }
    }

    fn wear() -> WearMeta {
        let mut meta = WearMeta::for_entry(symbios_avatar::Socket::Crown, None);
        meta.fit_band_mm = 178;
        meta.offset.translation = crate::pds::types::Fp3([0.0, 0.02, 0.0]);
        meta
    }

    /// Unpack an offer's payload bytes, whatever the envelope says.
    fn payload_of(message: &OverlandsMessage) -> Vec<u8> {
        let OverlandsMessage::ItemOffer { payload_json, .. } = message else {
            panic!("not an ItemOffer");
        };
        payload_json.clone()
    }

    /// #1108: the wear metadata survives the whole wire path — JSON inside
    /// the message, the message through bincode (the chunker's codec) —
    /// and comes back equal, fit and offset included. Decor carries none.
    #[test]
    fn a_gifted_wearable_keeps_its_wear_metadata_across_the_wire() {
        let offer = OverlandsMessage::item_offer(
            7,
            String::from("did:plc:recipient"),
            String::from("circlet"),
            &Generator::default(),
            Some(&wear()),
        );
        let bytes = offer.to_chunk_bytes().expect("encodes");
        let landed = OverlandsMessage::from_chunk_bytes(&bytes).expect("decodes");
        let payload =
            OverlandsMessage::decode_item_offer(&payload_of(&landed)).expect("payload decodes");
        assert_eq!(payload.item_name, "circlet");
        assert_eq!(
            payload.wear,
            Some(wear()),
            "socket, fit and offset all round-trip"
        );

        let decor = OverlandsMessage::item_offer(
            8,
            String::from("did:plc:recipient"),
            String::from("bench"),
            &Generator::default(),
            None,
        );
        let payload =
            OverlandsMessage::decode_item_offer(&payload_of(&decor)).expect("payload decodes");
        assert_eq!(payload.wear, None, "decor carries no wear metadata");
        assert!(
            !String::from_utf8_lossy(&payload_of(&decor)).contains("wear"),
            "and elides the member entirely rather than shipping a null"
        );
    }

    /// A malformed wear payload must not refuse the gift: the item is
    /// intact, so it lands as decor.
    ///
    /// This was free when `wear` was its own `Vec<u8>` with its own
    /// decoder. Folding it into one payload (#1184) would have made a bad
    /// `wear` fail the whole struct and lose the item too, which is why
    /// the member is read through `lenient_wear`.
    #[test]
    fn a_malformed_wear_payload_degrades_to_decor() {
        let payload = OverlandsMessage::decode_item_offer(
            br#"{"item_name":"circlet","generator":{"$type":"empty"},"wear":{"socket":12}}"#,
        )
        .expect("the gift still decodes");
        assert_eq!(payload.item_name, "circlet", "the item survives");
        assert_eq!(payload.wear, None, "only the placement is lost");
    }

    /// **A field this build has never heard of does not break the gift**
    /// (#1184).
    ///
    /// The whole reason the variant moved. `wear_json` was appended to
    /// `ItemOffer` in 59ff989 as a fifth bincode field, and because
    /// bincode identifies fields by position, every gift across that
    /// boundary decoded as an error the receiver could only drop —
    /// silently, for as long as it took #1121 to build a detector. Inside
    /// a JSON payload the same addition is invisible to an older peer.
    ///
    /// A round-trip test would have passed before this change too, since
    /// each build agrees with itself; what this asserts is a payload the
    /// *encoder in this build cannot produce*.
    #[test]
    fn a_payload_carrying_an_unknown_field_still_decodes() {
        let from_a_newer_build = br#"{
            "item_name": "circlet",
            "generator": {"$type": "empty"},
            "gift_note": "happy birthday",
            "bound_to_did": "did:plc:someone"
        }"#;
        let payload = OverlandsMessage::decode_item_offer(from_a_newer_build)
            .expect("unknown members are skipped, not refused");
        assert_eq!(payload.item_name, "circlet");
        assert_eq!(payload.wear, None, "an absent member defaults");

        // And the other direction: a payload MISSING a member this build
        // knows about, as an older build would send it.
        let from_an_older_build = br#"{"generator": {"$type": "empty"}}"#;
        let payload = OverlandsMessage::decode_item_offer(from_an_older_build)
            .expect("missing members default, they do not refuse");
        assert_eq!(
            payload.item_name, "",
            "clamped to (unnamed) by the receiver"
        );
    }

    /// **A response this build cannot read is a decline, never an
    /// acceptance** (#1184).
    ///
    /// The asymmetry is deliberate: `accepted` defaults to `false` and an
    /// outright decode failure yields `None`. The worst a version-skewed
    /// peer can do is make a gift that arrived look declined; it can never
    /// tell the sender a gift landed when it did not.
    #[test]
    fn an_unreadable_offer_response_declines() {
        let OverlandsMessage::ItemOfferResponse { payload_json, .. } =
            OverlandsMessage::item_offer_response(
                1,
                String::from("did:plc:alice"),
                true,
                DeclineReason::Declined,
            )
        else {
            panic!("not a response");
        };
        assert_eq!(
            OverlandsMessage::decode_item_offer_response(&payload_json).map(|p| p.accepted),
            Some(true)
        );
        assert_eq!(
            OverlandsMessage::decode_item_offer_response(b"{}").map(|p| p.accepted),
            Some(false),
            "a payload with no verdict is not a yes"
        );
        assert!(
            OverlandsMessage::decode_item_offer_response(b"not json").is_none(),
            "and one that will not parse at all is not a yes either"
        );
    }

    /// #1220 f127. The decline reason travels in THREE directions, which is
    /// what a schema test owes (#211/#212): this build round-trips its own
    /// payload; an OLDER peer's payload, which has no `reason` key at all,
    /// decodes to the safe default; and an older peer READING this build's
    /// payload ignores the key it does not know.
    #[test]
    fn the_decline_reason_travels_in_all_three_directions() {
        // 1. This build to itself.
        for reason in [
            DeclineReason::Declined,
            DeclineReason::Busy,
            DeclineReason::Unavailable,
            DeclineReason::Unanswered,
        ] {
            let OverlandsMessage::ItemOfferResponse { payload_json, .. } =
                OverlandsMessage::item_offer_response(
                    7,
                    String::from("did:plc:alice"),
                    false,
                    reason,
                )
            else {
                panic!("not a response");
            };
            let back = OverlandsMessage::decode_item_offer_response(&payload_json)
                .expect("our own payload decodes");
            assert_eq!(back.reason, reason);
            assert!(!back.accepted);
        }

        // 2. An older peer's payload — the shape before this field existed.
        let old = br#"{"accepted":false}"#;
        let back = OverlandsMessage::decode_item_offer_response(old)
            .expect("a payload without the field still decodes");
        assert_eq!(
            back.reason,
            DeclineReason::Declined,
            "an older peer's silence must read as the sentence it has always produced"
        );

        // 3. An older peer reading ours: it knows only `accepted`, and
        // serde ignores keys a struct does not declare.
        #[derive(serde::Deserialize, Default)]
        #[serde(default)]
        struct OldPayload {
            accepted: bool,
        }
        let OverlandsMessage::ItemOfferResponse { payload_json, .. } =
            OverlandsMessage::item_offer_response(
                7,
                String::from("did:plc:alice"),
                true,
                DeclineReason::Declined,
            )
        else {
            panic!("not a response");
        };
        let old_reader: OldPayload =
            serde_json::from_slice(&payload_json).expect("an older build still reads the verdict");
        assert!(
            old_reader.accepted,
            "the field this build added must not cost an older peer the answer it came for"
        );
    }

    /// A reason a FUTURE build invents degrades to `Declined` rather than
    /// failing the whole response — a response that will not decode leaves
    /// the sender's pending offer untouched until it expires three minutes
    /// later, which is worse than a slightly vague sentence.
    #[test]
    fn an_unknown_decline_reason_does_not_cost_the_whole_response() {
        let future = br#"{"accepted":false,"reason":"eatenByABear"}"#;
        let back = OverlandsMessage::decode_item_offer_response(future)
            .expect("an unknown reason is not a decode failure");
        assert_eq!(back.reason, DeclineReason::Declined);
        assert!(!back.accepted);

        // And the verdict still survives when the unknown reason rides
        // alongside an acceptance.
        let accepted = br#"{"accepted":true,"reason":"somethingNew"}"#;
        let back = OverlandsMessage::decode_item_offer_response(accepted).expect("decodes");
        assert!(back.accepted);
    }

    /// The wire names are the camelCase ones a lexicon would declare, and
    /// they are pinned: renaming a variant silently re-addresses what every
    /// other peer reads.
    #[test]
    fn the_decline_reason_wire_names_are_pinned() {
        for (reason, wire) in [
            (DeclineReason::Declined, "declined"),
            (DeclineReason::Busy, "busy"),
            (DeclineReason::Unavailable, "unavailable"),
            (DeclineReason::Unanswered, "unanswered"),
        ] {
            assert_eq!(
                serde_json::to_string(&reason).expect("serialises"),
                format!("\"{wire}\""),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant, in declaration order — the list the discriminant pin
    /// below walks. A new arm belongs at the END of both.
    fn one_of_each() -> Vec<OverlandsMessage> {
        vec![
            OverlandsMessage::Transform {
                position: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
            OverlandsMessage::Identity {
                did: String::from("did:plc:alice"),
                handle: String::from("alice.test"),
            },
            OverlandsMessage::Chat {
                text: String::from("hi"),
            },
            OverlandsMessage::RoomStateUpdate {
                record_json: Vec::new(),
            },
            OverlandsMessage::AvatarStateUpdate {
                record_json: Vec::new(),
            },
            OverlandsMessage::ItemOffer {
                offer_id: 1,
                target_did: String::from("did:plc:bob"),
                payload_json: Vec::new(),
            },
            OverlandsMessage::ItemOfferResponse {
                offer_id: 1,
                target_did: String::from("did:plc:alice"),
                payload_json: Vec::new(),
            },
            OverlandsMessage::ChunkedPayload {
                msg_id: 1,
                seq: 0,
                total: 1,
                data: Vec::new(),
            },
            OverlandsMessage::AvatarRecordsPublished,
            OverlandsMessage::Hello {
                protocol: 1,
                build: String::from("0.0.0+abcdef1"),
            },
            OverlandsMessage::WorldDigest {
                record_fp: 1,
                digest: 2,
            },
        ]
    }

    /// The wire carries no protocol version (#1121), and bincode identifies
    /// a variant by its INDEX — so an arm inserted anywhere but the end
    /// silently re-points every later one, and two builds in the same room
    /// would read each other's messages as the wrong kind with no error.
    /// This pins the mapping: #1122's `AvatarRecordsPublished` is last, and
    /// the eight before it kept the numbers they shipped with.
    #[test]
    fn variant_discriminants_are_append_only() {
        for (index, message) in one_of_each().into_iter().enumerate() {
            let bytes = message.to_chunk_bytes().expect("serializes");
            assert_eq!(
                bytes.first().copied(),
                Some(index as u8),
                "variant {index} moved on the wire — a new arm must be appended, never inserted"
            );
        }
    }

    /// And the new notice survives the chunk codec it will actually travel
    /// through. It has no payload, so this is really a check that a unit
    /// variant round-trips at all.
    #[test]
    fn the_publish_notice_round_trips() {
        let bytes = OverlandsMessage::AvatarRecordsPublished
            .to_chunk_bytes()
            .expect("serializes");
        assert!(matches!(
            OverlandsMessage::from_chunk_bytes(&bytes),
            Some(OverlandsMessage::AvatarRecordsPublished)
        ));
    }

    /// The byte layout of every variant, as the DATA CHANNEL encodes it —
    /// `bevy_symbios_multiuser::systems::bincode_options()`, fixint and
    /// little-endian, which is a different encoding from the varint
    /// `DefaultOptions` [`OverlandsMessage::to_chunk_bytes`] uses inside a
    /// fragment. Pinning the chunk codec alone (the test above) would have
    /// missed this entirely.
    ///
    /// Generated once from [`one_of_each`]; each entry is the full hex
    /// encoding of that variant with the field values that function supplies.
    /// Do not "fix" a failure by re-generating it — a changed line means the
    /// wire moved, and the questions are whether the change was appended and
    /// whether [`PROTOCOL_VERSION`] went up in the same commit.
    const WIRE_LAYOUT: &[(&str, &str)] = &[
        (
            "Transform",
            "000000000000000000000000000000000000000000000000000000000000803f",
        ),
        (
            "Identity",
            "010000000d000000000000006469643a706c633a616c6963650a00000000000000616c6963652e74657374",
        ),
        ("Chat", "0200000002000000000000006869"),
        ("RoomStateUpdate", "030000000000000000000000"),
        ("AvatarStateUpdate", "040000000000000000000000"),
        // #1184 moved both of these onto a single JSON payload, so what
        // is pinned here is now the ENVELOPE — discriminant, offer id,
        // address, payload length — and the payload itself is opaque
        // bytes, exactly as it is for the two state updates above. That is
        // the point: fields added inside the payload no longer move any
        // line in this table, which is what stops the next `wear_json`
        // from being a silent break.
        (
            "ItemOffer",
            "0500000001000000000000000b000000000000006469643a706c633a626f620000000000000000",
        ),
        (
            "ItemOfferResponse",
            "0600000001000000000000000d000000000000006469643a706c633a616c6963650000000000000000",
        ),
        (
            "ChunkedPayload",
            "070000000100000000000000000001000000000000000000",
        ),
        ("AvatarRecordsPublished", "08000000"),
        (
            "Hello",
            "0900000001000d00000000000000302e302e302b61626364656631",
        ),
        ("WorldDigest", "0a00000001000000000000000200000000000000"),
    ];

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// #1121. The failure this pins is the one that already shipped:
    /// `ItemOffer` gained `wear_json` in 59ff989 with no version bump and no
    /// test, so the break reached users as a gift that never arrived. A field
    /// added to any variant here now fails this assertion instead — which is
    /// the whole point, because the wire cannot tell you it broke and the
    /// receiving peer only ever sees a decode error it must drop.
    ///
    /// Deliberately a byte pin and not a round-trip: a round-trip passes
    /// happily on both sides of a layout change, since each build agrees with
    /// itself. Only a committed constant catches a change that two builds
    /// would disagree about.
    #[test]
    fn the_wire_layout_of_every_variant_is_pinned() {
        use bincode::Options;
        let messages = one_of_each();
        assert_eq!(
            messages.len(),
            WIRE_LAYOUT.len(),
            "a variant was added or removed without a line in WIRE_LAYOUT — \
             append the new arm to both, and bump PROTOCOL_VERSION"
        );
        for (message, (name, expected)) in messages.iter().zip(WIRE_LAYOUT) {
            let bytes = bevy_symbios_multiuser::systems::bincode_options()
                .serialize(message)
                .expect("serializes");
            let actual = hex(&bytes);
            assert_eq!(
                &actual, expected,
                "the wire layout of {name} moved: a peer on the other side of \
                 this change decodes an error and drops the message"
            );
        }
    }

    /// A version nobody bumps is a version nobody has. This does not prove the
    /// bump happened — no test can — but it pins the pairing so the two facts
    /// live in one place: when [`WIRE_LAYOUT`] above fails, this constant is
    /// what has to move with it.
    #[test]
    fn the_protocol_version_matches_the_pinned_layout() {
        assert_eq!(
            PROTOCOL_VERSION, 3,
            "PROTOCOL_VERSION changed: WIRE_LAYOUT must have changed in the \
             same commit, or the bump describes nothing"
        );
    }
}
