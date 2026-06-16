//! Seeded ambient-audio recipe deriver.
//!
//! Produces a deterministic [`bevy_symbios_audio::SequenceRecipe`] for a
//! room, varying by [`SceneCharacter`] so each player's homeworld has a
//! sonically distinct ambient bed — Lush biomes get a warm low-passed
//! pink-noise hum, Tundra a high-passed airy wind, Volcanic a deep
//! brown-noise rumble, and so on. The same `(scene, seed)` pair always
//! yields the same recipe, matching the determinism contract the rest of
//! the room derivers already honour.
//!
//! # Sound design
//!
//! Four layers:
//!
//! 1. **Bed** — one sustained voice (noise → biquad → reverb) with a
//!    slow LFO sweeping the cutoff. Lowpass for the warm biomes;
//!    arid/tundra run highpass so their wind sits in a thin airy
//!    band instead of the universal low rumble.
//! 2. **Gusts** — band-passed noise riding a slow loop-synced sine
//!    VCA just above the bed's cutoff, so the wind breathes.
//! 3. **Chimes** — a sparse melodic voice (gate → ADSR → VCA'd
//!    sine/triangle → wet reverb) striking a handful of pentatonic
//!    notes per loop. The root pitch is anchored to the scene hue,
//!    the mode to its temperature (warm → major, cool → minor), and
//!    the register to the biome (volcanic tolls an octave down,
//!    tundra rings an octave up).
//! 4. **Punctuation** — the biome's signature voice
//!    ([`PunctuationMood`]): bird chirps in lush valleys, wave washes
//!    on coasts, sub-bass booms over volcanic rock, whistle gusts in
//!    deserts, ice tings on tundra, a distant howl in the alps. The
//!    other layers differ by knob values; this one differs in kind.
//!
//! # Looping
//!
//! The timeline is `WARMUP_BEATS` of run-up plus `LOOP_BEATS` of loop
//! region (`loop_start_beats = WARMUP_BEATS`). The run-up plays once
//! and exists so the filter / reverb states are hot by the time the
//! loop region begins — looping from beat 0 replayed the cold-start
//! fade-in on every pass. The sustained voices carry
//! `release_beats = loop_crossfade_beats` so the baker has real tail
//! material to crossfade into the loop start, and every LFO rate is
//! quantised to whole cycles per loop region so modulation phase is
//! continuous across the seam.

use bevy_symbios_audio::{
    AdsrCurve, AdsrEnvelope, AudioPatch, BiquadBandpass, BiquadHighpass, BiquadLowpass, BrownNoise,
    Connection, Event, Gain, Gate, GraphNode, Instrument, Lfo, LfoShape, NodeGraph, NodeId,
    NodeKind, PinkNoise, PitchMode, Reverb, SequenceRecipe, SineOsc, Track, TriangleOsc,
    WhiteNoise,
};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use std::collections::BTreeMap;

use crate::seeded_defaults::scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, range_f32, unit_f32,
};

/// Sub-stream salt distinct from palette / terrain / textures / atmosphere.
const AUDIO_STREAM_SALT: u64 = 0xAD17_BEEF_C0DE_AC1D;

/// Stable instrument identifier used by the single ambient event.
const INSTRUMENT_ID: &str = "ambient_bed";

/// Stable identifier for the sparse melodic chime layer.
const CHIME_INSTRUMENT_ID: &str = "chime_voice";

/// Stable identifier for the slow gust-swell layer.
const GUST_INSTRUMENT_ID: &str = "gust_swell";

/// Stable identifier for the biome punctuation voice (bird chirps,
/// wave washes, sub booms, …).
const PUNCT_INSTRUMENT_ID: &str = "punct_voice";

/// One-shot run-up before the loop region — long enough for the
/// lowpass and reverb states to reach steady level so the loop never
/// replays the cold-start fade-in.
const WARMUP_BEATS: f32 = 2.0;
/// Length of the looped region (= seconds at 60 BPM).
const LOOP_BEATS: f32 = 16.0;
/// Tail-crossfade window blending the timeline end into the loop
/// start.
const CROSSFADE_BEATS: f32 = 2.0;

/// Major-pentatonic just ratios — warm rooms chime in major.
const PENTATONIC_MAJOR: &[f32] = &[1.0, 1.125, 1.25, 1.5, 1.6667];
/// Minor-pentatonic just ratios — cool rooms chime in minor.
const PENTATONIC_MINOR: &[f32] = &[1.0, 1.2, 1.3333, 1.5, 1.8];

/// Node ids inside the ambient patch — kept as constants so the wiring
/// reads top-to-bottom rather than threading magic integers.
const NOISE_ID: NodeId = NodeId(0);
const LFO_ID: NodeId = NodeId(1);
const FILTER_ID: NodeId = NodeId(2);
const REVERB_ID: NodeId = NodeId(3);

/// Tunable parameters extracted before patch construction so each piece
/// of the wiring graph can be read in one place.
struct AmbientParams {
    /// Which noise colour the bed uses. Brown for the low-rumble biomes,
    /// pink for the rest. White noise is never a bed (only a band-passed
    /// punctuation accent) — even high-passed it reads as harsh hiss.
    noise_kind: NoiseKind,
    /// Which biquad shapes the bed. Lowpass is the classic warm wind;
    /// arid and tundra run *highpass* so their wind sits in a thin,
    /// airy band the ear immediately separates from the lush rumble.
    filter_kind: BedFilter,
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
    Pink,
    Brown,
}

#[derive(Clone, Copy, PartialEq)]
enum BedFilter {
    Lowpass,
    Highpass,
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
        let chime = derive_chime(scene, &mut rng);
        let chime_patch = build_chime_patch(&chime, &params, room_seed);
        let chime_events = chime_track_events(&chime, &mut rng);
        let gust = derive_gust(&params, &mut rng);
        let gust_patch = build_gust_patch(&gust, &params, room_seed);
        let punct = derive_punctuation(scene, &mut rng);
        let punct_patch = build_punctuation_patch(&punct, &params, room_seed);
        let punct_events = punctuation_track_events(&punct, &mut rng);

        let duration = WARMUP_BEATS + LOOP_BEATS;
        // Sustained voice covering the full timeline, with a release
        // tail past the end so the loop crossfade has real material to
        // blend into the loop start.
        let sustained = |id: &str, volume: f32| Event {
            time_beats: 0.0,
            instrument_id: id.to_string(),
            pitch_multiplier: 1.0,
            volume,
            gate_beats: duration,
            release_beats: CROSSFADE_BEATS,
            // Native pitch; resample mode is irrelevant at
            // pitch_multiplier 1.0.
            pitch_mode: PitchMode::Varispeed,
        };

        let recipe = SequenceRecipe {
            bpm: 60.0,
            sample_rate: 44_100,
            duration_beats: duration,
            // Loop region starts after the warm-up run-up; see the
            // module docs' Looping section.
            loop_start_beats: Some(WARMUP_BEATS),
            loop_crossfade_beats: CROSSFADE_BEATS,
            instruments: vec![
                Instrument {
                    id: INSTRUMENT_ID.to_string(),
                    patch,
                },
                Instrument {
                    id: GUST_INSTRUMENT_ID.to_string(),
                    patch: gust_patch,
                },
                Instrument {
                    id: CHIME_INSTRUMENT_ID.to_string(),
                    patch: chime_patch,
                },
                Instrument {
                    id: PUNCT_INSTRUMENT_ID.to_string(),
                    patch: punct_patch,
                },
            ],
            tracks: vec![
                Track {
                    // Bed event volume trimmed (was 0.6) so the bed +
                    // gust + chime + punctuation sum stays under the
                    // mixdown tanh knee — see noise_amplitude in
                    // derive_params for the saturation rationale.
                    events: vec![sustained(INSTRUMENT_ID, 0.5)],
                },
                Track {
                    events: vec![sustained(GUST_INSTRUMENT_ID, gust.volume)],
                },
                Track {
                    events: chime_events,
                },
                Track {
                    events: punct_events,
                },
            ],
        };
        Self { recipe }
    }
}

fn derive_params(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> AmbientParams {
    // Noise colour follows biome character — brown for the low/dark,
    // dense-mass biomes, pink for the breezier ones. White noise is
    // deliberately *not* used for any bed: even high-passed it reads as
    // harsh, over-saturated hiss rather than wind. (It survives only as a
    // sparse, tightly band-passed punctuation accent.) Arid/Tundra still
    // separate from the lush rumble through their high-pass bed filter.
    let noise_kind = match scene.biome {
        BiomeArchetype::Lush => NoiseKind::Pink,
        BiomeArchetype::Volcanic => NoiseKind::Brown,
        BiomeArchetype::Alpine => NoiseKind::Brown,
        BiomeArchetype::Arid => NoiseKind::Pink,
        BiomeArchetype::Coastal => NoiseKind::Pink,
        BiomeArchetype::Tundra => NoiseKind::Pink,
    };
    let filter_kind = match scene.biome {
        BiomeArchetype::Arid | BiomeArchetype::Tundra => BedFilter::Highpass,
        _ => BedFilter::Lowpass,
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
    // 1–4 whole cycles per loop region (0.0625–0.25 Hz at 60 BPM) —
    // the "ambient breathe" band, quantised so the sweep phase is
    // identical at the loop start and loop end and the seam never
    // jumps mid-sweep.
    let lfo_rate_hz = loop_synced_rate(rng, 1, 4);
    // Soft Q for a wide bed. Tundra/Volcanic lean a touch whistlier from
    // their landform character, but the peak is kept modest — a high
    // resonant Q on the high-pass bed sings a harsh tone rather than
    // breathing as wind.
    let filter_q = match scene.biome {
        BiomeArchetype::Tundra | BiomeArchetype::Volcanic => range_f32(rng, 0.8, 1.3),
        _ => range_f32(rng, 0.5, 1.0),
    };
    // Noise amplitude kept low. The four layers sum and pass through the
    // mixdown's tanh soft-clip; at the old 0.55–0.8 the bed alone pushed
    // the sum into saturation, which flattens the noise spectrum into the
    // harsh "over-saturated white noise" the wind used to read as. Trim it
    // (and the Event volume below) so the summed peak stays in tanh's
    // near-linear region and the bed reads as soft air, not clipped hiss.
    let noise_amplitude = range_f32(rng, 0.35, 0.55);
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
        filter_kind,
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
        kind: match params.filter_kind {
            BedFilter::Lowpass => NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: params.base_cutoff_hz,
                q: params.filter_q,
            }),
            BedFilter::Highpass => NodeKind::BiquadHighpass(BiquadHighpass {
                cutoff_hz: params.base_cutoff_hz,
                q: params.filter_q,
            }),
        },
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

/// Uniform pick of `lo..=hi` whole LFO cycles per loop region,
/// returned as a rate in Hz. Whole-cycle rates make the modulation
/// phase continuous across the loop seam.
fn loop_synced_rate(rng: &mut ChaCha8Rng, lo: u32, hi: u32) -> f32 {
    let cycles = lo + (range_f32(rng, 0.0, (hi - lo + 1) as f32) as u32).min(hi - lo);
    let loop_secs = LOOP_BEATS; // 60 BPM: one beat = one second.
    cycles as f32 / loop_secs
}

// ---------------------------------------------------------------------------
// Gust layer — slow band-limited swells over the steady bed
// ---------------------------------------------------------------------------

/// Seeded gust-swell parameters — see [`derive_gust`].
struct GustParams {
    /// Bandpass centre the gust whistles through.
    center_hz: f32,
    /// Bandpass resonance.
    q: f32,
    /// Swell rate (Hz) — whole cycles per loop region.
    swell_rate_hz: f32,
    /// Event volume for the gust voice.
    volume: f32,
}

/// Derive the gust layer relative to the bed: the gust band sits just
/// above the bed's lowpass cutoff so the swells read as a voice on
/// top rather than more of the same rumble. One or two swells per
/// loop keeps it weather, not tremolo.
fn derive_gust(params: &AmbientParams, rng: &mut ChaCha8Rng) -> GustParams {
    GustParams {
        center_hz: params.base_cutoff_hz * range_f32(rng, 1.1, 1.6),
        q: range_f32(rng, 1.4, 2.4),
        swell_rate_hz: loop_synced_rate(rng, 1, 2),
        // Trimmed (was 0.22–0.38) so the band-passed gust rides over the
        // softened bed without re-introducing the saturated-sum harshness.
        volume: range_f32(rng, 0.16, 0.28),
    }
}

/// Node ids inside the gust patch.
const GUST_NOISE_ID: NodeId = NodeId(0);
const GUST_LFO_ID: NodeId = NodeId(1);
const GUST_FILTER_ID: NodeId = NodeId(2);
const GUST_VCA_ID: NodeId = NodeId(3);
const GUST_REVERB_ID: NodeId = NodeId(4);

/// `noise → bandpass → VCA(gain ← slow sine LFO) → reverb`: the LFO
/// swells the band-limited noise from near-silence to full and back,
/// reading as wind gusts moving through the bed.
fn build_gust_patch(gust: &GustParams, params: &AmbientParams, seed: u64) -> AudioPatch {
    let noise_node = GraphNode {
        // Band-passed white noise reads as a moving whoosh (not broadband
        // hiss), but the source amplitude is trimmed (was 0.8) to keep the
        // gust's contribution to the summed mix gentle.
        id: GUST_NOISE_ID,
        kind: NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }),
        inputs: BTreeMap::new(),
    };

    // Gain CV in [0.05, 0.95]: the gust never fully dies (a hard zero
    // reads as a dropout) and never clips the VCA.
    let lfo_node = GraphNode {
        id: GUST_LFO_ID,
        kind: NodeKind::Lfo(Lfo {
            rate_hz: gust.swell_rate_hz,
            shape: LfoShape::Sine,
            depth: 0.45,
            offset: 0.5,
        }),
        inputs: BTreeMap::new(),
    };

    let mut filter_inputs = BTreeMap::new();
    filter_inputs.insert("in".to_string(), vec![Connection::from_node(GUST_NOISE_ID)]);
    let filter_node = GraphNode {
        id: GUST_FILTER_ID,
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: gust.center_hz,
            q: gust.q,
        }),
        inputs: filter_inputs,
    };

    let mut vca_inputs = BTreeMap::new();
    vca_inputs.insert(
        "in".to_string(),
        vec![Connection::from_node(GUST_FILTER_ID)],
    );
    vca_inputs.insert("gain".to_string(), vec![Connection::from_node(GUST_LFO_ID)]);
    let vca_node = GraphNode {
        id: GUST_VCA_ID,
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_inputs,
    };

    // Same acoustic space as the bed so the gusts live in the room.
    let mut reverb_inputs = BTreeMap::new();
    reverb_inputs.insert("in".to_string(), vec![Connection::from_node(GUST_VCA_ID)]);
    let reverb_node = GraphNode {
        id: GUST_REVERB_ID,
        kind: NodeKind::Reverb(Reverb {
            room_size: params.reverb_room_size,
            damping: params.reverb_damping,
            mix: params.reverb_mix,
        }),
        inputs: reverb_inputs,
    };

    AudioPatch {
        seed: (seed.rotate_left(16) & 0xFFFF_FFFF) as u32,
        graph: NodeGraph {
            nodes: vec![noise_node, lfo_node, filter_node, vca_node, reverb_node],
            output: GUST_REVERB_ID,
        },
    }
}

// ---------------------------------------------------------------------------
// Chime layer — the sparse melodic voice over the bed
// ---------------------------------------------------------------------------

/// Chime-voice timbre. Sine is glassy/soft, Triangle adds a reedy
/// edge for the harsher biomes.
#[derive(Clone, Copy)]
enum ChimeWave {
    Sine,
    Triangle,
}

/// Seeded chime-layer parameters — see [`derive_chime`].
struct ChimeParams {
    wave: ChimeWave,
    /// Root note (Hz) the pentatonic ratios multiply.
    root_hz: f32,
    /// Pentatonic ratio table — major for warm rooms, minor for cool.
    ratios: &'static [f32],
    /// Notes per 16-beat loop.
    note_count: u32,
    attack_s: f32,
    release_s: f32,
    /// Chime-side reverb mix — wetter than the bed so the notes hang.
    reverb_mix: f32,
}

/// Derive the melodic layer. The root pitch is anchored to the scene
/// hue (one octave of range across the colour wheel) so a room's
/// colour and its key are the same fact; temperature picks the mode
/// (warm → major pentatonic, cool → minor); biome sets density —
/// barren rooms chime sparsely, verdant ones closer to a melody.
fn derive_chime(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> ChimeParams {
    let wave = match scene.biome {
        BiomeArchetype::Tundra | BiomeArchetype::Volcanic | BiomeArchetype::Arid => {
            ChimeWave::Triangle
        }
        _ => ChimeWave::Sine,
    };
    // Hue anchors the key; biome shifts the register — volcanic rooms
    // toll an octave down, tundra rings an octave up, alpine a fifth.
    let register = match scene.biome {
        BiomeArchetype::Volcanic => 0.5,
        BiomeArchetype::Tundra => 2.0,
        BiomeArchetype::Alpine => 1.5,
        _ => 1.0,
    };
    let root_hz = 220.0 * 2.0_f32.powf(scene.base_hue_deg / 360.0) * register;
    let ratios = if scene.temperature >= 0.0 {
        PENTATONIC_MAJOR
    } else {
        PENTATONIC_MINOR
    };
    let note_count = match scene.biome {
        BiomeArchetype::Lush | BiomeArchetype::Coastal => 4 + (range_f32(rng, 0.0, 3.0) as u32),
        BiomeArchetype::Arid | BiomeArchetype::Volcanic => 2 + (range_f32(rng, 0.0, 2.0) as u32),
        _ => 3 + (range_f32(rng, 0.0, 2.0) as u32),
    };
    ChimeParams {
        wave,
        root_hz,
        ratios,
        note_count,
        attack_s: range_f32(rng, 0.02, 0.12),
        release_s: range_f32(rng, 1.2, 2.5),
        reverb_mix: range_f32(rng, 0.35, 0.5),
    }
}

/// Node ids inside the chime patch (instrument graphs are independent,
/// so these can overlap the bed's ids without clashing).
const CHIME_GATE_ID: NodeId = NodeId(0);
const CHIME_ADSR_ID: NodeId = NodeId(1);
const CHIME_OSC_ID: NodeId = NodeId(2);
const CHIME_VCA_ID: NodeId = NodeId(3);
const CHIME_REVERB_ID: NodeId = NodeId(4);

/// `Gate → ADSR → VCA` bell voice: the sequencer's per-event gate
/// window strikes the envelope, the VCA shapes the oscillator with it,
/// and a wet reverb hangs each note in the bed's space.
fn build_chime_patch(chime: &ChimeParams, params: &AmbientParams, seed: u64) -> AudioPatch {
    let gate_node = GraphNode {
        id: CHIME_GATE_ID,
        kind: NodeKind::Gate(Gate { invert: false }),
        inputs: BTreeMap::new(),
    };

    let mut adsr_inputs = BTreeMap::new();
    adsr_inputs.insert(
        "gate".to_string(),
        vec![Connection::from_node(CHIME_GATE_ID)],
    );
    let adsr_node = GraphNode {
        id: CHIME_ADSR_ID,
        kind: NodeKind::Adsr(AdsrEnvelope {
            attack_s: chime.attack_s,
            decay_s: 0.6,
            // Bell-like: no sustain plateau, the tail is all release.
            sustain_level: 0.0,
            release_s: chime.release_s,
            curve: AdsrCurve::Exponential,
        }),
        inputs: adsr_inputs,
    };

    let osc_node = GraphNode {
        id: CHIME_OSC_ID,
        kind: match chime.wave {
            ChimeWave::Sine => NodeKind::Sine(SineOsc {
                freq_hz: chime.root_hz,
                phase_offset: 0.0,
                amplitude: 1.0,
            }),
            ChimeWave::Triangle => NodeKind::Triangle(TriangleOsc {
                freq_hz: chime.root_hz,
                amplitude: 1.0,
                anti_alias: Default::default(),
            }),
        },
        inputs: BTreeMap::new(),
    };

    // VCA: base gain 0 so the envelope CV alone opens the voice.
    let mut vca_inputs = BTreeMap::new();
    vca_inputs.insert("in".to_string(), vec![Connection::from_node(CHIME_OSC_ID)]);
    vca_inputs.insert(
        "gain".to_string(),
        vec![Connection::from_node(CHIME_ADSR_ID)],
    );
    let vca_node = GraphNode {
        id: CHIME_VCA_ID,
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_inputs,
    };

    // Share the bed's seeded acoustic space (room size / damping) but
    // run wetter so the chimes ring into it.
    let mut reverb_inputs = BTreeMap::new();
    reverb_inputs.insert("in".to_string(), vec![Connection::from_node(CHIME_VCA_ID)]);
    let reverb_node = GraphNode {
        id: CHIME_REVERB_ID,
        kind: NodeKind::Reverb(Reverb {
            room_size: params.reverb_room_size,
            damping: params.reverb_damping,
            mix: chime.reverb_mix,
        }),
        inputs: reverb_inputs,
    };

    AudioPatch {
        seed: ((seed >> 32) & 0xFFFF_FFFF) as u32,
        graph: NodeGraph {
            nodes: vec![gate_node, adsr_node, osc_node, vca_node, reverb_node],
            output: CHIME_REVERB_ID,
        },
    }
}

/// Scatter the chime notes across the loop region. Onsets land on
/// half-beat boundaries inside `[WARMUP_BEATS, WARMUP_BEATS + 13.5]`
/// — never in the play-once warm-up run-up — and a note whose ring
/// extends past the timeline end is exactly what the loop crossfade
/// wants: its tail blends into the loop start, ringing across the
/// seam. Pitches walk the pentatonic table with an occasional octave
/// lift so the register doesn't flatline.
fn chime_track_events(chime: &ChimeParams, rng: &mut ChaCha8Rng) -> Vec<Event> {
    let mut events = Vec::with_capacity(chime.note_count as usize);
    for _ in 0..chime.note_count {
        // Half-beat quantise an onset across [0, 13.5) of the loop
        // region: floor(t·2)/2.
        let time_beats = WARMUP_BEATS + (range_f32(rng, 0.0, 13.5) * 2.0).floor() * 0.5;
        let ratio_idx =
            (range_f32(rng, 0.0, chime.ratios.len() as f32) as usize).min(chime.ratios.len() - 1);
        let octave = if unit_f32(rng) < 0.2 { 2.0 } else { 1.0 };
        events.push(Event {
            time_beats,
            instrument_id: CHIME_INSTRUMENT_ID.to_string(),
            pitch_multiplier: chime.ratios[ratio_idx] * octave,
            volume: range_f32(rng, 0.10, 0.20),
            gate_beats: range_f32(rng, 0.4, 1.0),
            release_beats: 2.5,
            pitch_mode: PitchMode::Varispeed,
        });
    }
    // Deterministic order regardless of sampling sequence — peers
    // must serialise identical recipes.
    events.sort_by(|a, b| a.time_beats.total_cmp(&b.time_beats));
    events
}

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
    /// Tundra: glassy high tings with a snap attack and long ring.
    IceTing,
    /// Alpine: a quiet distant howl with slow vibrato.
    DistantHowl,
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
        Lush => PunctuationParams {
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
        Arid => PunctuationParams {
            mood: PunctuationMood::WhistleGust,
            base_hz: range_f32(rng, 1_200.0, 2_200.0),
            event_count: 2 + (range_f32(rng, 0.0, 2.0) as u32),
            volume: (0.14, 0.24),
            gate: (1.2, 2.0),
            release_beats: 1.5,
        },
        Tundra => PunctuationParams {
            mood: PunctuationMood::IceTing,
            base_hz: range_f32(rng, 2_400.0, 3_800.0),
            event_count: 2 + (range_f32(rng, 0.0, 3.0) as u32),
            volume: (0.10, 0.18),
            gate: (0.05, 0.12),
            release_beats: 1.2,
        },
        Alpine => PunctuationParams {
            mood: PunctuationMood::DistantHowl,
            base_hz: range_f32(rng, 280.0, 380.0),
            event_count: 1,
            volume: (0.12, 0.2),
            gate: (3.0, 4.0),
            release_beats: 2.0,
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
    let tonal = matches!(punct.mood, BirdChirps | SubBoom | IceTing | DistantHowl);
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
                // loop-sync invariant (4–6 Hz ⇒ 64–96 cycles).
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
            _ => 5.0, // WhistleGust: narrow singing band.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_four_layer_recipe() {
        let scene = SceneCharacter::for_seed(9);
        let a = AmbientRecipe::from_scene(&scene, 9);
        let b = AmbientRecipe::from_scene(&scene, 9);
        assert_eq!(
            a.recipe.instruments.len(),
            4,
            "bed + gust + chime + punctuation"
        );
        assert_eq!(a.recipe.tracks.len(), 4);
        let ev_a = &a.recipe.tracks[2].events;
        let ev_b = &b.recipe.tracks[2].events;
        assert_eq!(ev_a.len(), ev_b.len());
        for (x, y) in ev_a.iter().zip(ev_b.iter()) {
            assert_eq!(x.time_beats, y.time_beats);
            assert_eq!(x.pitch_multiplier, y.pitch_multiplier);
            assert_eq!(x.volume, y.volume);
        }
    }

    #[test]
    fn loop_region_starts_hot_and_lfos_are_loop_synced() {
        for s in 0u64..16 {
            let scene = SceneCharacter::for_seed(s);
            let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
            assert_eq!(recipe.duration_beats, WARMUP_BEATS + LOOP_BEATS);
            assert_eq!(recipe.loop_start_beats, Some(WARMUP_BEATS));
            // The sustained voices must cover the whole timeline and
            // leave a release tail for the crossfade — a zero tail is
            // exactly the fade-in-at-loop-start bug.
            for track in &recipe.tracks[0..2] {
                let e = &track.events[0];
                assert_eq!(e.time_beats, 0.0);
                assert_eq!(e.gate_beats, recipe.duration_beats);
                assert_eq!(e.release_beats, recipe.loop_crossfade_beats);
            }
            // Every LFO rate is whole cycles per loop region (loop is
            // LOOP_BEATS seconds at 60 BPM) so modulation phase is
            // continuous across the seam.
            for instrument in &recipe.instruments {
                for node in &instrument.patch.graph.nodes {
                    if let NodeKind::Lfo(lfo) = &node.kind {
                        let cycles = lfo.rate_hz * LOOP_BEATS;
                        assert!(
                            (cycles - cycles.round()).abs() < 1e-4,
                            "LFO rate {} Hz is not loop-synced ({cycles} cycles)",
                            lfo.rate_hz
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn chime_notes_stay_in_loop_region_and_under_the_bed() {
        for s in 0u64..32 {
            let scene = SceneCharacter::for_seed(s);
            let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
            let chimes = &recipe.tracks[2].events;
            assert!(!chimes.is_empty(), "every room gets at least one chime");
            assert!(chimes.len() <= 7);
            for e in chimes {
                assert_eq!(e.instrument_id, CHIME_INSTRUMENT_ID);
                // Never in the play-once warm-up run-up.
                assert!(e.time_beats >= WARMUP_BEATS);
                // Tails may overhang the timeline end by at most the
                // crossfade window (that overhang *is* the seam blend).
                assert!(
                    e.time_beats + e.gate_beats + e.release_beats
                        <= recipe.duration_beats + recipe.loop_crossfade_beats + 1e-3,
                    "chime tail exceeds the crossfade overhang: onset {} gate {} release {}",
                    e.time_beats,
                    e.gate_beats,
                    e.release_beats
                );
                assert!(
                    (0.05..=0.3).contains(&e.volume),
                    "chimes stay under the bed"
                );
            }
            // Onsets are sorted so peers serialise identical recipes.
            for pair in chimes.windows(2) {
                assert!(pair[0].time_beats <= pair[1].time_beats);
            }
        }
    }

    #[test]
    fn punctuation_differs_in_kind_per_biome() {
        use BiomeArchetype::*;
        // The punctuation patch must be structurally different across
        // biomes — tonal moods carry a Sine source, noise moods a
        // WhiteNoise → Bandpass chain — and every event must fit the
        // loop-plus-crossfade window.
        let has_sine = |patch: &AudioPatch| {
            patch
                .graph
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Sine(_)))
        };
        let expects_sine = |b: BiomeArchetype| matches!(b, Lush | Volcanic | Tundra | Alpine);
        for biome in BiomeArchetype::ALL {
            for s in 0u64..4 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
                let punct_patch = &recipe.instruments[3].patch;
                assert_eq!(
                    has_sine(punct_patch),
                    expects_sine(biome),
                    "{biome:?} punctuation voice has the wrong source family"
                );
                let events = &recipe.tracks[3].events;
                assert!(!events.is_empty(), "{biome:?} has no punctuation events");
                for e in events {
                    assert_eq!(e.instrument_id, PUNCT_INSTRUMENT_ID);
                    assert!(e.time_beats >= WARMUP_BEATS);
                    assert!(
                        e.time_beats + e.gate_beats + e.release_beats
                            <= recipe.duration_beats + recipe.loop_crossfade_beats + 1e-3,
                        "{biome:?} punctuation tail exceeds the crossfade overhang"
                    );
                }
            }
        }
        // Arid/tundra beds run highpass; lush stays lowpass.
        for (biome, expect_hp) in [(Arid, true), (Tundra, true), (Lush, false)] {
            let mut scene = SceneCharacter::for_seed(1);
            scene.biome = biome;
            let recipe = AmbientRecipe::from_scene(&scene, 1).recipe;
            let bed = &recipe.instruments[0].patch;
            let hp = bed
                .graph
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::BiquadHighpass(_)));
            assert_eq!(hp, expect_hp, "{biome:?} bed filter family wrong");
        }
    }

    #[test]
    fn warm_rooms_chime_major_cool_rooms_minor() {
        // Notes may carry an octave lift (×2), so membership is
        // checked against the base ratio.
        let on_table =
            |table: &[f32], pitch: f32| table.contains(&pitch) || table.contains(&(pitch / 2.0));
        for s in 0u64..32 {
            let mut scene = SceneCharacter::for_seed(s);
            scene.temperature = 0.8;
            let warm = AmbientRecipe::from_scene(&scene, s).recipe;
            for e in &warm.tracks[2].events {
                assert!(
                    on_table(PENTATONIC_MAJOR, e.pitch_multiplier),
                    "warm room chimed off the major table: {}",
                    e.pitch_multiplier
                );
            }
            scene.temperature = -0.8;
            let cool = AmbientRecipe::from_scene(&scene, s).recipe;
            for e in &cool.tracks[2].events {
                assert!(
                    on_table(PENTATONIC_MINOR, e.pitch_multiplier),
                    "cool room chimed off the minor table: {}",
                    e.pitch_multiplier
                );
            }
        }
    }
}
