//! Seeded ambient-audio recipe deriver.
//!
//! Produces a deterministic [`bevy_symbios_audio::SequenceRecipe`] for a
//! room, varying by [`SceneCharacter`] so each player's homeworld has a
//! sonically distinct ambient bed — Lush biomes get a warm filtered
//! brown-noise hum, Tundra a high-pass white-noise wind, Volcanic a
//! deep rumble, and so on. The same `(scene, seed)` pair always yields
//! the same recipe, matching the determinism contract the rest of the
//! room derivers already honour.
//!
//! # Sound design
//!
//! The recipe is intentionally minimal in this first iteration: one
//! instrument carrying a single voice (noise → biquad LP → output)
//! with a slow LFO sweeping the cutoff. The wiring mirrors the audio
//! crate's "wind" acceptance example: a slowly-modulated filter on
//! pink/brown/white noise reads as a continuously-evolving ambient
//! bed.
//!
//! # Looping
//!
//! `loop_start_beats = Some(0)` and `loop_crossfade_beats = 2.0` make
//! the resulting buffer hard-loop cleanly under the audio crate's
//! tail-crossfade pre-mix (Phase 4 #15 of the audio plan). 16 beats
//! at 60 BPM is 16 seconds of ambient bed before the loop restarts.

use bevy_symbios_audio::{
    AudioPatch, BiquadLowpass, BrownNoise, Connection, Event, GraphNode, Instrument, Lfo, LfoShape,
    NodeGraph, NodeId, NodeKind, PinkNoise, PitchMode, Reverb, SequenceRecipe, Track, WhiteNoise,
};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use std::collections::BTreeMap;

use crate::seeded_defaults::scene::{BiomeArchetype, LandformArchetype, SceneCharacter, range_f32};

/// Sub-stream salt distinct from palette / terrain / textures / atmosphere.
const AUDIO_STREAM_SALT: u64 = 0xAD17_BEEF_C0DE_AC1D;

/// Stable instrument identifier used by the single ambient event.
const INSTRUMENT_ID: &str = "ambient_bed";

/// Node ids inside the ambient patch — kept as constants so the wiring
/// reads top-to-bottom rather than threading magic integers.
const NOISE_ID: NodeId = NodeId(0);
const LFO_ID: NodeId = NodeId(1);
const FILTER_ID: NodeId = NodeId(2);
const REVERB_ID: NodeId = NodeId(3);

/// Tunable parameters extracted before patch construction so each piece
/// of the wiring graph can be read in one place.
struct AmbientParams {
    /// Which noise colour the bed uses. Brown for low-rumble biomes,
    /// pink for warm/lush, white for arid/coastal hiss.
    noise_kind: NoiseKind,
    /// Base cutoff (Hz) of the lowpass.
    base_cutoff_hz: f32,
    /// LFO sweep depth (Hz) around the base cutoff.
    cutoff_sweep_hz: f32,
    /// LFO rate (Hz). Sub-1 Hz produces a slow ambient breathe.
    lfo_rate_hz: f32,
    /// Filter resonance. Low for a wide, soft bed; nearly 1.0 for a
    /// more whistling, tuned sweep.
    filter_q: f32,
    /// Noise amplitude. Quieter for sustained drones to avoid pumping
    /// the master into the soft clip.
    noise_amplitude: f32,
    /// Reverb room size in `[0, 1]` — larger biomes/landforms ring
    /// longer, placing the bed in a bigger acoustic space.
    reverb_room_size: f32,
    /// Reverb damping in `[0, 1]` — darker (more absorbed highs) for
    /// dense/muffled biomes, brighter for open ones.
    reverb_damping: f32,
    /// Reverb dry/wet mix in `[0, 1]` — kept modest so the bed stays
    /// readable rather than washing out.
    reverb_mix: f32,
}

#[derive(Clone, Copy)]
enum NoiseKind {
    White,
    Pink,
    Brown,
}

/// Top-level seeded recipe — the value the wiring layer hands to the
/// PDS record (and the loading-gate baker consumes in #297).
pub struct AmbientRecipe {
    pub recipe: SequenceRecipe,
}

impl AmbientRecipe {
    /// Derive a deterministic ambient recipe from the room's scene
    /// anchor and the room seed. Same inputs → same recipe.
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ AUDIO_STREAM_SALT);
        let params = derive_params(scene, &mut rng);
        let patch = build_patch(&params, room_seed);
        let recipe = SequenceRecipe {
            bpm: 60.0,
            sample_rate: 44_100,
            // 16 beats at 60 BPM = 16 seconds of bed before loop
            // restart. Long enough that the loop seam isn't obvious;
            // short enough that the first bake completes in seconds.
            duration_beats: 16.0,
            loop_start_beats: Some(0.0),
            loop_crossfade_beats: 2.0,
            instruments: vec![Instrument {
                id: INSTRUMENT_ID.to_string(),
                patch,
            }],
            tracks: vec![Track {
                events: vec![Event {
                    time_beats: 0.0,
                    instrument_id: INSTRUMENT_ID.to_string(),
                    pitch_multiplier: 1.0,
                    volume: 0.6,
                    // Hold the gate open for the full loop window so
                    // the noise + filter sustain rather than gating.
                    gate_beats: 16.0,
                    // No release tail — the bed loops on the gate window
                    // and the crossfade smooths the seam.
                    release_beats: 0.0,
                    // Ambient bed plays at native pitch; resample mode is
                    // irrelevant at pitch_multiplier 1.0, keep the default.
                    pitch_mode: PitchMode::Varispeed,
                }],
            }],
        };
        Self { recipe }
    }
}

fn derive_params(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> AmbientParams {
    // Noise colour follows biome character — low/dark for dense-mass
    // biomes, bright/hissy for sparse ones.
    let noise_kind = match scene.biome {
        BiomeArchetype::Lush => NoiseKind::Pink,
        BiomeArchetype::Volcanic => NoiseKind::Brown,
        BiomeArchetype::Alpine => NoiseKind::Brown,
        BiomeArchetype::Arid => NoiseKind::White,
        BiomeArchetype::Coastal => NoiseKind::Pink,
        BiomeArchetype::Tundra => NoiseKind::White,
    };
    // Landform sets the base cutoff envelope — open ranges for big
    // skies (Mesa/Rolling), tighter for valleys/archipelago.
    let (cutoff_lo, cutoff_hi) = match scene.landform {
        LandformArchetype::Rolling => (700.0, 1_200.0),
        LandformArchetype::Craggy => (500.0, 900.0),
        LandformArchetype::Mesa => (900.0, 1_500.0),
        LandformArchetype::Archipelago => (400.0, 800.0),
        LandformArchetype::Valleys => (500.0, 1_000.0),
    };
    let base_cutoff_hz = range_f32(rng, cutoff_lo, cutoff_hi);
    // Sweep depth is tied to base cutoff so the audible range scales
    // with the bed's pitch. 30-60% of the base keeps the sweep musical
    // rather than crashing through the audible bottom.
    let cutoff_sweep_hz = base_cutoff_hz * range_f32(rng, 0.3, 0.6);
    // 0.05 Hz - 0.3 Hz: one full cycle every 3-20 seconds, which is
    // the "ambient breathe" range every modular synth tutorial cites.
    let lfo_rate_hz = range_f32(rng, 0.05, 0.30);
    // Soft Q for a wide bed unless the biome pushes toward a more
    // dramatic sweep — Tundra and Volcanic both want a whistlier
    // feel from their landform's character.
    let filter_q = match scene.biome {
        BiomeArchetype::Tundra | BiomeArchetype::Volcanic => range_f32(rng, 1.2, 2.0),
        _ => range_f32(rng, 0.5, 1.0),
    };
    // Noise amplitude well below unity so the master never clips after
    // the volume scale in the Event (0.6) compounds. The reverb's wet
    // tail adds energy on top, so trim the dry noise a touch here.
    let noise_amplitude = range_f32(rng, 0.55, 0.8);
    // Reverb places the bed in an acoustic space. Bigger skies
    // (Mesa/Rolling) get a larger room; valleys/archipelago stay
    // tighter. Damping follows biome brightness — dark/dense biomes
    // absorb highs faster, open/arid ones keep them.
    let reverb_room_size = match scene.landform {
        LandformArchetype::Mesa => range_f32(rng, 0.7, 0.9),
        LandformArchetype::Rolling => range_f32(rng, 0.6, 0.8),
        LandformArchetype::Craggy | LandformArchetype::Valleys => range_f32(rng, 0.45, 0.65),
        LandformArchetype::Archipelago => range_f32(rng, 0.4, 0.6),
    };
    let reverb_damping = match scene.biome {
        BiomeArchetype::Volcanic | BiomeArchetype::Lush => range_f32(rng, 0.6, 0.85),
        BiomeArchetype::Arid | BiomeArchetype::Coastal => range_f32(rng, 0.2, 0.45),
        _ => range_f32(rng, 0.4, 0.6),
    };
    // Modest wet so the loop stays readable rather than washing out.
    let reverb_mix = range_f32(rng, 0.15, 0.3);
    AmbientParams {
        noise_kind,
        base_cutoff_hz,
        cutoff_sweep_hz,
        lfo_rate_hz,
        filter_q,
        noise_amplitude,
        reverb_room_size,
        reverb_damping,
        reverb_mix,
    }
}

fn build_patch(params: &AmbientParams, seed: u64) -> AudioPatch {
    let noise_node = GraphNode {
        id: NOISE_ID,
        kind: match params.noise_kind {
            NoiseKind::White => NodeKind::WhiteNoise(WhiteNoise {
                amplitude: params.noise_amplitude,
            }),
            NoiseKind::Pink => NodeKind::PinkNoise(PinkNoise {
                amplitude: params.noise_amplitude,
            }),
            NoiseKind::Brown => NodeKind::BrownNoise(BrownNoise {
                amplitude: params.noise_amplitude,
            }),
        },
        inputs: BTreeMap::new(),
    };

    let lfo_node = GraphNode {
        id: LFO_ID,
        kind: NodeKind::Lfo(Lfo {
            rate_hz: params.lfo_rate_hz,
            shape: LfoShape::Sine,
            // LFO output = sin(2π·phase) * depth + offset. Depth alone
            // produces a centred sweep around 0; the offset 0 lets the
            // filter's base cutoff carry the centre. The connection
            // `amount` (set below) controls the final injection scale.
            depth: 1.0,
            offset: 0.0,
        }),
        inputs: BTreeMap::new(),
    };

    let mut filter_inputs = BTreeMap::new();
    filter_inputs.insert("in".to_string(), vec![Connection::from_node(NOISE_ID)]);
    // LFO output is in [-1, 1] (depth=1, offset=0); the `modulation`
    // constructor scales it by `cutoff_sweep_hz` before delivery to the
    // filter's cutoff input, which is then added to the filter's base
    // cutoff inside its per-sample loop.
    filter_inputs.insert(
        "cutoff_hz".to_string(),
        vec![Connection::modulation(LFO_ID, params.cutoff_sweep_hz)],
    );
    let filter_node = GraphNode {
        id: FILTER_ID,
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: params.base_cutoff_hz,
            q: params.filter_q,
        }),
        inputs: filter_inputs,
    };

    // Reverb tail places the filtered bed in an acoustic space — the
    // graph output. Wired only on `in`; room size / damping / mix are
    // the seeded character.
    let mut reverb_inputs = BTreeMap::new();
    reverb_inputs.insert("in".to_string(), vec![Connection::from_node(FILTER_ID)]);
    let reverb_node = GraphNode {
        id: REVERB_ID,
        kind: NodeKind::Reverb(Reverb {
            room_size: params.reverb_room_size,
            damping: params.reverb_damping,
            mix: params.reverb_mix,
        }),
        inputs: reverb_inputs,
    };

    AudioPatch {
        // Truncate the room seed to 32 bits for the patch's stochastic
        // RNG. Two rooms with seeds that collide in the low 32 bits
        // still vary in their derived params (since AmbientParams used
        // the full 64-bit ChaCha stream above); the patch seed only
        // affects noise sequence ordering, which is musically
        // indistinguishable at ambient time scales.
        seed: (seed & 0xFFFF_FFFF) as u32,
        graph: NodeGraph {
            nodes: vec![noise_node, lfo_node, filter_node, reverb_node],
            output: REVERB_ID,
        },
    }
}
