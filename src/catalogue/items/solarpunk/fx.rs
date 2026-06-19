//! Solarpunk "bring-it-to-life" helpers: a soft drift of golden pollen, and
//! two spatial-audio patches — a chirping birdsong for the green pavilion and
//! a clean-air breeze for the biodome.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{
    BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind,
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

/// A soft drift of golden pollen and seed-fluff carried on a gentle breeze —
/// the living air of the eco-quarter.
pub(super) fn pollen_drift(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([4.0, 1.5, 4.0]),
        },
        rate: 6.0,
        burst: 0,
        max: 50,
        life: (3.0, 6.0),
        speed: (0.2, 0.7),
        gravity: -0.02,
        accel: [0.2, 0.06, 0.1],
        drag: 0.5,
        size: (0.06, 0.05),
        start_color: [1.0, 0.92, 0.5, 0.6],
        end_color: [1.0, 0.92, 0.5, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x501A_0F00) as u32,
            color_base: Fp3([1.0, 0.94, 0.6]),
            color_shadow: Fp3([0.8, 0.74, 0.4]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A bright birdsong — high band-passed noise chittering on a quick uneven
/// LFO, the dawn chorus over the gardens.
pub(super) fn birdsong() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    // High band so the noise reads as chirps, not hiss.
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 4200.0,
            q: 5.0,
        }),
        inputs: bp_in,
    };
    // Quick uneven chirp gate.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 7.0,
            shape: LfoShape::Sine,
            depth: 0.9,
            offset: 0.1,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let chirp = GraphNode {
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
    patch(vec![noise, bp, lfo, chirp, mix], NodeId(4))
}

/// A soft clean-air breeze — band-limited noise breathing slowly through a
/// lowpass, the fresh air of the dome gardens.
pub(super) fn breeze_calm() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.34 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 500.0,
                q: 1.0,
            }),
            inputs: lp_in,
        }
    };
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.2,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.45,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let vca = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![noise, lp, lfo, vca], NodeId(3))
}
