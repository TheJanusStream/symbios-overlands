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
    /// `wear_json` carries the item's wear metadata (#1108) — socket, fit
    /// and saved offset, a JSON-serialised
    /// [`WearMeta`](crate::pds::inventory::WearMeta) — so a gifted
    /// wearable arrives wearable rather than as decor. Empty for plain
    /// decor. JSON rather than a bincode `Option<WearMeta>` because the
    /// offset's serializer elides default fields, which bincode cannot
    /// round-trip (its decoder expects every field present).
    ItemOffer {
        offer_id: u64,
        target_did: String,
        item_name: String,
        generator_json: Vec<u8>,
        wear_json: Vec<u8>,
    },
    /// Reply to an [`Self::ItemOffer`]. The `target_did` is the *sender* of
    /// the original offer so non-originators can drop the response on
    /// receipt; `accepted = true` means the recipient added the item to
    /// their inventory, `false` covers decline / mute / busy / full /
    /// over-capacity — the sender just needs a yes/no for UX feedback.
    ItemOfferResponse {
        offer_id: u64,
        target_did: String,
        accepted: bool,
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

    /// Package an [`ItemOffer`](Self::ItemOffer). Serialises the `Generator`
    /// blueprint as JSON for the same reason room/avatar updates do —
    /// bincode cannot stream `#[serde(tag = "$type")]` open unions.
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
        Self::ItemOffer {
            offer_id,
            target_did,
            item_name,
            generator_json: serde_json::to_vec(generator).unwrap_or_else(|e| {
                bevy::log::error!("Failed to serialize Generator for ItemOffer: {}", e);
                Vec::new()
            }),
            wear_json: wear.map_or_else(Vec::new, |meta| {
                serde_json::to_vec(meta).unwrap_or_else(|e| {
                    bevy::log::error!("Failed to serialize WearMeta for ItemOffer: {}", e);
                    Vec::new()
                })
            }),
        }
    }

    /// Decode a [`Generator`] from an `ItemOffer` payload.
    pub fn decode_item_offer(bytes: &[u8]) -> Option<Generator> {
        match serde_json::from_slice(bytes) {
            Ok(g) => Some(g),
            Err(e) => {
                bevy::log::warn!("Generator decode error: {}", e);
                None
            }
        }
    }

    /// Decode the wear metadata of an `ItemOffer` (#1108): `None` for an
    /// empty payload (decor) — and for a malformed one, which degrades the
    /// gift to decor rather than refusing it, because the item itself is
    /// intact and the metadata is a convenience the recipient can redo.
    pub fn decode_item_offer_wear(bytes: &[u8]) -> Option<crate::pds::inventory::WearMeta> {
        if bytes.is_empty() {
            return None;
        }
        match serde_json::from_slice(bytes) {
            Ok(meta) => Some(meta),
            Err(e) => {
                bevy::log::warn!("WearMeta decode error (gift lands as decor): {}", e);
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

    fn wear() -> WearMeta {
        let mut meta = WearMeta::for_entry(symbios_avatar::Socket::Crown, None);
        meta.fit_band_mm = 178;
        meta.offset.translation = crate::pds::types::Fp3([0.0, 0.02, 0.0]);
        meta
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
        let OverlandsMessage::ItemOffer {
            generator_json,
            wear_json,
            ..
        } = OverlandsMessage::from_chunk_bytes(&bytes).expect("decodes")
        else {
            panic!("not an ItemOffer");
        };
        assert!(OverlandsMessage::decode_item_offer(&generator_json).is_some());
        assert_eq!(
            OverlandsMessage::decode_item_offer_wear(&wear_json),
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
        let OverlandsMessage::ItemOffer { wear_json, .. } = decor else {
            panic!("not an ItemOffer");
        };
        assert!(wear_json.is_empty(), "decor ships no wear payload");
        assert_eq!(OverlandsMessage::decode_item_offer_wear(&wear_json), None);
    }

    /// A malformed wear payload must not refuse the gift: the item is
    /// intact, so it lands as decor.
    #[test]
    fn a_malformed_wear_payload_degrades_to_decor() {
        assert_eq!(
            OverlandsMessage::decode_item_offer_wear(b"{\"socket\": 12}"),
            None
        );
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
                item_name: String::from("hat"),
                generator_json: Vec::new(),
                wear_json: Vec::new(),
            },
            OverlandsMessage::ItemOfferResponse {
                offer_id: 1,
                target_did: String::from("did:plc:alice"),
                accepted: true,
            },
            OverlandsMessage::ChunkedPayload {
                msg_id: 1,
                seq: 0,
                total: 1,
                data: Vec::new(),
            },
            OverlandsMessage::AvatarRecordsPublished,
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
}
