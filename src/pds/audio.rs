//! Sovereign mirror of the [`bevy_symbios_audio`] crate's authoring types
//! for use in DAG-CBOR-encoded ATProto records.
//!
//! # Why JSON-as-string for the procedural variants
//!
//! The audio crate's `AudioPatch` and `SequenceRecipe` are pure-`f32`
//! types — fine for in-memory baking but incompatible with DAG-CBOR,
//! which forbids floats. Mirroring all 13 [`NodeKind`] variants and
//! every [`SequenceRecipe`] field with the project's `Fp` / `Fp64`
//! fixed-point wrappers is a substantial undertaking (see follow-up
//! issue #311). To unblock the rest of milestone #2 today, the
//! procedural variants ([`SovereignAudioConfig::Patch`] and
//! [`SovereignAudioConfig::Sequence`]) instead carry the patch /
//! recipe as a serde-JSON-encoded `String`. The DAG-CBOR encoder sees
//! a single opaque string with no floats, so the record round-trips
//! through the PDS unchanged; baking-time consumers re-parse the
//! string via `serde_json::from_str` into the native types.
//!
//! The trade-off is that an in-editor structured node editor isn't
//! possible against the string blob — the bridge UI exposes a
//! multi-line text area for now, mirroring how the L-system editor
//! treats its `source_code` field. Structured editing lands with
//! #311's proper Sovereign* mirrors.
//!
//! [`bevy_symbios_audio`]: bevy_symbios_audio
//! [`NodeKind`]: bevy_symbios_audio::NodeKind
//! [`SequenceRecipe`]: bevy_symbios_audio::SequenceRecipe

use serde::{Deserialize, Serialize};

use super::asset_reference::SovereignAssetReference;

/// Open-union enum describing where audio data for a slot comes from.
/// Mirrors the structural shape of [`SovereignTextureConfig`] so the
/// editor bridges behave identically across asset classes: one
/// "Referenced" variant for explicit URL / DID-pinned blobs, one or
/// more procedural variants, and forward-compat [`Self::Unknown`].
///
/// The procedural variants embed the audio crate's authoring JSON as a
/// `String`. The bake-time consumer re-parses via
/// `serde_json::from_str::<bevy_symbios_audio::AudioPatch>` (or the
/// sequence equivalent) and hands the result to the crate's bake
/// pipeline.
///
/// [`SovereignTextureConfig`]: crate::pds::texture::SovereignTextureConfig
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum SovereignAudioConfig {
    /// No audio for this slot.
    None,
    /// External asset pointer — fetched bytes are decoded by the
    /// resolver into a `Handle<AudioSource>`.
    Referenced { source: SovereignAssetReference },
    /// Procedural single-voice patch — the audio crate's
    /// [`bevy_symbios_audio::AudioPatch`] serialised to JSON. Re-parse
    /// at consumption time. See module-level docstring for the
    /// DAG-CBOR rationale.
    Patch { patch_json: String },
    /// Procedural multi-voice mixdown — the audio crate's
    /// [`bevy_symbios_audio::SequenceRecipe`] serialised to JSON.
    Sequence { recipe_json: String },
    /// Forward-compat seam — a record from a future engine version
    /// decodes here rather than failing the whole load.
    #[serde(other)]
    Unknown,
}

impl Default for SovereignAudioConfig {
    fn default() -> Self {
        SovereignAudioConfig::None
    }
}

impl SovereignAudioConfig {
    /// Human-readable variant name for UI combo boxes — the strings
    /// match the dropdown labels in `draw_audio_bridge`.
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Referenced { .. } => "Referenced",
            Self::Patch { .. } => "Patch",
            Self::Sequence { .. } => "Sequence",
            Self::Unknown => "Unknown",
        }
    }

    /// Construct a `Patch` variant from a native
    /// [`bevy_symbios_audio::AudioPatch`]. Returns the underlying
    /// `serde_json` error if the patch can't be serialised — should
    /// not happen for any patch the audio crate itself produces, but
    /// defensible.
    pub fn from_patch(patch: &bevy_symbios_audio::AudioPatch) -> Result<Self, serde_json::Error> {
        Ok(SovereignAudioConfig::Patch {
            patch_json: serde_json::to_string(patch)?,
        })
    }

    /// Construct a `Sequence` variant from a native
    /// [`bevy_symbios_audio::SequenceRecipe`].
    pub fn from_sequence(
        recipe: &bevy_symbios_audio::SequenceRecipe,
    ) -> Result<Self, serde_json::Error> {
        Ok(SovereignAudioConfig::Sequence {
            recipe_json: serde_json::to_string(recipe)?,
        })
    }

    /// If this is a `Patch` variant, decode the embedded JSON back to
    /// the native type. Returns `None` for every other variant;
    /// `Some(Err(_))` if the embedded string is malformed JSON (the
    /// bake consumer should fall back to silence in that case).
    pub fn parse_patch(&self) -> Option<Result<bevy_symbios_audio::AudioPatch, serde_json::Error>> {
        match self {
            SovereignAudioConfig::Patch { patch_json } => Some(serde_json::from_str(patch_json)),
            _ => None,
        }
    }

    /// If this is a `Sequence` variant, decode the embedded JSON back
    /// to the native [`bevy_symbios_audio::SequenceRecipe`].
    pub fn parse_sequence(
        &self,
    ) -> Option<Result<bevy_symbios_audio::SequenceRecipe, serde_json::Error>> {
        match self {
            SovereignAudioConfig::Sequence { recipe_json } => {
                Some(serde_json::from_str(recipe_json))
            }
            _ => None,
        }
    }
}
