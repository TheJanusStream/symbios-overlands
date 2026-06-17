//! Conflict tension layer — a low, pulsing dissonant two-tone "alarm" that
//! only sounds when a room's escalation reaches
//! [`EscalationTier::Conflict`]. It is the audio counterpart of the smoke
//! particles and the smoke-red fog accent: calm and tense rooms never carry
//! it, so the layer is strictly additive and gated.
//!
//! The voice is two sine oscillators a tritone apart (the most restless
//! interval) summed through a VCA whose gain is driven by a slow,
//! loop-synced sine LFO — the swell that reads as a distant siren rising
//! and falling. It shares the bed's reverb space ([`AmbientParams`]) so it
//! sits in the same room as everything else, and its volume is kept well
//! under the bed so it colours the mood rather than dominating it.

use std::collections::BTreeMap;

use bevy_symbios_audio::{
    AudioPatch, Connection, Gain, GraphNode, Instrument, Lfo, LfoShape, NodeGraph, NodeId,
    NodeKind, Reverb, SineOsc, Track,
};
use rand_chacha::ChaCha8Rng;

use super::LOOP_BEATS;
use super::bed::AmbientParams;
use crate::seeded_defaults::scene::{EscalationTier, SceneCharacter, range_f32};

/// Stable instrument id for the conflict tension siren.
pub(super) const TENSION_INSTRUMENT_ID: &str = "tension_siren";

/// Tritone ratio (√2) — the dissonant interval between the two tones.
const TRITONE: f32 = std::f32::consts::SQRT_2;

const OSC1_ID: NodeId = NodeId(0);
const OSC2_ID: NodeId = NodeId(1);
const LFO_ID: NodeId = NodeId(2);
const VCA_ID: NodeId = NodeId(3);
const REVERB_ID: NodeId = NodeId(4);

fn sine(id: NodeId, freq_hz: f32, amplitude: f32) -> GraphNode {
    GraphNode {
        id,
        kind: NodeKind::Sine(SineOsc {
            freq_hz,
            phase_offset: 0.0,
            amplitude,
        }),
        inputs: BTreeMap::new(),
    }
}

/// `osc1 + osc2(tritone) → VCA(gain ← slow sine LFO) → reverb`: the LFO
/// swells the dissonant pair from near-silence to full and back, reading as
/// a distant rising/falling alarm.
fn build_patch(root_hz: f32, lfo_rate_hz: f32, params: &AmbientParams, seed: u64) -> AudioPatch {
    let osc1 = sine(OSC1_ID, root_hz, 0.6);
    let osc2 = sine(OSC2_ID, root_hz * TRITONE, 0.6);

    // Gain CV in [0.05, 0.95] (offset ± depth): the siren never fully dies
    // nor clips the VCA.
    let lfo = GraphNode {
        id: LFO_ID,
        kind: NodeKind::Lfo(Lfo {
            rate_hz: lfo_rate_hz,
            shape: LfoShape::Sine,
            depth: 0.45,
            offset: 0.5,
        }),
        inputs: BTreeMap::new(),
    };

    let mut vca_inputs = BTreeMap::new();
    vca_inputs.insert(
        "in".to_string(),
        vec![
            Connection::from_node(OSC1_ID),
            Connection::from_node(OSC2_ID),
        ],
    );
    vca_inputs.insert("gain".to_string(), vec![Connection::from_node(LFO_ID)]);
    let vca = GraphNode {
        id: VCA_ID,
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_inputs,
    };

    let mut reverb_inputs = BTreeMap::new();
    reverb_inputs.insert("in".to_string(), vec![Connection::from_node(VCA_ID)]);
    let reverb = GraphNode {
        id: REVERB_ID,
        kind: NodeKind::Reverb(Reverb {
            room_size: params.reverb_room_size,
            damping: params.reverb_damping,
            mix: 0.3,
        }),
        inputs: reverb_inputs,
    };

    AudioPatch {
        seed: (seed.rotate_left(8) & 0xFFFF_FFFF) as u32,
        graph: NodeGraph {
            nodes: vec![osc1, osc2, lfo, vca, reverb],
            output: REVERB_ID,
        },
    }
}

/// Uniform pick of `lo..=hi` whole LFO cycles per loop region, as a rate in
/// Hz — whole cycles keep the swell phase continuous across the loop seam.
fn loop_synced_rate(rng: &mut ChaCha8Rng, lo: u32, hi: u32) -> f32 {
    let cycles = lo + (range_f32(rng, 0.0, (hi - lo + 1) as f32) as u32).min(hi - lo);
    cycles as f32 / LOOP_BEATS
}

/// Build the conflict tension layer, or `None` for any non-conflict room.
/// Shares the bed's reverb space via `params`.
pub(super) fn build(
    scene: &SceneCharacter,
    params: &AmbientParams,
    rng: &mut ChaCha8Rng,
    seed: u64,
) -> Option<(Instrument, Track)> {
    if scene.escalation_tier() != EscalationTier::Conflict {
        return None;
    }
    // A low-mid, ominous root from the room's hue, an octave below the
    // theme voice's 220 Hz anchor.
    let root_hz = 110.0 * 2.0_f32.powf(scene.base_hue_deg / 360.0);
    let lfo_rate_hz = loop_synced_rate(rng, 5, 8);
    let patch = build_patch(root_hz, lfo_rate_hz, params, seed);
    Some((
        Instrument {
            id: TENSION_INSTRUMENT_ID.to_string(),
            patch,
        },
        Track {
            events: vec![super::sustained(TENSION_INSTRUMENT_ID, 0.12)],
        },
    ))
}
