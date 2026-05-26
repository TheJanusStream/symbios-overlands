//! Sanitiser for [`SovereignAudioConfig`]. The Referenced variant
//! forwards to the asset-reference sanitiser (URL / DID / CID length
//! caps); the procedural JSON-stash variants are length-capped at
//! [`limits::MAX_AUDIO_PATCH_JSON_BYTES`] to defuse a hostile peer
//! shipping an inert megabyte of string through a room recipe.

use super::Sanitize;
use super::limits;
use crate::pds::audio::SovereignAudioConfig;
use crate::pds::types::truncate_on_char_boundary;

impl Sanitize for SovereignAudioConfig {
    fn sanitize(&mut self) {
        match self {
            SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
            SovereignAudioConfig::Referenced { source } => source.sanitize(),
            SovereignAudioConfig::Patch { patch_json } => {
                truncate_on_char_boundary(patch_json, limits::MAX_AUDIO_PATCH_JSON_BYTES);
            }
            SovereignAudioConfig::Sequence { recipe_json } => {
                truncate_on_char_boundary(recipe_json, limits::MAX_AUDIO_PATCH_JSON_BYTES);
            }
        }
    }
}
