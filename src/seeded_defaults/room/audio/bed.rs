//! Biome ambient *texture* — the atonal noise bed + wind-gust layer.
//!
//! This owns the environment's *sound* (noise bed + band-passed gusts),
//! driven by [`BiomeArchetype`] / [`LandformArchetype`]. The tonal,
//! melodic character now lives in [`super::theme_music`].
//! [`build_texture`] is the entry the orchestrator ([`super`]) calls; it
//! also returns the derived [`AmbientParams`] so the theme-music layer
//! can share the bed's acoustic space (reverb).

use std::collections::BTreeMap;

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, BiquadHighpass, BiquadLowpass, BrownNoise, Connection, Gain,
    GraphNode, Instrument, Lfo, LfoShape, NodeGraph, NodeId, NodeKind, PinkNoise, Reverb, Track,
    WhiteNoise,
};
use rand_chacha::ChaCha8Rng;

use super::{LOOP_BEATS, sustained};
use crate::seeded_defaults::scene::{BiomeArchetype, LandformArchetype, SceneCharacter, range_f32};

/// Stable instrument id for the sustained noise bed.
const INSTRUMENT_ID: &str = "ambient_bed";
/// Stable instrument id for the slow gust-swell layer.
const GUST_INSTRUMENT_ID: &str = "gust_swell";

/// Node ids inside the ambient patch — kept as constants so the wiring
/// reads top-to-bottom rather than threading magic integers.
const NOISE_ID: NodeId = NodeId(0);
const LFO_ID: NodeId = NodeId(1);
const FILTER_ID: NodeId = NodeId(2);
const REVERB_ID: NodeId = NodeId(3);

/// Tunable parameters extracted before patch construction so each piece
/// of the wiring graph can be read in one place.
pub(super) struct AmbientParams {
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
    pub(super) reverb_room_size: f32,
    /// Reverb damping in `[0, 1]` — darker (more absorbed highs) for
    /// dense/muffled biomes, brighter for open ones.
    pub(super) reverb_damping: f32,
    /// Reverb dry/wet mix in `[0, 1]` — kept modest so the bed stays
    /// readable rather than washing out.
    pub(super) reverb_mix: f32,
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

/// Build the biome texture layer — the sustained noise bed + the gust
/// swell — as instruments + full-timeline tracks, returning the derived
/// [`AmbientParams`] so the theme-music layer can reuse the bed's reverb.
pub(super) fn build_texture(
    scene: &SceneCharacter,
    rng: &mut ChaCha8Rng,
    seed: u64,
) -> (AmbientParams, Vec<Instrument>, Vec<Track>) {
    let params = derive_params(scene, rng);
    let bed_patch = build_patch(&params, seed);
    let gust = derive_gust(&params, rng);
    let gust_patch = build_gust_patch(&gust, &params, seed);

    let instruments = vec![
        Instrument {
            id: INSTRUMENT_ID.to_string(),
            patch: bed_patch,
        },
        Instrument {
            id: GUST_INSTRUMENT_ID.to_string(),
            patch: gust_patch,
        },
    ];
    // Bed event volume trimmed so the bed + gust + theme-music sum stays
    // under the mixdown tanh knee (see noise_amplitude rationale).
    let tracks = vec![
        Track {
            events: vec![sustained(INSTRUMENT_ID, 0.5)],
        },
        Track {
            events: vec![sustained(GUST_INSTRUMENT_ID, gust.volume)],
        },
    ];
    (params, instruments, tracks)
}
