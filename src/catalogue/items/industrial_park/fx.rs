//! Industrial-Park "bring-it-to-life" helpers: smokestack smoke and a cooling-
//! tower steam plume, plus the low hum of machinery and a steam hiss, hung on
//! the kit's signature elements.
//!
//! Particle emitters are returned as positioned [`Generator`] nodes that
//! drop straight into an [`assemble`](super::super::util::assemble) list;
//! counts stay small and within the particle sanitiser's bounds. Audio
//! patches return a [`SovereignAudioConfig`] for a node's `audio` field.

use bevy_symbios_audio::{
    BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind,
    SineOsc, WhiteNoise,
};

use crate::catalogue::items::fx::{Emitter, node, patch};
use crate::pds::{
    EmitterShape, Fp, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignPuffConfig,
    SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A column of dark smoke pouring from a smokestack and drifting on the wind.
pub(super) fn stack_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.25),
            height: Fp(0.5),
        },
        burst: 0,
        gravity: -0.04,
        drag: 0.6,
        blend: ParticleBlendMode::Alpha,
        rate: 12.0,
        max: 90,
        life: (3.0, 6.0),
        speed: (0.8, 1.6),
        accel: [0.5, 0.3, 0.0],
        size: (0.6, 2.4),
        start_color: [0.30, 0.29, 0.27, 0.55],
        end_color: [0.42, 0.41, 0.39, 0.0],
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0540_0E00) as u32,
            color_base: Fp3([0.34, 0.33, 0.31]),
            color_shadow: Fp3([0.16, 0.16, 0.15]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A fat white plume of cooling-tower steam billowing up and out.
pub(super) fn cooling_steam(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.25),
            height: Fp(0.5),
        },
        burst: 0,
        gravity: -0.04,
        drag: 0.6,
        blend: ParticleBlendMode::Alpha,
        rate: 16.0,
        max: 120,
        life: (3.5, 7.0),
        speed: (1.0, 2.2),
        accel: [0.3, 0.5, 0.0],
        size: (1.2, 4.0),
        start_color: [0.90, 0.91, 0.93, 0.5],
        end_color: [0.95, 0.96, 0.98, 0.0],
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x000C_0015) as u32,
            color_base: Fp3([0.93, 0.94, 0.96]),
            color_shadow: Fp3([0.66, 0.68, 0.72]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A small jet of white steam from a relief valve or vent stack.
pub(super) fn stack_vent(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.25),
            height: Fp(0.5),
        },
        burst: 0,
        gravity: -0.04,
        drag: 0.6,
        blend: ParticleBlendMode::Alpha,
        rate: 9.0,
        max: 55,
        life: (1.5, 3.0),
        speed: (0.8, 1.6),
        accel: [0.15, 0.3, 0.0],
        size: (0.2, 0.9),
        start_color: [0.88, 0.89, 0.91, 0.4],
        end_color: [0.92, 0.93, 0.95, 0.0],
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0057_E700) as u32,
            color_base: Fp3([0.90, 0.91, 0.93]),
            color_shadow: Fp3([0.62, 0.64, 0.66]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A heavy machinery hum — a low fundamental and its octave with a touch of
/// motor noise, darkened by a lowpass: the drone of a working plant.
pub(super) fn machine_hum() -> SovereignAudioConfig {
    let s1 = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 50.0,
            phase_offset: 0.0,
            amplitude: 0.45,
        }),
    );
    let s2 = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 100.0,
            phase_offset: 0.0,
            amplitude: 0.22,
        }),
    );
    let noise = node(2, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.2 }));
    let bp = {
        let mut bp_in = std::collections::BTreeMap::new();
        bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
        GraphNode {
            id: NodeId(3),
            kind: NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: 300.0,
                q: 1.2,
            }),
            inputs: bp_in,
        }
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(1)),
            Connection::from_node(NodeId(3)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.55 }),
        inputs: mix_in,
    };
    let mut lp_in = std::collections::BTreeMap::new();
    lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(4))]);
    let lp = GraphNode {
        id: NodeId(5),
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 500.0,
            q: 0.9,
        }),
        inputs: lp_in,
    };
    patch(vec![s1, s2, noise, bp, mix, lp], NodeId(5))
}

/// A steady steam hiss — high band-passed noise swelling slowly, venting from
/// a pressure relief on a tank or pipe.
pub(super) fn steam_hiss() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let bp = {
        let mut bp_in = std::collections::BTreeMap::new();
        bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: 4200.0,
                q: 1.2,
            }),
            inputs: bp_in,
        }
    };
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.3,
            shape: LfoShape::Sine,
            depth: 0.4,
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
    patch(vec![noise, bp, lfo, vca], NodeId(3))
}
