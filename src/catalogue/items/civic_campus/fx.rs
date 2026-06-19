//! Civic / Campus "bring-it-to-life" helpers: a thin drift of seed-fluff
//! across the quad, and two spatial-audio patches — a soft resonant hum for
//! the clock tower and a calm airy bed for the town-hall lawn.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{
    BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind, SineOsc,
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

/// A lazy drift of pale seed-fluff carried across the quad on the breeze —
/// the soft motion of a still campus afternoon.
pub(super) fn seed_drift(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([3.0, 1.2, 3.0]),
        },
        rate: 5.0,
        burst: 0,
        max: 50,
        life: (3.0, 6.0),
        speed: (0.2, 0.7),
        gravity: -0.02,
        accel: [0.25, 0.05, 0.1],
        drag: 0.5,
        size: (0.05, 0.04),
        start_color: [0.94, 0.93, 0.86, 0.55],
        end_color: [0.94, 0.93, 0.86, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0C1F_0F00) as u32,
            color_base: Fp3([0.96, 0.95, 0.90]),
            color_shadow: Fp3([0.80, 0.80, 0.74]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A soft resonant tower hum — two metallic sine partials under a slow
/// tremolo, the lingering ring of the clock-tower bell mechanism.
pub(super) fn tower_resonance() -> SovereignAudioConfig {
    let low = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 165.0,
            phase_offset: 0.0,
            amplitude: 0.16,
        }),
    );
    let high = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 247.5,
            phase_offset: 0.0,
            amplitude: 0.10,
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
    // Slow tremolo so the ring breathes.
    let lfo = node(
        3,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.4,
            shape: LfoShape::Sine,
            depth: 0.4,
            offset: 0.5,
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
    patch(vec![low, high, mix, lfo, vca], NodeId(4))
}

/// A calm airy quad bed — band-limited noise breathing slowly through a
/// lowpass, the quiet of an open campus lawn.
pub(super) fn campus_calm() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.34 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 480.0,
                q: 1.0,
            }),
            inputs: lp_in,
        }
    };
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.18,
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
