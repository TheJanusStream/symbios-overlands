//! Alien-Monolithic "bring-it-to-life" helpers: slow rising energy motes and
//! two spatial-audio patches — a deep resonant hum for the monolith and a high
//! power shimmer for the light pylon.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind, SineOsc};

use crate::catalogue::items::fx::{Emitter, node, patch};
use crate::pds::{
    EmitterShape, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignSoftDiscConfig,
    SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// Slow, orderly motes of blue energy rising straight up — the field around an
/// active monolith.
pub(super) fn energy_motes(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([1.6, 0.4, 1.6]),
        },
        rate: 8.0,
        burst: 0,
        max: 60,
        life: (3.0, 6.0),
        speed: (0.3, 0.7),
        gravity: -0.08,
        accel: [0.0, 0.25, 0.0],
        drag: 0.3,
        size: (0.1, 0.0),
        start_color: [0.5, 0.65, 1.0, 0.9],
        end_color: [0.4, 0.9, 1.0, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
            seed: (seed ^ 0x0A30_0F00) as u32,
            color_core: Fp3([0.8, 0.9, 1.0]),
            color_halo: Fp3([0.3, 0.5, 1.0]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A deep resonant monolith hum — a low fundamental and a pure fifth above,
/// steady under a faint tremolo. The voice of the active array.
pub(super) fn monolith_hum() -> SovereignAudioConfig {
    let low = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 55.0,
            phase_offset: 0.0,
            amplitude: 0.26,
        }),
    );
    let fifth = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 82.5,
            phase_offset: 0.0,
            amplitude: 0.12,
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
            rate_hz: 0.5,
            shape: LfoShape::Sine,
            depth: 0.2,
            offset: 0.75,
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
    patch(vec![low, fifth, mix, lfo, vca], NodeId(4))
}

/// A high power shimmer — a pure tone ringing under a quick tremolo, the charge
/// crackling at a light pylon's tip.
pub(super) fn power_shimmer() -> SovereignAudioConfig {
    let tone = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 988.0,
            phase_offset: 0.0,
            amplitude: 0.1,
        }),
    );
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 6.0,
            shape: LfoShape::Sine,
            depth: 0.7,
            offset: 0.3,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let vca = GraphNode {
        id: NodeId(2),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![tone, lfo, vca], NodeId(2))
}
