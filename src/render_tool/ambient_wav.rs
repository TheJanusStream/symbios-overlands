//! `--ambient-wav` (#1519): a world's ambient bed baked to a WAV file,
//! without a render.
//!
//! A world's sound is a recipe like the rest of it - the room's
//! `ambient_audio`, a synthesised patch or a multi-track sequence - and until
//! this mode it could only be heard by standing in the world. Written to a
//! file it can be listened to by someone who is not there, have its levels
//! read by an agent that cannot hear, and be cut under a video of the world.
//!
//! The bytes are the game's own: [`crate::loading::ambient_bake_job`] makes
//! the job the loading gate dispatches, and [`gen_jobs::GenJob::run`] bakes
//! it as the native offload does - mono 16-bit PCM, written unchanged. A
//! baked sequence does not loop from its first sample (bevy_symbios_audio's
//! `looping` module, #1341): the game plays every pass from the recipe's
//! loop start to the end of the bake, so the one-line summary names that
//! point (`loop_start_s`) and the whole bake is kept, run-up and all. What a
//! visitor hears is the file from `loop_start_s` on, repeated.

use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::pds::{RoomRecord, SovereignAudioConfig};

/// What a bake wrote: the bed's kind, the WAV's rate and length, and where
/// the loop the game plays starts in it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct BakedBed {
    pub(super) kind: &'static str,
    pub(super) sample_rate: u32,
    pub(super) frames: usize,
    pub(super) loop_start: Duration,
}

impl BakedBed {
    fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate.max(1))
    }
}

/// `--ambient-wav PATH`: bake `record`'s ambient bed into `path` and print
/// the summary, or say why there is nothing to bake and exit 2.
pub(super) fn print_ambient_wav(record: &RoomRecord, path: &str) {
    match write_bed(&record.environment.ambient_audio, Path::new(path)) {
        Ok(bed) => println!(
            "{}",
            json!({
                "wav": path,
                "kind": bed.kind,
                "sample_rate": bed.sample_rate,
                "seconds": bed.seconds(),
                "loop_start_s": bed.loop_start.as_secs_f64(),
                "loop_seconds": bed.seconds() - bed.loop_start.as_secs_f64(),
            })
        ),
        Err(why) => {
            eprintln!("--ambient-wav: {why}");
            std::process::exit(2);
        }
    }
}

/// Bake `audio` as the world bakes it and write the WAV to `path`. Nothing
/// is written when the bed is not a recipe.
pub(super) fn write_bed(audio: &SovereignAudioConfig, path: &Path) -> Result<BakedBed, String> {
    let (wav, bed) = bake_bed(audio)?;
    std::fs::write(path, wav).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(bed)
}

/// The game's bake of `audio` - its WAV bytes - and what they hold.
fn bake_bed(audio: &SovereignAudioConfig) -> Result<(Vec<u8>, BakedBed), String> {
    let kind = audio.label();
    let job = crate::loading::ambient_bake_job(audio).ok_or_else(|| {
        format!("the ambient bed is {kind}: only a Patch or a Sequence is a recipe to bake")
    })?;
    let gen_jobs::GenResult::Audio(wav) = gen_jobs::GenJob::AudioBake(job).run() else {
        return Err(format!("the {kind} bake returned no audio"));
    };
    let (sample_rate, frames) = pcm_rate_and_frames(&wav)
        .ok_or_else(|| format!("the {kind} bake is not a PCM WAV this tool can read"))?;
    let loop_start =
        crate::world_builder::spatial_audio::baked_loop_start(audio).unwrap_or(Duration::ZERO);
    let bed = BakedBed {
        kind,
        sample_rate,
        frames,
        loop_start,
    };
    Ok((wav, bed))
}

/// A RIFF/WAVE file's sample rate and its frame count (samples per channel),
/// read from its `fmt ` and `data` chunks.
fn pcm_rate_and_frames(wav: &[u8]) -> Option<(u32, usize)> {
    if wav.get(..4)? != b"RIFF" || wav.get(8..12)? != b"WAVE" {
        return None;
    }
    let (mut rate, mut block_align) = (None, None);
    let mut at = 12;
    while let Some(head) = wav.get(at..at + 8) {
        let size = u32::from_le_bytes(head[4..8].try_into().ok()?) as usize;
        match &head[..4] {
            b"fmt " => {
                let body = wav.get(at + 8..at + 8 + size)?;
                rate = Some(u32::from_le_bytes(body.get(4..8)?.try_into().ok()?));
                block_align = Some(u16::from_le_bytes(body.get(12..14)?.try_into().ok()?));
            }
            b"data" => return Some((rate?, size / usize::from(block_align?.max(1)))),
            _ => {}
        }
        // Chunks are word-aligned: an odd-sized chunk carries a pad byte.
        at += 8 + size + size % 2;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::SovereignAssetReference;
    use crate::pds::audio::SovereignAudioPatch;

    /// Four beats at 120 BPM, looping from beat 1: a 2 s bake whose loop
    /// starts 0.5 s in. No instruments, so it bakes silence - cheap on CI's
    /// unoptimised build, and the loop point is all it has to carry.
    fn short_looped_sequence() -> SovereignAudioConfig {
        SovereignAudioConfig::from_sequence(&bevy_symbios_audio::SequenceRecipe {
            bpm: 120.0,
            sample_rate: 22_050,
            duration_beats: 4.0,
            loop_start_beats: Some(1.0),
            ..Default::default()
        })
    }

    fn temp_wav(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ambient-wav-{name}-{}.wav", std::process::id()))
    }

    #[test]
    fn a_sequence_bed_is_written_as_the_game_bakes_it() {
        let bed = short_looped_sequence();
        let path = temp_wav("sequence");
        let written = write_bed(&bed, &path).expect("a sequence is a recipe");
        let file = std::fs::read(&path).expect("the WAV was written");
        std::fs::remove_file(&path).ok();

        let job = crate::loading::ambient_bake_job(&bed).expect("the game bakes it");
        let gen_jobs::GenResult::Audio(game) = gen_jobs::GenJob::AudioBake(job).run() else {
            panic!("the game's bake returned no audio");
        };
        assert_eq!(file, game, "the file is the game's bake, byte for byte");
        assert_eq!(written.kind, "Sequence");
        assert_eq!(written.sample_rate, 22_050);
        assert_eq!(written.frames, 44_100, "4 beats at 120 BPM is 2 s");
    }

    #[test]
    fn a_sequence_bed_names_the_loop_start_the_game_plays_from() {
        let (_, bed) = bake_bed(&short_looped_sequence()).expect("a sequence is a recipe");
        assert_eq!(
            bed.loop_start,
            Duration::from_millis(500),
            "beat 1 at 120 BPM"
        );
    }

    #[test]
    fn a_patch_bed_loops_from_its_first_sample() {
        let patch = SovereignAudioConfig::Patch {
            patch: SovereignAudioPatch::default(),
        };
        let (_, bed) = bake_bed(&patch).expect("a patch is a recipe");
        assert_eq!(bed.kind, "Patch");
        assert_eq!(bed.loop_start, Duration::ZERO);
        assert_eq!(bed.sample_rate, crate::loading::AMBIENT_PATCH_SAMPLE_RATE);
        let expected = (f64::from(crate::loading::AMBIENT_PATCH_SECS) * f64::from(bed.sample_rate))
            .round() as usize;
        assert_eq!(bed.frames, expected, "the bed is AMBIENT_PATCH_SECS long");
    }

    #[test]
    fn a_silent_bed_writes_nothing_and_says_so() {
        let path = temp_wav("silent");
        let why = write_bed(&SovereignAudioConfig::None, &path).expect_err("None is no recipe");
        assert!(why.contains("None"), "{why}");
        assert!(!path.exists(), "nothing is written for a silent bed");
    }

    #[test]
    fn a_referenced_clip_is_not_baked() {
        let clip = SovereignAudioConfig::Referenced {
            source: SovereignAssetReference::default(),
        };
        let why = bake_bed(&clip).expect_err("a referenced clip is fetched, not baked");
        assert!(why.contains("Referenced"), "{why}");
    }

    #[test]
    fn the_header_reader_skips_chunks_it_does_not_know() {
        let mut wav = bevy_symbios_audio::samples_to_wav_bytes_pcm16(&[0.0; 10], 8_000);
        // A 3-byte LIST chunk (padded to 4) before `fmt `, where other
        // writers put metadata.
        let list = [b"LIST".as_slice(), &3u32.to_le_bytes(), &[1, 2, 3, 0]].concat();
        wav.splice(12..12, list);
        assert_eq!(pcm_rate_and_frames(&wav), Some((8_000, 10)));
        assert_eq!(pcm_rate_and_frames(b"RIFF\0\0\0\0WAVX"), None);
    }
}
