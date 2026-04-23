//! Wire protocol for `OverlandsMessage`. The message enum is Serde-tagged
//! and rides the `bevy_symbios_multiuser` data channels; each variant's
//! docstring records which channel it is expected to travel over.
//!
//! Avatar records are **not** broadcast inline — Identity carries just the
//! peer's DID/handle, and the receiver fetches the signed `AvatarRecord`
//! from the owner's PDS directly. The lightweight `AvatarStateUpdate`
//! variant nudges peers to re-fetch after a live edit.

use serde::{Deserialize, Serialize};

use crate::pds::{AvatarRecord, Generator, RoomRecord};

/// Shape of the outrigger pontoons.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub enum PontoonShape {
    #[default]
    Capsule,
    VHull,
}

/// All messages exchanged over the P2P network.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OverlandsMessage {
    /// Physics transform broadcast at ~60 Hz over the Unreliable channel.
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
    /// [`bevy_symbios_multiuser::Broadcast`] has no directed-send primitive —
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
    ItemOffer {
        offer_id: u64,
        target_did: String,
        item_name: String,
        generator_json: Vec<u8>,
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
}

impl OverlandsMessage {
    /// Package a [`RoomRecord`] for broadcast over the P2P channel. Falls
    /// back to an empty payload if serialisation fails, which the receiver
    /// will drop — the alternative of panicking on a malformed record would
    /// tear down the session mid-edit.
    pub fn room_state_update(record: &RoomRecord) -> Self {
        Self::RoomStateUpdate {
            record_json: serde_json::to_vec(record).unwrap_or_else(|e| {
                bevy::log::error!("Failed to serialize RoomRecord: {}", e);
                Vec::new()
            }),
        }
    }

    /// Attempt to decode a[`RoomRecord`] from a `RoomStateUpdate` payload.
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
    /// Same fallback policy as [`Self::room_state_update`].
    pub fn avatar_state_update(record: &AvatarRecord) -> Self {
        Self::AvatarStateUpdate {
            record_json: serde_json::to_vec(record).unwrap_or_else(|e| {
                bevy::log::error!("Failed to serialize AvatarRecord: {}", e);
                Vec::new()
            }),
        }
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
    pub fn item_offer(
        offer_id: u64,
        target_did: String,
        item_name: String,
        generator: &Generator,
    ) -> Self {
        Self::ItemOffer {
            offer_id,
            target_did,
            item_name,
            generator_json: serde_json::to_vec(generator).unwrap_or_else(|e| {
                bevy::log::error!("Failed to serialize Generator for ItemOffer: {}", e);
                Vec::new()
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
}
