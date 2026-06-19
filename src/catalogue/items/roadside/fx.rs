//! Roadside "bring-it-to-life" helpers: a thin shoulder-dust emitter the
//! forecourt kicks up, and two spatial-audio patches — a buzzing-neon hum
//! for the lit signs and a distant highway drone for the lot.
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

/// A thin veil of warm grey dust drifting low across the lot — grit lifted
/// off the cracked forecourt on the draught of passing trucks.
pub(super) fn road_dust(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([2.5, 0.1, 0.4]),
        },
        rate: 6.0,
        burst: 0,
        max: 60,
        life: (2.0, 4.0),
        speed: (0.4, 1.0),
        gravity: 0.05,
        accel: [0.6, 0.05, 0.0],
        drag: 0.7,
        size: (0.3, 1.2),
        start_color: [0.70, 0.66, 0.56, 0.22],
        end_color: [0.74, 0.70, 0.60, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0D05_7000) as u32,
            color_base: Fp3([0.74, 0.70, 0.60]),
            color_shadow: Fp3([0.50, 0.46, 0.38]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A buzzing neon hum — a mains tone with a band-passed noise crackle pulsed
/// by a fast uneven LFO, the electrical fizz of a lit sign.
pub(super) fn neon_buzz() -> SovereignAudioConfig {
    // Mains hum under the buzz.
    let hum = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 120.0,
            phase_offset: 0.0,
            amplitude: 0.16,
        }),
    );
    let noise = node(1, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.4 }));
    // Fast, uneven flicker so the buzz comes in bursts.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 14.0,
            shape: LfoShape::Sine,
            depth: 0.8,
            offset: 0.2,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    let bp = GraphNode {
        id: NodeId(3),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 2400.0,
            q: 3.0,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let crackle = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(4)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(5),
        kind: NodeKind::Gain(Gain { gain: 0.6 }),
        inputs: mix_in,
    };
    patch(vec![hum, noise, lfo, bp, crackle, mix], NodeId(5))
}

/// A low distant highway drone — broadband noise rolled off through a
/// lowpass and swelled by a slow LFO, the whoosh of traffic over the rise.
pub(super) fn highway_drone() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 260.0,
                q: 1.0,
            }),
            inputs: lp_in,
        }
    };
    // Slow swell as cars pass and recede.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.35,
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
