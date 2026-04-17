//! Wire protocol for `OverlandsMessage`. The message enum is Serde-tagged
//! and rides the `bevy_symbios_multiuser` data channels; each variant's
//! docstring records which channel it is expected to travel over.
//!
//! Avatar records are **not** broadcast inline — Identity carries just the
//! peer's DID/handle, and the receiver fetches the signed `AvatarRecord`
//! from the owner's PDS directly. The lightweight `AvatarStateUpdate`
//! variant nudges peers to re-fetch after a live edit.

use serde::{Deserialize, Serialize};

use crate::pds::{AvatarRecord, RoomRecord};

/// Shape of the outrigger pontoons.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub enum PontoonShape {
    #[default]
    Capsule,
    VHull,
}

/// Per-peer airship construction and material parameters, included in Identity
/// messages so every peer can set their own vessel appearance.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct AirshipParams {
    // --- Construction -------------------------------------------------------
    /// Overall hull length (m).
    pub hull_length: f32,
    /// Overall hull width (m).
    pub hull_width: f32,
    /// Lateral centre-to-pontoon distance (m).
    pub pontoon_spread: f32,
    /// Length of each outrigger pontoon (m).
    pub pontoon_length: f32,
    /// Cross-section width of each outrigger pontoon (m).
    pub pontoon_width: f32,
    /// Cross-section height of each outrigger pontoon (m); keel depth for V-hull.
    pub pontoon_height: f32,
    /// Shape of the outrigger pontoons.
    pub pontoon_shape: PontoonShape,
    /// Downward offset for struts & pontoons as fraction (0–1) of hull keel depth.
    pub strut_drop: f32,
    /// Height of the central mast (m).
    pub mast_height: f32,
    /// Radius of the central mast cylinder (m).
    pub mast_radius: f32,
    /// 2D offset [X, Z] of the mast position on the deck (m).
    pub mast_offset: [f32; 2],
    /// Side length of the square solar sail (m).
    pub sail_size: f32,
    /// Depth of the V-hull keel below the deck rim (m).
    pub hull_depth: f32,
    // --- Material -----------------------------------------------------------
    /// Hull base colour [sRGB; 0-1].
    pub hull_color: [f32; 3],
    /// Pontoon base colour [sRGB; 0-1].
    pub pontoon_color: [f32; 3],
    /// Mast base colour [sRGB; 0-1].
    pub mast_color: [f32; 3],
    /// Strut base colour [sRGB; 0-1].
    pub strut_color: [f32; 3],
    /// PBR metallic factor (0–1).
    pub metallic: f32,
    /// PBR perceptual roughness (0–1).
    pub roughness: f32,
}

impl AirshipParams {
    /// Clamp every numeric field to a safe range before the values feed
    /// Bevy primitive constructors (`Capsule3d::new`, `Cylinder::new`,
    /// `Sphere::new`, `Rectangle::new`), which panic on negative, zero, or
    /// NaN inputs. A malicious peer can otherwise crash every guest by
    /// broadcasting an Identity with `pontoon_width = -1.0`.
    pub fn sanitize(&mut self) {
        const MIN_DIM: f32 = 0.01;
        const MAX_DIM: f32 = 50.0;
        let clamp = |v: f32| {
            if v.is_finite() {
                v.clamp(MIN_DIM, MAX_DIM)
            } else {
                MIN_DIM
            }
        };
        let clamp_unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let clamp_color = |c: [f32; 3]| [clamp_unit(c[0]), clamp_unit(c[1]), clamp_unit(c[2])];
        let clamp_offset = |v: f32| {
            if v.is_finite() {
                v.clamp(-MAX_DIM, MAX_DIM)
            } else {
                0.0
            }
        };

        self.hull_length = clamp(self.hull_length);
        self.hull_width = clamp(self.hull_width);
        self.pontoon_spread = clamp(self.pontoon_spread);
        self.pontoon_length = clamp(self.pontoon_length);
        self.pontoon_width = clamp(self.pontoon_width);
        self.pontoon_height = clamp(self.pontoon_height);
        self.strut_drop = clamp_unit(self.strut_drop);
        self.mast_height = clamp(self.mast_height);
        self.mast_radius = clamp(self.mast_radius);
        self.mast_offset = [
            clamp_offset(self.mast_offset[0]),
            clamp_offset(self.mast_offset[1]),
        ];
        self.sail_size = clamp(self.sail_size);
        self.hull_depth = clamp(self.hull_depth);
        self.hull_color = clamp_color(self.hull_color);
        self.pontoon_color = clamp_color(self.pontoon_color);
        self.mast_color = clamp_color(self.mast_color);
        self.strut_color = clamp_color(self.strut_color);
        self.metallic = clamp_unit(self.metallic);
        self.roughness = clamp_unit(self.roughness);
    }
}

impl Default for AirshipParams {
    fn default() -> Self {
        use crate::config::airship as cfg;
        Self {
            hull_length: cfg::HULL_LENGTH,
            hull_width: cfg::HULL_WIDTH,
            pontoon_spread: cfg::PONTOON_SPREAD,
            pontoon_length: cfg::PONTOON_LENGTH,
            pontoon_width: cfg::PONTOON_WIDTH,
            pontoon_height: cfg::PONTOON_HEIGHT,
            pontoon_shape: PontoonShape::default(),
            strut_drop: cfg::STRUT_DROP,
            mast_height: cfg::MAST_HEIGHT,
            mast_radius: cfg::MAST_RADIUS,
            mast_offset: cfg::MAST_OFFSET,
            sail_size: cfg::SAIL_SIZE,
            hull_depth: cfg::HULL_DEPTH,
            hull_color: cfg::HULL_COLOR,
            pontoon_color: cfg::PONTOON_COLOR,
            mast_color: cfg::MAST_COLOR,
            strut_color: cfg::STRUT_COLOR,
            metallic: cfg::METALLIC,
            roughness: cfg::ROUGHNESS,
        }
    }
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
}
