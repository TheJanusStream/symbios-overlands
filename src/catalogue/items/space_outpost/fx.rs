//! Space-Outpost "bring-it-to-life" helpers: a thin drift of regolith dust,
//! and two spatial-audio patches — a steady reactor hum for the habitat dome
//! and a pulsing radio static for the comms dish.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{
    BiquadBandpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind, SineOsc,
    WhiteNoise,
};

use crate::catalogue::items::fx::{Emitter, node, patch};
use crate::pds::{
    EmitterShape, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignPuffConfig,
    SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A thin veil of rust-grey regolith dust skating low across the ground on
/// the thin wind — the restless surface of the outpost world.
pub(super) fn regolith_dust(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([4.0, 0.15, 4.0]),
        },
        rate: 7.0,
        burst: 0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.4, 1.0),
        gravity: 0.02,
        accel: [0.5, 0.04, 0.1],
        drag: 0.6,
        size: (0.2, 1.0),
        start_color: [0.62, 0.50, 0.44, 0.2],
        end_color: [0.66, 0.54, 0.48, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x5EA0_0700) as u32,
            color_base: Fp3([0.68, 0.56, 0.50]),
            color_shadow: Fp3([0.44, 0.34, 0.30]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A steady reactor hum — two stacked sine partials under a faint tremolo,
/// the life-support plant of the habitat.
pub(super) fn reactor_hum() -> SovereignAudioConfig {
    let low = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 110.0,
            phase_offset: 0.0,
            amplitude: 0.2,
        }),
    );
    let harm = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 220.0,
            phase_offset: 0.0,
            amplitude: 0.08,
        }),
    );
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(1)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(2),
        kind: NodeKind::Gain(Gain { gain: 1.0 }),
        inputs: mix_in,
    };
    let lfo = node(
        3,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.7,
            shape: LfoShape::Sine,
            depth: 0.25,
            offset: 0.7,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(3))]);
    let vca = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![low, harm, mix, lfo, vca], NodeId(4))
}

/// A pulsing radio static — mid band-passed noise gated by a slow uneven LFO,
/// the comms dish listening to the void.
pub(super) fn comms_static() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.4 }));
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1800.0,
            q: 1.2,
        }),
        inputs: bp_in,
    };
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 3.0,
            shape: LfoShape::Sine,
            depth: 0.7,
            offset: 0.25,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let pulse = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    let mix = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.5 }),
        inputs: mix_in,
    };
    patch(vec![noise, bp, lfo, pulse, mix], NodeId(4))
}
