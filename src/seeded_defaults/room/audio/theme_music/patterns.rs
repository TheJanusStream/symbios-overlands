//! Note-pattern generation: onset scattering (arp phrases / sparse
//! scatter), the melodic contour tables, and the bass voice derived from
//! the melody — everything that turns a [`ThemeVoice`] into `Event`s.

use bevy_symbios_audio::{Event, Instrument, PitchMode, Track};
use rand_chacha::ChaCha8Rng;

use super::super::bed::AmbientParams;
use super::super::{LOOP_BEATS, WARMUP_BEATS};
use super::patch::build_patch;
use super::voices::{ThemeVoice, Wave, biome_register, voice_for};
use crate::seeded_defaults::scene::{SceneCharacter, range_f32, unit_f32};

const ONSET_TAIL_BEATS: f32 = 4.0;
/// Hard ceiling on note events per voice — keeps bake time + the mixdown
/// bounded however the phrase/scatter maths lands on a long loop.
const MAX_NOTES: usize = 40;

/// One melodic event at `t` on scale `deg`/`octave`, with the voice's
/// per-note volume / gate / release sampled from its bands.
pub(super) fn note(
    voice: &ThemeVoice,
    rng: &mut ChaCha8Rng,
    t: f32,
    deg: usize,
    octave: f32,
) -> Event {
    Event {
        time_beats: t,
        instrument_id: voice.id.to_string(),
        pitch_multiplier: voice.scale[deg] * octave,
        volume: range_f32(rng, voice.volume.0, voice.volume.1),
        gate_beats: range_f32(rng, voice.gate.0, voice.gate.1),
        release_beats: voice.release_s,
        pitch_mode: PitchMode::Varispeed,
    }
}

pub(super) fn build_events(voice: &ThemeVoice, rng: &mut ChaCha8Rng) -> Vec<Event> {
    let n = voice.scale.len();
    // Beats available for onsets after the warm-up run-up.
    let span = (LOOP_BEATS - ONSET_TAIL_BEATS).max(4.0);
    let mut events = if voice.arp {
        arp_phrases(voice, rng, span, n)
    } else {
        sparse_scatter(voice, rng, span, n)
    };
    events.sort_by(|a, b| a.time_beats.total_cmp(&b.time_beats));
    events
}

/// Arpeggio voices fill the loop with eighth-note scale runs, but every
/// phrase is transformed so a long loop reads as developing A/B/C phrases
/// rather than one tiled arpeggio: the melodic contour (ascending,
/// descending, arch, pendulum) is re-rolled per phrase, an occasional phrase
/// plays a shorter motif fragment, every third phrase jumps an octave, and
/// whole-phrase rests punctuate. All seeded, so two rooms of the same theme
/// also lay their phrases out differently.
pub(super) fn arp_phrases(
    voice: &ThemeVoice,
    rng: &mut ChaCha8Rng,
    span: f32,
    n: usize,
) -> Vec<Event> {
    const STEP: f32 = 0.5; // eighth notes at 60 BPM
    let end = WARMUP_BEATS + span;
    let mut events = Vec::new();
    let mut t = WARMUP_BEATS;
    let mut phrase = 0u32;
    loop {
        // Most phrases run the full scale; some play a shorter motif so the
        // rhythm isn't a metronomic n-note tile.
        let len = if phrase > 0 && unit_f32(rng) < 0.3 {
            (n / 2 + 1).clamp(3.min(n), n)
        } else {
            n
        };
        let phrase_beats = len as f32 * STEP;
        if t + phrase_beats > end || events.len() + len > MAX_NOTES {
            break;
        }
        // An occasional whole-phrase rest (never the first) breaks the tile.
        let rest_phrase = phrase > 0 && unit_f32(rng) < 0.18;
        if !rest_phrase {
            let octave = if phrase % 3 == 1 { 2.0 } else { 1.0 };
            let contour = (unit_f32(rng) * 4.0) as u32;
            for k in 0..len {
                let deg = contour_deg(contour, k, len, n);
                events.push(note(voice, rng, t + k as f32 * STEP, deg, octave));
            }
        }
        // A short rest between phrases keeps them from running solid.
        t += phrase_beats + range_f32(rng, 0.5, 1.5);
        phrase += 1;
    }
    events
}

/// Map note index `k` (`0..len`) to a scale degree (`0..n`) following one of
/// four melodic contours, so successive phrases trace different shapes.
pub(super) fn contour_deg(contour: u32, k: usize, len: usize, n: usize) -> usize {
    match contour % 4 {
        // Ascending run up the scale.
        0 => k % n,
        // Descending run back down.
        1 => (n - 1).saturating_sub(k % n),
        // Arch: climb to the phrase midpoint, then fall back toward the root.
        2 => {
            let mid = len / 2;
            let pos = if k <= mid { k } else { len.saturating_sub(k) };
            pos.min(n - 1)
        }
        // Pendulum: alternate low and high degrees.
        _ => {
            let h = (k / 2) % n;
            if k.is_multiple_of(2) {
                h
            } else {
                (n - 1).saturating_sub(h)
            }
        }
    }
}

/// Sparse voices place their onsets as a handful of *gestures* — short runs
/// of close notes tracing a small up/down contour from a seeded root degree,
/// separated by rests and spread across the loop. This reads as a breathing,
/// developing phrase (and lays out differently per seed) rather than a
/// uniform random sprinkle. The note budget still scales with loop length.
pub(super) fn sparse_scatter(
    voice: &ThemeVoice,
    rng: &mut ChaCha8Rng,
    span: f32,
    n: usize,
) -> Vec<Event> {
    let band = (voice.note_count.1 - voice.note_count.0 + 1) as f32;
    let base = voice.note_count.0 + (range_f32(rng, 0.0, band) as u32);
    let count = ((base as f32 * (LOOP_BEATS / 16.0)).round() as usize).min(MAX_NOTES);
    if count == 0 {
        return Vec::new();
    }
    // A few gestures, each owning a slice of the span. The final gesture mops
    // up whatever notes remain so the total stays exactly `count`.
    let gestures = (count / 2).clamp(2, 5);
    let slice = span / gestures as f32;
    let mut events = Vec::with_capacity(count);
    let mut remaining = count;
    for g in 0..gestures {
        let here = if g == gestures - 1 {
            remaining
        } else {
            (remaining / (gestures - g)).max(1)
        };
        // Gesture onset near the front of its slice; notes then step on a
        // quarter-ish grid, clamped inside the onset window.
        let start = WARMUP_BEATS + g as f32 * slice + range_f32(rng, 0.0, slice * 0.4);
        let dir: i32 = if unit_f32(rng) < 0.5 { 1 } else { -1 };
        let root = (range_f32(rng, 0.0, n as f32) as usize).min(n - 1);
        for j in 0..here {
            let t = (start + j as f32 * range_f32(rng, 0.5, 1.0)).min(WARMUP_BEATS + span);
            let deg = (root as i32 + dir * j as i32).rem_euclid(n as i32) as usize;
            let octave = if unit_f32(rng) < 0.2 { 2.0 } else { 1.0 };
            events.push(note(voice, rng, t, deg, octave));
        }
        remaining = remaining.saturating_sub(here);
        if remaining == 0 {
            break;
        }
    }
    events
}

/// Hz floor for the bass fundamental so the octave-below-the-melody drop
/// never pushes a low-drone theme in a register-lowering biome (Volcanic)
/// into sub-sonic rumble.
const BASS_FLOOR_HZ: f32 = 40.0;

/// The bass octave multiplier: one octave below the melody, but never
/// shallower than the deep `0.5` register the mid/high voices already sit
/// at. Low-drone themes (melody octave < 1.0) therefore drop to a real
/// octave below their drone instead of doubling it at unison; mid/high
/// themes keep the existing deep pad.
pub(super) fn bass_octave_for(melody_octave: f32) -> f32 {
    (melody_octave * 0.5).min(0.5)
}

/// Low drone / pad second voice — a sustained sine one octave below the
/// melody (see [`bass_octave_for`]) that fills the long loop's bottom end
/// and shares the bed's reverb. Its voicing varies by seed (a static held
/// drone, or a slow walk across 2–3 scale degrees), but it is always present
/// so every room reads layered. Returns the instrument + its track.
pub(crate) fn build_bass(
    scene: &SceneCharacter,
    params: &AmbientParams,
    rng: &mut ChaCha8Rng,
    seed: u64,
) -> (Instrument, Track) {
    let melody = voice_for(scene, seed);
    // Prosperity lifts the pad volume a touch, mirroring the melody.
    let wealth = (scene.prosperity.clamp(0.0, 1.0) - 0.5) * 2.0;
    let vol = (0.07 + 0.02 * wealth).clamp(0.04, 0.11);
    let bass = ThemeVoice {
        id: "theme_bass",
        wave: Wave::Sine,
        detune_cents: 0.0,
        scale: melody.scale,
        octave: bass_octave_for(melody.octave),
        attack_s: 0.8,
        decay_s: 0.6,
        sustain_level: 0.8,
        release_s: 1.5,
        note_count: (1, 3),
        gate: (0.0, 0.0),
        volume: (vol, vol),
        arp: false,
        reverb_mix: (melody.reverb_mix + 0.1).min(0.6),
    };
    // One octave below the melody for clean separation; the Hz floor keeps a
    // register-lowering biome from pushing the drop sub-sonic.
    let root_hz = (220.0
        * 2.0_f32.powf(scene.base_hue_deg / 360.0)
        * biome_register(scene.biome)
        * bass.octave)
        .max(BASS_FLOOR_HZ);
    let patch = build_patch(&bass, root_hz, params, seed ^ 0x0BA5_50DD);
    let events = build_bass_events(&bass, rng);
    (
        Instrument {
            id: bass.id.to_string(),
            patch,
        },
        Track { events },
    )
}

/// 1–3 long held notes spanning the loop. One note is a static drone; two
/// or three step slowly across low scale degrees, each held for its share of
/// the loop with a release tail into the seam. The walk *shape* is seeded
/// (I–V–I, I–♭VII–V, or I–III–VI-ish) so the bottom end isn't a fixed
/// figure across every room of a theme.
pub(super) fn build_bass_events(bass: &ThemeVoice, rng: &mut ChaCha8Rng) -> Vec<Event> {
    let n = bass.scale.len();
    let notes = (1 + (unit_f32(rng) * 3.0) as u32).clamp(1, 3); // 1..=3
    // Low scale degrees for a slow root movement; clamped to the scale.
    let walk: [usize; 3] = match (unit_f32(rng) * 3.0) as usize % 3 {
        0 => [0, (n / 2).min(n - 1), 0],
        1 => [0, n - 1, (n / 2).min(n - 1)],
        _ => [0, (n / 3).min(n - 1), (2 * n / 3).min(n - 1)],
    };
    let seg = LOOP_BEATS / notes as f32;
    let mut events = Vec::with_capacity(notes as usize);
    for k in 0..notes {
        let deg = walk[(k as usize) % walk.len()].min(n - 1);
        events.push(Event {
            time_beats: WARMUP_BEATS + k as f32 * seg,
            instrument_id: bass.id.to_string(),
            pitch_multiplier: bass.scale[deg],
            volume: bass.volume.0,
            // Hold each segment; the final note's release tails into the seam.
            gate_beats: seg,
            release_beats: bass.release_s,
            pitch_mode: PitchMode::Varispeed,
        });
    }
    events
}
