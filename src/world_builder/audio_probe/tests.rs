use super::*;

use bevy::audio::Decodable;

/// What a host serves where a sound was promised: a web page.
const WEB_PAGE: &[u8] = b"<!doctype html><html><body>Not found</body></html>";

/// A minimal PCM WAV, 16-bit mono at `rate` samples a second, holding
/// `samples` samples - its byte rate kept consistent with `rate`, as hound
/// checks.
fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// The hazard itself (#1560): Bevy unwraps the decoder of the clip a voice
/// plays, so bytes that are not audio panic the client. If a Bevy upgrade
/// stops panicking here, the probe may be worth revisiting.
#[test]
#[should_panic]
fn bevy_panics_playing_bytes_that_are_not_audio() {
    let clip = AudioSource {
        bytes: WEB_PAGE.into(),
    };
    let _ = clip.decoder();
}

/// The second hazard: rodio's own decoder divides by a WAV's sample rate
/// while it is built, so a probe that only built it would crash on the very
/// file it was checking.
#[test]
#[should_panic]
fn rodio_panics_building_a_wav_with_no_sample_rate() {
    let bytes = wav(0, &[0, 0]);
    let _ = rodio::Decoder::builder()
        .with_byte_len(bytes.len() as u64)
        .with_data(Cursor::new(bytes))
        .build();
}

#[test]
fn bytes_that_are_not_audio_make_no_clip() {
    let mut truncated_header = wav(22_050, &[0, 0]);
    truncated_header.truncate(30);
    for (what, bytes) in [
        ("a web page", WEB_PAGE.to_vec()),
        ("nothing", Vec::new()),
        (
            "noise",
            (0..4096u32)
                .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
                .collect(),
        ),
        ("a WAV cut off in its header", truncated_header),
        ("an Ogg page with no Vorbis in it", b"OggS\0\x02".repeat(40)),
    ] {
        assert_eq!(
            playable_clip(bytes).err(),
            Some(AssetFetchError::Undecodable),
            "{what} must be refused"
        );
    }
}

#[test]
fn a_wav_with_no_sample_rate_makes_no_clip() {
    assert_eq!(
        playable_clip(wav(0, &[0, 0])).err(),
        Some(AssetFetchError::Undecodable)
    );
}

/// The control: a real clip passes untouched, and plays.
#[test]
fn a_wav_makes_its_clip() {
    let bytes = wav(22_050, &[0, 1000, -1000, 0]);
    let clip = playable_clip(bytes.clone()).expect("a WAV is a clip");
    assert_eq!(&clip.bytes[..], &bytes[..]);
    assert_eq!(clip.decoder().count(), 4, "it decodes every sample");
}

/// A WAV whose data stops short of what its header promises is still a clip
/// (it only plays short), and playing it to the end never panics.
#[test]
fn a_wav_cut_off_in_its_data_plays_short() {
    let mut bytes = wav(22_050, &[0; 64]);
    bytes.truncate(bytes.len() - 100);
    let clip = playable_clip(bytes).expect("a short WAV is still a clip");
    assert!(clip.decoder().count() < 64);
}
