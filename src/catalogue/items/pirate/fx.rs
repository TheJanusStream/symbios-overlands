//! Pirate-theme "bring-it-to-life" helpers: the spent-powder smoke that hangs
//! over the battery's embrasures, and three spatial-audio patches — the swell
//! working against the harbour wall, the creak of a block and tackle, and the
//! cold hiss of witchfire for the cursed register.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame, so
//! they drop straight into an [`assemble`](super::super::util::assemble) list
//! — or through [`attach`](super::super::util::attach) if they are added after
//! the root is built, which is the trap #1010 closed. Counts stay small
//! (signature, not spectacle) and well within the particle sanitiser's bounds.
//! Audio patches return a [`SovereignAudioConfig`] to assign to a node's
//! `audio` field; the world compiler plays it spatially at that node's
//! position.

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

/// Spent powder smoke drifting out of an embrasure and away on the wind — the
/// battery's signature.
///
/// Deliberately slow, thin and *lateral*: this is smoke that has already been
/// fired and is leaving, not a discharge. A fast upward plume would read as a
/// chimney, which is the one thing a gun deck must not look like. The
/// acceleration carries it out over the water (`-Z`, the hero side) while
/// gravity is left slightly negative so it lifts as it goes.
pub(super) fn powder_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.4),
            height: Fp(0.25),
        },
        rate: 7.0,
        burst: 0,
        max: 60,
        life: (2.4, 4.6),
        speed: (0.35, 0.9),
        gravity: -0.05,
        accel: [0.12, 0.14, -0.4],
        drag: 0.8,
        size: (0.3, 1.6),
        start_color: [0.78, 0.77, 0.72, 0.42],
        end_color: [0.70, 0.70, 0.68, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0B0D_5E12) as u32,
            color_base: Fp3([0.80, 0.79, 0.75]),
            color_shadow: Fp3([0.48, 0.48, 0.46]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Wood smoke leaving a chimney — the tavern's hearth, seen from the lane.
///
/// Slower, thinner and warmer than [`powder_smoke`]: a hearth draws steadily
/// where a gun discharges, so this rises rather than drifting off sideways,
/// and it carries a faint brown rather than the powder's grey. Small enough
/// to be a signature; a chimney that pumps like a factory reads as one.
pub(super) fn hearth_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.22),
            height: Fp(0.2),
        },
        rate: 5.0,
        burst: 0,
        max: 44,
        life: (2.8, 5.0),
        speed: (0.5, 1.0),
        gravity: -0.12,
        accel: [0.16, 0.3, -0.1],
        drag: 0.72,
        size: (0.22, 1.3),
        start_color: [0.62, 0.58, 0.52, 0.36],
        end_color: [0.58, 0.56, 0.54, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0EA7_1100) as u32,
            color_base: Fp3([0.66, 0.62, 0.56]),
            color_shadow: Fp3([0.36, 0.33, 0.30]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// The swell working against a harbour wall — band-passed noise swelled by a
/// very slow LFO over a deep rumble.
///
/// The same *shape* as `coastal_resort::surf_wash` and tuned to the opposite
/// end of it: lower centre frequency, slower swell, more rumble. A resort
/// hears the surf break on a beach; a harbour hears it heave against masonry,
/// which is a duller and much heavier sound.
pub(super) fn harbour_swell() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    // Slower than the resort's 0.3 Hz: a long harbour swell, not a beach wave.
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.18,
            shape: LfoShape::Sine,
            depth: 0.75,
            offset: 0.25,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 330.0,
            q: 0.7,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let wash = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    // Deep hull-rumble under the wash — the mass of water the wall is holding.
    let rumble = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 44.0,
            phase_offset: 0.0,
            amplitude: 0.22,
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
        kind: NodeKind::Gain(Gain { gain: 0.72 }),
        inputs: mix_in,
    };
    patch(vec![noise, lfo, bp, wash, rumble, mix], NodeId(5))
}

/// Rope through a block, and timber taking a load — a narrow resonant band of
/// noise, gated by a slow ramp so it groans and releases rather than hissing.
///
/// The voice of the careening slip, the signal mast and the capstan: anything
/// with a fall of rope on it. The narrow `q` is what turns noise into a
/// *creak*; widen it and the patch becomes wind.
pub(super) fn rigging_creak() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.34 }));
    // A sharp resonant peak — the sound is the sheave, not the air.
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 880.0,
            q: 7.5,
        }),
        inputs: bp_in,
    };
    // A slow saw ramp: the load comes on gradually and lets go at the turn,
    // which is what makes it read as a rope rendering rather than a tone.
    let ramp = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.13,
            shape: LfoShape::Saw,
            depth: 0.85,
            offset: 0.1,
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
    // Timber groan an octave and a half below the sheave.
    let groan = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 118.0,
            phase_offset: 0.0,
            amplitude: 0.1,
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
        kind: NodeKind::Gain(Gain { gain: 0.55 }),
        inputs: mix_in,
    };
    patch(vec![noise, bp, ramp, vca, groan, mix], NodeId(5))
}

// The cursed register's voice — `witchfire_hiss`, a thin cold hiss with
// nothing warm under it — arrives with the entries that need it (#1023). It is
// this file's `harbour_swell` with the rumble removed and the band moved up
// two and a half octaves, which is the whole idea: where the working harbour
// has mass, the cursed one has none.
