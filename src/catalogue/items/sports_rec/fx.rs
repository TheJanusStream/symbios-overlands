//! Sports / Recreation "bring-it-to-life" helpers: a faint dust drifting
//! over the pitch, and two spatial-audio patches — a swelling crowd murmur
//! for the stadium bowl and a low tannoy hum for the scoreboard.
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
    EmitterShape, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignPuffConfig,
    SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A faint drift of pale dust over the pitch — chalk and grit lifted off the
/// field on the breeze.
pub(super) fn field_dust(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([6.0, 0.4, 4.0]),
        },
        rate: 6.0,
        burst: 0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.2, 0.7),
        gravity: 0.03,
        accel: [0.3, 0.05, 0.0],
        drag: 0.6,
        size: (0.2, 0.9),
        start_color: [0.82, 0.82, 0.76, 0.18],
        end_color: [0.84, 0.84, 0.78, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x05F0_0700) as u32,
            color_base: Fp3([0.86, 0.86, 0.80]),
            color_shadow: Fp3([0.60, 0.60, 0.54]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A swelling crowd murmur — vocal-band noise rising and falling on a slow
/// LFO, the breathing hubbub of a full stand.
pub(super) fn crowd_murmur() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.6 }));
    // Vocal band so the noise reads as voices, not hiss.
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 700.0,
            q: 0.7,
        }),
        inputs: bp_in,
    };
    // Slow swell as the crowd surges and settles.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.45,
            shape: LfoShape::Sine,
            depth: 0.6,
            offset: 0.4,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let murmur = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    let mix = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.7 }),
        inputs: mix_in,
    };
    patch(vec![noise, bp, lfo, murmur, mix], NodeId(4))
}

/// A low tannoy hum — a mains tone under a lowpassed hiss, the idle PA at
/// the scoreboard.
pub(super) fn tannoy_hum() -> SovereignAudioConfig {
    let hum = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 100.0,
            phase_offset: 0.0,
            amplitude: 0.14,
        }),
    );
    let noise = node(1, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.25 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
        GraphNode {
            id: NodeId(2),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 220.0,
                q: 1.0,
            }),
            inputs: lp_in,
        }
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(2)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.5 }),
        inputs: mix_in,
    };
    patch(vec![hum, noise, lp, mix], NodeId(3))
}
