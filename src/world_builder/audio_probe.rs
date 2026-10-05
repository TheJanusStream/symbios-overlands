//! The one door fetched audio bytes pass through before any voice can play
//! them (#1560). Bevy decodes a clip with rodio when a voice first plays it,
//! and unwraps the decoder there (bevy_audio 0.19 `AudioSource::decoder`), so
//! bytes that are not audio in a format this build decodes - a web page
//! served where a sound was promised, a hostile room's reference, a hostile
//! peer's avatar part - crashed the client of everyone who came near the
//! voice. A clip is built here, by that same decoder, before it reaches
//! [`Assets<AudioSource>`](bevy::asset::Assets); bytes it cannot be built
//! from are an [`AssetFetchError::Undecodable`] fetch, which the caches
//! report and remember like any other failure, and the voice stays silent.
//!
//! The decoder must not be built on bytes that would bring the client down
//! while it is built, either. A WAV header whose sample rate is zero passes
//! hound's checks (with a byte rate of zero to match), and rodio divides by
//! the rate: hound reads the header first. And lewton, the Vorbis decoder,
//! sizes allocations from Ogg Vorbis header fields before it checks them -
//! a header a few dozen bytes long can ask for terabytes - so [`vorbis`]
//! walks those headers first without allocating.

use std::io::Cursor;

use bevy::audio::AudioSource;

use super::asset_failure::AssetFetchError;

/// The clip `bytes` make, or [`AssetFetchError::Undecodable`] where a voice
/// could not play them without panicking.
pub fn playable_clip(bytes: Vec<u8>) -> Result<AudioSource, AssetFetchError> {
    let clip = AudioSource {
        bytes: bytes.into(),
    };
    if zero_rate_wav(&clip.bytes) || !vorbis::headers_bounded(&clip.bytes) {
        return Err(AssetFetchError::Undecodable);
    }
    // Exactly the decoder Bevy builds - and unwraps - to play a clip.
    rodio::Decoder::builder()
        .with_byte_len(clip.bytes.len() as u64)
        .with_data(Cursor::new(clip.clone()))
        .build()
        .map(|_| clip)
        .map_err(|_| AssetFetchError::Undecodable)
}

/// Whether `bytes` are a WAV file hound reads whose sample rate is zero,
/// which rodio's decoder would divide by.
fn zero_rate_wav(bytes: &[u8]) -> bool {
    hound::WavReader::new(Cursor::new(bytes)).is_ok_and(|wav| wav.spec().sample_rate == 0)
}

mod vorbis;

#[cfg(test)]
mod tests;
