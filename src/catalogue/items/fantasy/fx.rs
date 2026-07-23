//! High-Fantasy "bring-it-to-life" helpers: drifting mana motes and arcane
//! sparkles, plus two spatial-audio patches — an ethereal arcane hum for the
//! wizard tower and a crystal shimmer for the shrine.
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
    EmitterShape, Fp, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignPuffConfig,
    SovereignSoftDiscConfig, SovereignSparkConfig, SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// Slow-rising motes of teal mana, glowing softly as they drift up — the
/// ambient enchantment of the arcane quarter.
pub(super) fn mana_motes(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([2.5, 0.6, 2.5]),
        },
        rate: 7.0,
        burst: 0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.2, 0.6),
        gravity: -0.06,
        accel: [0.0, 0.2, 0.0],
        drag: 0.4,
        size: (0.12, 0.0),
        start_color: [0.4, 1.0, 0.85, 0.9],
        end_color: [0.4, 0.9, 1.0, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
            seed: (seed ^ 0x0A1A_0F00) as u32,
            color_core: Fp3([0.7, 1.0, 0.95]),
            color_halo: Fp3([0.2, 0.7, 0.9]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Fine arcane sparkles whirling close around an orb or crystal — the
/// crackle of bound magic.
pub(super) fn arcane_sparkle(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.5) },
        rate: 10.0,
        burst: 0,
        max: 50,
        life: (0.6, 1.4),
        speed: (0.3, 0.9),
        gravity: -0.02,
        accel: [0.0, 0.1, 0.0],
        drag: 0.3,
        size: (0.1, 0.0),
        start_color: [0.85, 0.7, 1.0, 1.0],
        end_color: [0.5, 0.3, 1.0, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Spark(SovereignSparkConfig {
            seed: (seed ^ 0x0A5C_0F00) as u32,
            points: 5,
            color_core: Fp3([1.0, 0.95, 1.0]),
            color_tip: Fp3([0.7, 0.4, 1.0]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Wood-smoke curling up from a hearth chimney — a soft grey plume rising and
/// leaning off on the breeze, the hedge-witch's fire kept in.
pub(super) fn chimney_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.26),
            height: Fp(0.35),
        },
        rate: 8.0,
        burst: 0,
        max: 72,
        life: (2.6, 5.2),
        speed: (0.35, 0.9),
        gravity: -0.05,
        accel: [0.08, 0.24, 0.0],
        drag: 0.6,
        size: (0.28, 1.35),
        start_color: [0.58, 0.57, 0.54, 0.34],
        end_color: [0.68, 0.67, 0.64, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0A5C_5304) as u32,
            color_base: Fp3([0.66, 0.65, 0.62]),
            color_shadow: Fp3([0.44, 0.43, 0.41]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// An ethereal arcane hum — two close-detuned sines beating slowly under a
/// gentle tremolo, the resonance of bound magic.
pub(super) fn arcane_hum() -> SovereignAudioConfig {
    let a = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 196.0,
            phase_offset: 0.0,
            amplitude: 0.16,
        }),
    );
    let b = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 199.0,
            phase_offset: 0.0,
            amplitude: 0.16,
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
            rate_hz: 0.3,
            shape: LfoShape::Sine,
            depth: 0.4,
            offset: 0.6,
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

/// A high crystal shimmer — a bright sine ringing under a quick tremolo, the
/// singing of the shrine crystal.
pub(super) fn crystal_shimmer() -> SovereignAudioConfig {
    let tone = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 880.0,
            phase_offset: 0.0,
            amplitude: 0.12,
        }),
    );
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 5.0,
            shape: LfoShape::Sine,
            depth: 0.6,
            offset: 0.4,
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
