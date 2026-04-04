use serde::{Deserialize, Serialize};

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
    /// Reliable identity announcement sent on join and periodically thereafter.
    /// Now includes per-peer airship construction/material parameters.
    Identity {
        did: String,
        handle: String,
        airship: AirshipParams,
    },
    /// Chat message sent over the Reliable channel.
    Chat { text: String },
}
