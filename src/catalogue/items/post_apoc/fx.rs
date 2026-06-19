//! Post-apocalyptic "bring-it-to-life" helpers: drifting ash and a barrel-fire
//! flame, plus two spatial-audio patches — a desolate wind for the camp and a
//! fire crackle for the burning drums.
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
    SineOsc, WhiteNoise,
};

use crate::catalogue::items::fx::{Emitter, node, patch};
use crate::pds::{
    EmitterShape, Fp, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig,
    SovereignFlameConfig, SovereignPuffConfig, SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A grey veil of ash and grit drifting low across the wasteland on a dry
/// wind — the dust haze of the dead world.
pub(super) fn ash_drift(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([4.0, 0.6, 4.0]),
        },
        rate: 7.0,
        burst: 0,
        max: 60,
        life: (3.0, 6.0),
        speed: (0.3, 0.9),
        gravity: 0.02,
        accel: [0.5, 0.05, 0.1],
        drag: 0.6,
        size: (0.3, 1.4),
        start_color: [0.50, 0.48, 0.44, 0.22],
        end_color: [0.54, 0.52, 0.48, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0A57_0700) as u32,
            color_base: Fp3([0.56, 0.54, 0.50]),
            color_shadow: Fp3([0.34, 0.32, 0.30]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A guttering barrel-fire flame leaping from a burning drum.
pub(super) fn fire_flame(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.34),
            height: Fp(0.3),
        },
        rate: 20.0,
        burst: 0,
        max: 100,
        life: (0.5, 1.2),
        speed: (0.7, 1.6),
        gravity: -0.1,
        accel: [0.1, 0.3, 0.0],
        drag: 0.3,
        size: (0.35, 0.0),
        start_color: [1.0, 0.7, 0.22, 1.0],
        end_color: [0.7, 0.15, 0.05, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Flame(SovereignFlameConfig {
            seed: (seed ^ 0x0A57_F1E0) as u32,
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A desolate dry wind — band-limited noise breathing very slowly through a
/// lowpass, the empty air of the wasteland.
pub(super) fn desolate_wind() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 280.0,
                q: 1.3,
            }),
            inputs: lp_in,
        }
    };
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.18,
            shape: LfoShape::Sine,
            depth: 0.55,
            offset: 0.35,
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

/// A warm irregular fire crackle — band-passed noise pulsed by a fast LFO over
/// a low ember rumble. The voice of a burning barrel.
pub(super) fn fire_crackle() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.6 }));
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 7.0,
            shape: LfoShape::Sine,
            depth: 0.8,
            offset: 0.2,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1500.0,
            q: 2.0,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let crackle = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let rumble = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 68.0,
            phase_offset: 0.0,
            amplitude: 0.16,
        }),
    );
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(3)),
            Connection::from_node(NodeId(4)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(5),
        kind: NodeKind::Gain(Gain { gain: 0.7 }),
        inputs: mix_in,
    };
    patch(vec![noise, lfo, bp, crackle, rumble, mix], NodeId(5))
}
