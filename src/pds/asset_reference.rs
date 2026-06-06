//! Canonical sovereign asset reference — a URL or DID-pinned blob pointer
//! shared by every dropdown that lets the owner slot an external asset
//! alongside the procedural-generator variants.
//!
//! # Scope
//!
//! Originally introduced as `SignSource` to back the
//! [`Sign`](crate::pds::GeneratorKind::Sign) image source
//! union; generalised here so [`SovereignTextureConfig::Referenced`] and
//! the future `SovereignAudioConfig::Referenced` can reuse the same wire
//! shape and resolver path. [`SignSource`] is retained as a type alias so
//! existing call sites (and stored room records) keep working without
//! migration.
//!
//! # Variants
//!
//! * [`Url`](SovereignAssetReference::Url) — direct HTTPS GET via the
//!   shared `reqwest` client. CORS is the host's responsibility on web.
//! * [`AtprotoBlob`](SovereignAssetReference::AtprotoBlob) — resolves the
//!   DID's PDS then calls `com.atproto.sync.getBlob?did=…&cid=…`. Pinned,
//!   content-addressed, reproducible.
//! * [`DidPfp`](SovereignAssetReference::DidPfp) — fetches
//!   `app.bsky.actor.getProfile` and follows the avatar URL.
//!   Self-updating: a refresh between sessions picks up a new pfp without
//!   changing the record. Image-only — the audio bridge UI should hide
//!   this variant from its sub-picker since a JPEG isn't an audio source.
//! * [`Unknown`](SovereignAssetReference::Unknown) — forward-compat seam.
//!   A record authored by a newer engine version round-trips intact
//!   through older clients.
//!
//! # Wire format
//!
//! The `$type` tags remain `network.symbios.sign.*` for backwards
//! compatibility with already-published records. They are NSID-style
//! tokens, not user-visible; the variant rename is purely a code-level
//! change.
//!
//! [`SignSource`]: crate::pds::SignSource
//! [`SovereignTextureConfig::Referenced`]: crate::pds::texture::SovereignTextureConfig

use serde::{Deserialize, Serialize};

/// Image- / blob-source open union shared by every "URL or DID" reference
/// in the engine. All variants resolve through the shared
/// [`BlobImageCache`] (for image bytes) or a sibling resolver (for audio
/// bytes), keyed by the same logical identity so a room scattering many
/// references to the same source issues one HTTPS round trip and reuses
/// the resulting handle across every consumer.
///
/// [`BlobImageCache`]: crate::world_builder::image_cache::BlobImageCache
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "$type")]
pub enum SovereignAssetReference {
    /// Direct HTTPS URL. Bytes are decoded by the consumer-specific
    /// pipeline (image, audio, …); on WASM the request goes through the
    /// same `reqwest` client as every other HTTP fetch. CORS is the
    /// host's responsibility on web; a server that doesn't serve
    /// `Access-Control-Allow-Origin: *` will fail to load on web.
    #[serde(rename = "network.symbios.sign.url")]
    Url { url: String },
    /// ATProto blob ref pinned to a specific DID. Resolves the DID's PDS
    /// then calls `com.atproto.sync.getBlob?did=…&cid=…`. Use this when
    /// the asset is hosted on a known PDS as a content-addressed blob —
    /// the CID makes the reference reproducible.
    #[serde(rename = "network.symbios.sign.atproto_blob")]
    AtprotoBlob { did: String, cid: String },
    /// "This DID's current profile picture" — fetches `app.bsky.actor.
    /// getProfile` and resolves the avatar URL through the same path
    /// Portal uses today. Self-updating: a refresh between sessions picks
    /// up a new pfp without changing the record.
    ///
    /// Image-only. Audio consumers should treat this as `Unknown`.
    #[serde(rename = "network.symbios.sign.did_pfp")]
    DidPfp { did: String },

    #[serde(other)]
    Unknown,
}

impl Default for SovereignAssetReference {
    fn default() -> Self {
        SovereignAssetReference::Url { url: String::new() }
    }
}

impl SovereignAssetReference {
    /// Human-readable variant name for the sub-source picker rendered
    /// when a parent dropdown selects "Referenced".
    pub fn label(&self) -> &'static str {
        match self {
            Self::Url { .. } => "URL",
            Self::AtprotoBlob { .. } => "ATProto Blob (DID + CID)",
            Self::DidPfp { .. } => "DID Profile Picture",
            Self::Unknown => "Unknown",
        }
    }
}
