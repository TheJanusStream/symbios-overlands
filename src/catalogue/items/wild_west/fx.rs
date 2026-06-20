//! Wild-West "bring-it-to-life" helpers: drifting prairie dust and two
//! spatial-audio patches — a dry prairie wind for the saloon and a slow
//! windmill creak for the water tower.
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

/// A low veil of tan dust skating along the street on the dry wind — the
/// restless dirt of the frontier.
pub(super) fn dust_drift(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([4.0, 0.15, 3.0]),
        },
        rate: 7.0,
        burst: 0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.5, 1.2),
        gravity: 0.02,
        accel: [0.7, 0.05, 0.0],
        drag: 0.6,
        size: (0.3, 1.4),
        start_color: [0.66, 0.56, 0.40, 0.22],
        end_color: [0.70, 0.60, 0.44, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0DE5_0700) as u32,
            color_base: Fp3([0.72, 0.62, 0.46]),
            color_shadow: Fp3([0.48, 0.40, 0.28]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A dry prairie wind — band-limited noise breathing slowly through a lowpass,
/// the empty air over the street.
pub(super) fn prairie_wind() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.46 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 340.0,
                q: 1.2,
            }),
            inputs: lp_in,
        }
    };
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.22,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.4,
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

/// A slow windmill creak — narrow band-passed noise pulsed by a slow LFO, the
/// rhythmic groan of the wind pump turning.
pub(super) fn windmill_creak() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 900.0,
            q: 5.0,
        }),
        inputs: bp_in,
    };
    // Slow groan rhythm as the vane turns.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.8,
            shape: LfoShape::Sine,
            depth: 0.85,
            offset: 0.15,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let creak = GraphNode {
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
    patch(vec![noise, bp, lfo, creak, mix], NodeId(4))
}
