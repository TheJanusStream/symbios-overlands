//! Pirate-theme "bring-it-to-life" helpers: three particle emitters — the
//! spent-powder smoke that hangs over the battery's embrasures, the wood smoke
//! off the tavern's hearth, and the cold green witchfire of the cursed
//! register — and three spatial-audio patches: the swell working against the
//! harbour wall, the creak of a block and tackle, and the witchfire's hiss.
//!
//! The three audio patches are built to be heard *against each other*. The
//! swell is nearly all mass, the creak is a narrow resonance with an object
//! behind it, and the hiss is the swell with its mass taken away — so the
//! cursed register sounds like what is left of the working one rather than like
//! a different place.
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

/// The cursed register's voice: a thin cold hiss with nothing warm under it.
///
/// Deliberately this file's [`harbour_swell`] with the **rumble removed** and
/// the band moved up two and a half octaves, because that is the whole idea.
/// The working harbour's voice is mostly mass — a 44 Hz sine carrying a low
/// wash, which is what a body of water heaving against masonry sounds like. Take
/// the mass away and leave only the top of the spectrum and the same patch
/// becomes something with no body at all. A listener does not need to be told
/// the difference; they have heard the tavern and the battery, and this is what
/// is left of them.
///
/// The tremble is faster than the swell (0.7 Hz against 0.18) and shallower, so
/// it flickers where the harbour breathes — a flame's rhythm rather than the
/// sea's. Gain is low: this is a sound you notice having heard, and a hiss loud
/// enough to announce itself would read as a leak.
pub(super) fn witchfire_hiss() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.3 }));
    // A flame's flicker, not a wave's period.
    let flicker = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.7,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.45,
        }),
    );
    // Wide `q` and high centre: air, not a resonating body. The creak's narrow
    // peak is what turns noise into an object, so the opposite of that is what
    // turns it back into nothing.
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1850.0,
            q: 0.55,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let hiss = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    // No rumble node here at all. That absence IS the patch.
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    let mix = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.4 }),
        inputs: mix_in,
    };
    patch(vec![noise, flicker, bp, hiss, mix], NodeId(4))
}

/// Cold green flame licking off a wreck, a cage or a coin spill — the visual
/// half of [`witchfire_hiss`].
///
/// Small, slow and *upward*, and much sparser than either smoke: witchfire is
/// meant to read as a few tongues of light rather than a plume. Gravity is
/// negative so the motes rise and hang; a witchfire that fell would read as
/// sparks, which is a warm idea.
pub(super) fn witchfire(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.2) },
        // A fifth of the battery's rate. Witchfire is a few tongues of light,
        // and the moment it becomes a plume it reads as a bonfire — which is
        // warm, and warm is the one thing this register cannot be.
        rate: 4.0,
        burst: 0,
        max: 24,
        life: (1.5, 2.8),
        speed: (0.18, 0.5),
        gravity: -0.16,
        accel: [0.0, 0.1, 0.0],
        drag: 1.2,
        size: (0.16, 0.36),
        start_color: [0.24, 0.9, 0.48, 0.62],
        end_color: [0.05, 0.28, 0.15, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0C1D_F17E) as u32,
            color_base: Fp3([0.34, 0.94, 0.54]),
            color_shadow: Fp3([0.06, 0.32, 0.18]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}
