//! Biome punctuation — the natural *signature voice* of a biome (bird
//! chirps in lush valleys, wave washes on coasts, sub booms over
//! volcanic rock, whistle gusts in deserts, ice tings on tundra, a
//! distant howl in the alps). This is biome sound, not theme music, so
//! it sits in the biome layer alongside the bed/gust texture and plays
//! under whatever theme music a room carries. [`build`] is the entry the
//! orchestrator ([`super`]) calls; it shares the bed's [`AmbientParams`]
//! acoustic space.

use std::collections::BTreeMap;

use bevy_symbios_audio::{
    AdsrCurve, AdsrEnvelope, AudioPatch, BiquadBandpass, Connection, Event, Gain, Gate, GraphNode,
    Instrument, Lfo, LfoShape, NodeGraph, NodeId, NodeKind, PitchMode, Reverb, SineOsc, Track,
    WhiteNoise,
};
use rand_chacha::ChaCha8Rng;

use super::bed::AmbientParams;
use super::{LOOP_BEATS, WARMUP_BEATS};
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32};

/// Stable id for the biome punctuation voice.
pub(super) const PUNCT_INSTRUMENT_ID: &str = "punct_voice";

// ---------------------------------------------------------------------------
// Punctuation layer — the biome's signature voice
// ---------------------------------------------------------------------------

/// What kind of punctuation the biome speaks. This is the layer that
/// makes two regions *sound* like different places: the bed and gust
/// layers differ by knob values, but the punctuation differs in kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PunctuationMood {
    /// Lush: bright chirp blips with a fast downward pitch flick.
    BirdChirps,
    /// Coastal: slow band-passed noise swells on a surf rhythm.
    WaveWash,
    /// Volcanic: sparse sub-bass booms with a pitch drop.
    SubBoom,
    /// Arid: narrow high-Q whistle swells.
    WhistleGust,
    /// Tundra / Glacial: glassy high tings with a snap attack and long ring.
    IceTing,
    /// Alpine / Boreal: a quiet distant howl with slow vibrato.
    DistantHowl,
    /// Wetland: a low pulsed chorus of frog croaks.
    FrogChorus,
    /// Meadow: a sustained high band of cricket / cicada song.
    InsectChorus,
}

/// Seeded punctuation parameters — see [`derive_punctuation`].
struct PunctuationParams {
    mood: PunctuationMood,
    /// Voice base pitch (Hz) for tonal moods, bandpass centre for the
    /// noise moods.
    base_hz: f32,
    /// Events per loop.
    event_count: u32,
    /// Per-event volume band.
    volume: (f32, f32),
    /// Per-event gate length band (beats).
    gate: (f32, f32),
    /// Event release tail (beats) — covers the mood's ADSR release.
    release_beats: f32,
}

fn derive_punctuation(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> PunctuationParams {
    use BiomeArchetype::*;
    match scene.biome {
        // Songbirds in the verdant broadleaf biomes; the jungle's canopy
        // chatters densest of all.
        Lush | TemperateForest | Jungle => PunctuationParams {
            mood: PunctuationMood::BirdChirps,
            base_hz: range_f32(rng, 1_800.0, 2_600.0),
            event_count: 4 + (range_f32(rng, 0.0, 4.0) as u32),
            volume: (0.08, 0.15),
            gate: (0.08, 0.18),
            release_beats: 0.3,
        },
        Coastal => PunctuationParams {
            mood: PunctuationMood::WaveWash,
            base_hz: range_f32(rng, 500.0, 900.0),
            event_count: 2 + (range_f32(rng, 0.0, 2.0) as u32),
            volume: (0.28, 0.42),
            gate: (2.0, 3.0),
            release_beats: 2.0,
        },
        Volcanic => PunctuationParams {
            mood: PunctuationMood::SubBoom,
            base_hz: range_f32(rng, 45.0, 65.0),
            event_count: 1 + (range_f32(rng, 0.0, 2.0) as u32),
            volume: (0.5, 0.7),
            gate: (0.3, 0.5),
            release_beats: 2.0,
        },
        // Dry wind whistling over open ground / through canyons.
        Arid | Savanna | Badlands => PunctuationParams {
            mood: PunctuationMood::WhistleGust,
            base_hz: range_f32(rng, 1_200.0, 2_200.0),
            event_count: 2 + (range_f32(rng, 0.0, 2.0) as u32),
            volume: (0.14, 0.24),
            gate: (1.2, 2.0),
            release_beats: 1.5,
        },
        // Glassy tings: frost on tundra, cracking ice on a glacier.
        Tundra | Glacial => PunctuationParams {
            mood: PunctuationMood::IceTing,
            base_hz: range_f32(rng, 2_400.0, 3_800.0),
            event_count: 2 + (range_f32(rng, 0.0, 3.0) as u32),
            volume: (0.10, 0.18),
            gate: (0.05, 0.12),
            release_beats: 1.2,
        },
        // A lone distant howl carrying over the alps and the taiga.
        Alpine | Boreal => PunctuationParams {
            mood: PunctuationMood::DistantHowl,
            base_hz: range_f32(rng, 280.0, 380.0),
            event_count: 1,
            volume: (0.12, 0.2),
            gate: (3.0, 4.0),
            release_beats: 2.0,
        },
        // Low pulsed frog croaks over the standing water.
        Wetland => PunctuationParams {
            mood: PunctuationMood::FrogChorus,
            base_hz: range_f32(rng, 140.0, 260.0),
            event_count: 5 + (range_f32(rng, 0.0, 4.0) as u32),
            volume: (0.12, 0.20),
            gate: (0.12, 0.30),
            release_beats: 0.5,
        },
        // A sustained band of cricket / cicada song over the grass.
        Meadow => PunctuationParams {
            mood: PunctuationMood::InsectChorus,
            base_hz: range_f32(rng, 3_500.0, 5_500.0),
            event_count: 1 + (range_f32(rng, 0.0, 2.0) as u32),
            volume: (0.08, 0.16),
            gate: (3.0, 4.0),
            release_beats: 1.5,
        },
    }
}

/// Node ids inside the punctuation patch.
const PUNCT_GATE_ID: NodeId = NodeId(0);
const PUNCT_AMP_ADSR_ID: NodeId = NodeId(1);
const PUNCT_PITCH_ADSR_ID: NodeId = NodeId(2);
const PUNCT_SOURCE_ID: NodeId = NodeId(3);
const PUNCT_FILTER_ID: NodeId = NodeId(4);
const PUNCT_VCA_ID: NodeId = NodeId(5);
const PUNCT_REVERB_ID: NodeId = NodeId(6);
const PUNCT_VIBRATO_ID: NodeId = NodeId(7);

/// Build the per-mood voice. All moods share the same skeleton —
/// `Gate → amp ADSR → VCA → reverb` — and differ in the source
/// (sine vs filtered noise), the envelope speeds, and the modulators
/// (pitch-flick ADSR for chirps/booms, vibrato LFO for the howl).
fn build_punctuation_patch(
    punct: &PunctuationParams,
    params: &AmbientParams,
    seed: u64,
) -> AudioPatch {
    use PunctuationMood::*;

    let gate_node = GraphNode {
        id: PUNCT_GATE_ID,
        kind: NodeKind::Gate(Gate { invert: false }),
        inputs: BTreeMap::new(),
    };

    // Amplitude envelope per mood.
    let (attack_s, decay_s, sustain, release_s, curve) = match punct.mood {
        BirdChirps => (0.005, 0.06, 0.0, 0.05, AdsrCurve::Exponential),
        WaveWash => (1.2, 1.0, 0.4, 1.8, AdsrCurve::Linear),
        SubBoom => (0.01, 0.6, 0.0, 1.2, AdsrCurve::Exponential),
        WhistleGust => (0.8, 0.6, 0.3, 1.2, AdsrCurve::Linear),
        IceTing => (0.001, 0.08, 0.0, 1.0, AdsrCurve::Exponential),
        DistantHowl => (1.5, 0.5, 0.6, 1.9, AdsrCurve::Linear),
        // Quick croak body with a brief sustain.
        FrogChorus => (0.01, 0.12, 0.2, 0.25, AdsrCurve::Exponential),
        // Slow swell into a sustained drone.
        InsectChorus => (0.6, 0.4, 0.6, 1.0, AdsrCurve::Linear),
    };
    let mut amp_inputs = BTreeMap::new();
    amp_inputs.insert(
        "gate".to_string(),
        vec![Connection::from_node(PUNCT_GATE_ID)],
    );
    let amp_node = GraphNode {
        id: PUNCT_AMP_ADSR_ID,
        kind: NodeKind::Adsr(AdsrEnvelope {
            attack_s,
            decay_s,
            sustain_level: sustain,
            release_s,
            curve,
        }),
        inputs: amp_inputs,
    };

    let mut nodes = vec![gate_node, amp_node];

    // Source + optional modulators.
    let tonal = matches!(
        punct.mood,
        BirdChirps | SubBoom | IceTing | DistantHowl | FrogChorus
    );
    if tonal {
        let mut osc_inputs = BTreeMap::new();
        match punct.mood {
            BirdChirps | SubBoom => {
                // Pitch-flick envelope: chirps flick *down* from above,
                // booms drop into the sub.
                let mut pitch_inputs = BTreeMap::new();
                pitch_inputs.insert(
                    "gate".to_string(),
                    vec![Connection::from_node(PUNCT_GATE_ID)],
                );
                nodes.push(GraphNode {
                    id: PUNCT_PITCH_ADSR_ID,
                    kind: NodeKind::Adsr(AdsrEnvelope {
                        attack_s: 0.001,
                        decay_s: if punct.mood == BirdChirps { 0.08 } else { 0.35 },
                        sustain_level: 0.0,
                        release_s: 0.05,
                        curve: AdsrCurve::Linear,
                    }),
                    inputs: pitch_inputs,
                });
                let sweep = if punct.mood == BirdChirps {
                    punct.base_hz * 0.35
                } else {
                    punct.base_hz * 0.6
                };
                osc_inputs.insert(
                    "freq".to_string(),
                    vec![Connection::modulation(PUNCT_PITCH_ADSR_ID, sweep)],
                );
            }
            DistantHowl => {
                // Slow vibrato. Whole cycles per loop region keeps the
                // loop-sync invariant (64–96 cycles ⇒ 2–3 Hz over the
                // 32-beat loop).
                nodes.push(GraphNode {
                    id: PUNCT_VIBRATO_ID,
                    kind: NodeKind::Lfo(Lfo {
                        rate_hz: (64 + (seed % 33) as u32) as f32 / LOOP_BEATS,
                        shape: LfoShape::Sine,
                        depth: 1.0,
                        offset: 0.0,
                    }),
                    inputs: BTreeMap::new(),
                });
                osc_inputs.insert(
                    "freq".to_string(),
                    vec![Connection::modulation(PUNCT_VIBRATO_ID, 9.0)],
                );
            }
            _ => {}
        }
        nodes.push(GraphNode {
            id: PUNCT_SOURCE_ID,
            kind: NodeKind::Sine(SineOsc {
                freq_hz: punct.base_hz,
                phase_offset: 0.0,
                amplitude: 1.0,
            }),
            inputs: osc_inputs,
        });
    } else {
        // Noise moods: white noise through a mood-tuned bandpass.
        nodes.push(GraphNode {
            id: PUNCT_SOURCE_ID,
            kind: NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.9 }),
            inputs: BTreeMap::new(),
        });
        let q = match punct.mood {
            WaveWash => 0.8,
            InsectChorus => 8.0, // tight cicada whine
            _ => 5.0,            // WhistleGust: narrow singing band.
        };
        let mut filter_inputs = BTreeMap::new();
        filter_inputs.insert(
            "in".to_string(),
            vec![Connection::from_node(PUNCT_SOURCE_ID)],
        );
        nodes.push(GraphNode {
            id: PUNCT_FILTER_ID,
            kind: NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: punct.base_hz,
                q,
            }),
            inputs: filter_inputs,
        });
    }

    let voice_out = if tonal {
        PUNCT_SOURCE_ID
    } else {
        PUNCT_FILTER_ID
    };
    let mut vca_inputs = BTreeMap::new();
    vca_inputs.insert("in".to_string(), vec![Connection::from_node(voice_out)]);
    vca_inputs.insert(
        "gain".to_string(),
        vec![Connection::from_node(PUNCT_AMP_ADSR_ID)],
    );
    nodes.push(GraphNode {
        id: PUNCT_VCA_ID,
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_inputs,
    });

    // Punctuation sits deeper in the room's space than the bed — howls
    // and booms especially live on their reverb tail.
    let wet = match punct.mood {
        SubBoom | DistantHowl | IceTing => 0.45,
        _ => params.reverb_mix,
    };
    let mut reverb_inputs = BTreeMap::new();
    reverb_inputs.insert("in".to_string(), vec![Connection::from_node(PUNCT_VCA_ID)]);
    nodes.push(GraphNode {
        id: PUNCT_REVERB_ID,
        kind: NodeKind::Reverb(Reverb {
            room_size: params.reverb_room_size,
            damping: params.reverb_damping,
            mix: wet,
        }),
        inputs: reverb_inputs,
    });

    AudioPatch {
        seed: (seed.rotate_left(40) & 0xFFFF_FFFF) as u32,
        graph: NodeGraph {
            nodes,
            output: PUNCT_REVERB_ID,
        },
    }
}

/// Scatter the punctuation events across the loop region with mild
/// per-event pitch variation so repeated chirps / tings don't machine-
/// gun the same note. Same half-beat quantise + sort discipline as the
/// chime track.
fn punctuation_track_events(punct: &PunctuationParams, rng: &mut ChaCha8Rng) -> Vec<Event> {
    let mut events = Vec::with_capacity(punct.event_count as usize);
    for _ in 0..punct.event_count {
        let time_beats = WARMUP_BEATS + (range_f32(rng, 0.0, 12.0) * 2.0).floor() * 0.5;
        let pitch = match punct.mood {
            PunctuationMood::BirdChirps => range_f32(rng, 0.85, 1.35),
            PunctuationMood::IceTing => range_f32(rng, 0.8, 1.6),
            PunctuationMood::SubBoom => range_f32(rng, 0.9, 1.1),
            PunctuationMood::FrogChorus => range_f32(rng, 0.8, 1.2),
            _ => 1.0,
        };
        events.push(Event {
            time_beats,
            instrument_id: PUNCT_INSTRUMENT_ID.to_string(),
            pitch_multiplier: pitch,
            volume: range_f32(rng, punct.volume.0, punct.volume.1),
            gate_beats: range_f32(rng, punct.gate.0, punct.gate.1),
            release_beats: punct.release_beats,
            pitch_mode: PitchMode::Varispeed,
        });
    }
    events.sort_by(|a, b| a.time_beats.total_cmp(&b.time_beats));
    events
}

/// Build the biome punctuation voice — one instrument + its scattered
/// event track. Shares the bed's reverb space via `params`.
pub(super) fn build(
    scene: &SceneCharacter,
    params: &AmbientParams,
    rng: &mut ChaCha8Rng,
    seed: u64,
) -> (Instrument, Track) {
    let punct = derive_punctuation(scene, rng);
    let patch = build_punctuation_patch(&punct, params, seed);
    let events = punctuation_track_events(&punct, rng);
    (
        Instrument {
            id: PUNCT_INSTRUMENT_ID.to_string(),
            patch,
        },
        Track { events },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::rand_core::SeedableRng;

    #[test]
    fn mood_matches_biome() {
        use BiomeArchetype::*;
        let cases = [
            (Lush, PunctuationMood::BirdChirps),
            (TemperateForest, PunctuationMood::BirdChirps),
            (Jungle, PunctuationMood::BirdChirps),
            (Coastal, PunctuationMood::WaveWash),
            (Volcanic, PunctuationMood::SubBoom),
            (Arid, PunctuationMood::WhistleGust),
            (Savanna, PunctuationMood::WhistleGust),
            (Badlands, PunctuationMood::WhistleGust),
            (Tundra, PunctuationMood::IceTing),
            (Glacial, PunctuationMood::IceTing),
            (Alpine, PunctuationMood::DistantHowl),
            (Boreal, PunctuationMood::DistantHowl),
            (Wetland, PunctuationMood::FrogChorus),
            (Meadow, PunctuationMood::InsectChorus),
        ];
        for (biome, mood) in cases {
            let mut sc = SceneCharacter::for_seed(1);
            sc.biome = biome;
            let mut rng = ChaCha8Rng::seed_from_u64(1);
            assert_eq!(derive_punctuation(&sc, &mut rng).mood, mood);
        }
    }
}
