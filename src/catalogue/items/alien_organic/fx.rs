//! Alien-Organic "bring-it-to-life" helpers: drifting glowing spores and two
//! spatial-audio patches — a low organic pulse for the hive and an eerie high
//! whine for the fleshy spire.
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

/// Glowing green spores drifting and rising on the warm exhalations of the
/// hive — the living air of the colony.
pub(super) fn spore_drift(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([2.0, 0.5, 2.0]),
        },
        rate: 8.0,
        burst: 0,
        max: 60,
        life: (3.0, 6.0),
        speed: (0.2, 0.6),
        gravity: -0.05,
        accel: [0.05, 0.2, 0.05],
        drag: 0.4,
        size: (0.1, 0.0),
        start_color: [0.5, 1.0, 0.55, 0.9],
        end_color: [0.4, 0.95, 0.7, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
            seed: (seed ^ 0x0A11_0F00) as u32,
            color_core: Fp3([0.7, 1.0, 0.7]),
            color_halo: Fp3([0.2, 0.8, 0.4]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A low organic pulse — a deep sine swelling on a slow heartbeat LFO, the
/// breathing of the living hive.
pub(super) fn bio_pulse() -> SovereignAudioConfig {
    let low = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 58.0,
            phase_offset: 0.0,
            amplitude: 0.28,
        }),
    );
    // Slow heartbeat-ish pulse.
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.7,
            shape: LfoShape::Sine,
            depth: 0.8,
            offset: 0.2,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let pulse = GraphNode {
        id: NodeId(2),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    let mix = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.8 }),
        inputs: mix_in,
    };
    patch(vec![low, lfo, pulse, mix], NodeId(3))
}

/// An eerie high whine — two close-detuned high sines beating into an
/// unsettling shimmer, the keening of alien tissue.
pub(super) fn eerie_whine() -> SovereignAudioConfig {
    let a = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 660.0,
            phase_offset: 0.0,
            amplitude: 0.1,
        }),
    );
    let b = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 666.0,
            phase_offset: 0.0,
            amplitude: 0.1,
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
    // Slow swell so the whine breathes.
    let lfo = node(
        3,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.4,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.45,
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
    patch(vec![a, b, mix, lfo, vca], NodeId(4))
}
