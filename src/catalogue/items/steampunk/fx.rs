//! Steampunk "bring-it-to-life" helpers: a white steam vent and a dark
//! furnace-smoke emitter, plus two spatial-audio patches — a rhythmic engine
//! chug for the tower and pump house and a boiler hiss for the foundry.
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
    EmitterShape, Fp, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignPuffConfig,
    SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A brisk jet of white steam venting upward — the release valve of the cog
/// tower or pump house.
pub(super) fn steam_vent(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.2),
            height: Fp(0.3),
        },
        rate: 14.0,
        burst: 0,
        max: 80,
        life: (1.2, 2.6),
        speed: (1.2, 2.4),
        gravity: -0.08,
        accel: [0.05, 0.5, 0.0],
        drag: 0.5,
        size: (0.2, 1.2),
        start_color: [0.92, 0.92, 0.90, 0.5],
        end_color: [0.94, 0.94, 0.92, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x57EA_0000) as u32,
            color_base: Fp3([0.95, 0.95, 0.93]),
            color_shadow: Fp3([0.72, 0.72, 0.70]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A dark column of sooty smoke rolling up off a chimney — the foundry in
/// full blast.
pub(super) fn furnace_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.26),
            height: Fp(0.4),
        },
        rate: 9.0,
        burst: 0,
        max: 80,
        life: (2.5, 5.0),
        speed: (0.6, 1.4),
        gravity: -0.05,
        accel: [0.2, 0.3, 0.0],
        drag: 0.6,
        size: (0.4, 1.8),
        start_color: [0.26, 0.24, 0.22, 0.55],
        end_color: [0.34, 0.32, 0.30, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x500F_0000) as u32,
            color_base: Fp3([0.30, 0.28, 0.26]),
            color_shadow: Fp3([0.14, 0.13, 0.12]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A rhythmic engine chug — a low piston tone pumped by a steady square-ish
/// LFO, the beat of a working beam engine.
pub(super) fn engine_chug() -> SovereignAudioConfig {
    let piston = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 82.0,
            phase_offset: 0.0,
            amplitude: 0.3,
        }),
    );
    // Steady pump — the chug rhythm.
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 2.4,
            shape: LfoShape::Sine,
            depth: 0.85,
            offset: 0.15,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let chug = GraphNode {
        id: NodeId(2),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    let mix = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.7 }),
        inputs: mix_in,
    };
    patch(vec![piston, lfo, chug, mix], NodeId(3))
}

/// A steady boiler hiss — high band-passed noise over a low rumble, the
/// pressure bleed of the foundry furnace.
pub(super) fn boiler_hiss() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.4 }));
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 3200.0,
            q: 1.5,
        }),
        inputs: bp_in,
    };
    let mut hiss_in = std::collections::BTreeMap::new();
    hiss_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    let hiss = GraphNode {
        id: NodeId(2),
        kind: NodeKind::Gain(Gain { gain: 0.3 }),
        inputs: hiss_in,
    };
    let rumble = node(
        3,
        NodeKind::Sine(SineOsc {
            freq_hz: 64.0,
            phase_offset: 0.0,
            amplitude: 0.16,
        }),
    );
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(2)),
            Connection::from_node(NodeId(3)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.6 }),
        inputs: mix_in,
    };
    patch(vec![noise, bp, hiss, rumble, mix], NodeId(4))
}
