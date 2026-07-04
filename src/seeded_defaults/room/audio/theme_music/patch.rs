//! Synth-patch plumbing: the fixed node-graph layout (gate → ADSR →
//! dual osc → VCA → shared reverb) and [`build_patch`], which realises a
//! [`ThemeVoice`]'s timbre as an [`AudioPatch`].

use std::collections::BTreeMap;

use bevy_symbios_audio::{
    AdsrCurve, AdsrEnvelope, AudioPatch, Connection, Gain, Gate, GraphNode, NodeGraph, NodeId,
    NodeKind, Reverb, SawtoothOsc, SineOsc, TriangleOsc,
};

use super::super::bed::AmbientParams;
use super::voices::{ThemeVoice, Wave};

const GATE_ID: NodeId = NodeId(0);
const ADSR_ID: NodeId = NodeId(1);
const OSC1_ID: NodeId = NodeId(2);
const OSC2_ID: NodeId = NodeId(3);
const VCA_ID: NodeId = NodeId(4);
const REVERB_ID: NodeId = NodeId(5);

pub(super) fn osc(id: NodeId, wave: Wave, freq_hz: f32, amplitude: f32) -> GraphNode {
    let kind = match wave {
        Wave::Sine => NodeKind::Sine(SineOsc {
            freq_hz,
            phase_offset: 0.0,
            amplitude,
        }),
        Wave::Triangle => NodeKind::Triangle(TriangleOsc {
            freq_hz,
            amplitude,
            anti_alias: Default::default(),
        }),
        Wave::Sawtooth => NodeKind::Sawtooth(SawtoothOsc {
            freq_hz,
            polarity: Default::default(),
            amplitude,
            anti_alias: Default::default(),
        }),
    };
    GraphNode {
        id,
        kind,
        inputs: BTreeMap::new(),
    }
}

/// `Gate -> ADSR -> osc(es) -> VCA -> reverb`: the per-event gate strikes
/// the envelope, which shapes the (optionally detuned-stacked) oscillator
/// through the VCA, ringing into the bed's shared space.
pub(super) fn build_patch(
    voice: &ThemeVoice,
    root_hz: f32,
    params: &AmbientParams,
    seed: u64,
) -> AudioPatch {
    let gate = GraphNode {
        id: GATE_ID,
        kind: NodeKind::Gate(Gate { invert: false }),
        inputs: BTreeMap::new(),
    };
    let mut adsr_inputs = BTreeMap::new();
    adsr_inputs.insert("gate".to_string(), vec![Connection::from_node(GATE_ID)]);
    let adsr = GraphNode {
        id: ADSR_ID,
        kind: NodeKind::Adsr(AdsrEnvelope {
            attack_s: voice.attack_s,
            decay_s: voice.decay_s,
            sustain_level: voice.sustain_level,
            release_s: voice.release_s,
            curve: AdsrCurve::Exponential,
        }),
        inputs: adsr_inputs,
    };

    let stacked = voice.detune_cents > 0.0;
    let osc_amp = if stacked { 0.6 } else { 1.0 };
    let mut nodes = vec![gate, adsr, osc(OSC1_ID, voice.wave, root_hz, osc_amp)];
    let mut vca_in = vec![Connection::from_node(OSC1_ID)];
    if stacked {
        let detuned = root_hz * 2.0_f32.powf(voice.detune_cents / 1200.0);
        nodes.push(osc(OSC2_ID, voice.wave, detuned, osc_amp));
        vca_in.push(Connection::from_node(OSC2_ID));
    }

    let mut vca_inputs = BTreeMap::new();
    vca_inputs.insert("in".to_string(), vca_in);
    vca_inputs.insert("gain".to_string(), vec![Connection::from_node(ADSR_ID)]);
    nodes.push(GraphNode {
        id: VCA_ID,
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_inputs,
    });

    let mut reverb_inputs = BTreeMap::new();
    reverb_inputs.insert("in".to_string(), vec![Connection::from_node(VCA_ID)]);
    nodes.push(GraphNode {
        id: REVERB_ID,
        kind: NodeKind::Reverb(Reverb {
            room_size: params.reverb_room_size,
            damping: params.reverb_damping,
            mix: voice.reverb_mix,
        }),
        inputs: reverb_inputs,
    });

    AudioPatch {
        seed: (seed.rotate_left(24) & 0xFFFF_FFFF) as u32,
        graph: NodeGraph {
            nodes,
            output: REVERB_ID,
        },
    }
}
