//! Space-Outpost "bring-it-to-life" helpers: a thin drift of regolith dust,
//! and two spatial-audio patches — a steady reactor hum for the habitat dome
//! and a pulsing radio static for the comms dish.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{
    BiquadBandpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind, Reverb, SineOsc,
    TriangleOsc, WhiteNoise,
};

use crate::catalogue::items::fx::{Emitter, node, patch, wired};
use crate::pds::{
    EmitterShape, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignPuffConfig,
    SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A thin veil of rust-grey regolith dust skating low across the ground on
/// the thin wind — the restless surface of the outpost world.
pub(super) fn regolith_dust(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([4.0, 0.15, 4.0]),
        },
        rate: 7.0,
        burst: 0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.4, 1.0),
        gravity: 0.02,
        accel: [0.5, 0.04, 0.1],
        drag: 0.6,
        size: (0.2, 1.0),
        start_color: [0.62, 0.50, 0.44, 0.2],
        end_color: [0.66, 0.54, 0.48, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x5EA0_0700) as u32,
            color_base: Fp3([0.68, 0.56, 0.50]),
            color_shadow: Fp3([0.44, 0.34, 0.30]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A steady reactor hum — two stacked sine partials under a faint tremolo,
/// the life-support plant of the habitat.
pub(super) fn reactor_hum() -> SovereignAudioConfig {
    let low = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 110.0,
            phase_offset: 0.0,
            amplitude: 0.2,
        }),
    );
    let harm = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 220.0,
            phase_offset: 0.0,
            amplitude: 0.08,
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
            rate_hz: 0.7,
            shape: LfoShape::Sine,
            depth: 0.25,
            offset: 0.7,
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
    patch(vec![low, harm, mix, lfo, vca], NodeId(4))
}

/// A pulsing radio static — mid band-passed noise gated by a slow uneven LFO,
/// the comms dish listening to the void.
pub(super) fn comms_static() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.4 }));
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1800.0,
            q: 1.2,
        }),
        inputs: bp_in,
    };
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 3.0,
            shape: LfoShape::Sine,
            depth: 0.7,
            offset: 0.25,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let pulse = GraphNode {
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
    patch(vec![noise, bp, lfo, pulse, mix], NodeId(4))
}

// ---------------------------------------------------------------------------
// Spatial audio (#1347)
// ---------------------------------------------------------------------------

/// A landing beacon's ping: a pure tone and its fifth struck once a second
/// and decaying to silence, with a short tail off the pad.
pub(super) fn beacon_ping() -> SovereignAudioConfig {
    let tone = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 1760.0,
            phase_offset: 0.0,
            amplitude: 0.14,
        }),
    );
    let fifth = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 2640.0,
            phase_offset: 0.0,
            amplitude: 0.05,
        }),
    );
    // Falling sawtooth with offset equal to depth, applied twice: a strike
    // and a quadratic fall to silence before the next.
    let decay = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 1.0,
            shape: LfoShape::Saw,
            depth: -0.5,
            offset: 0.5,
        }),
    );
    let struck = wired(
        3,
        NodeKind::Gain(Gain { gain: 0.0 }),
        &[("in", &[0, 1]), ("gain", &[2])],
    );
    let ping = wired(
        4,
        NodeKind::Gain(Gain { gain: 0.0 }),
        &[("in", &[3]), ("gain", &[2])],
    );
    let pad = wired(
        5,
        NodeKind::Reverb(Reverb {
            room_size: 0.3,
            damping: 0.5,
            mix: 0.2,
        }),
        &[("in", &[4])],
    );
    patch(vec![tone, fifth, decay, struck, ping, pad], NodeId(5))
}

/// An airlock holding pressure: the thin hiss of a seal that is never quite
/// tight, wavering to a random level five times a second, over the low note
/// of the vent stack.
pub(super) fn seal_hiss() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.35 }));
    let seal = wired(
        1,
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 3800.0,
            q: 1.5,
        }),
        &[("in", &[0])],
    );
    let waver = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 5.0,
            shape: LfoShape::Random,
            depth: 0.06,
            offset: 0.0,
        }),
    );
    let hiss = wired(
        3,
        NodeKind::Gain(Gain { gain: 0.18 }),
        &[("in", &[1]), ("gain", &[2])],
    );
    let stack = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 90.0,
            phase_offset: 0.0,
            amplitude: 0.03,
        }),
    );
    let mix = wired(5, NodeKind::Gain(Gain { gain: 0.8 }), &[("in", &[3, 4])]);
    patch(vec![noise, seal, waver, hiss, stack, mix], NodeId(5))
}

/// A hydroponic bay's circulation: a low pump stroking twice a second and
/// the nutrient feed trickling through the racks.
pub(super) fn hydro_pump() -> SovereignAudioConfig {
    let pump = node(
        0,
        NodeKind::Triangle(TriangleOsc {
            freq_hz: 45.0,
            amplitude: 0.12,
            ..Default::default()
        }),
    );
    let stroke = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 2.0,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.5,
        }),
    );
    let stroking = wired(
        2,
        NodeKind::Gain(Gain { gain: 0.0 }),
        &[("in", &[0]), ("gain", &[1])],
    );
    let noise = node(3, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.3 }));
    let feed = wired(
        4,
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1400.0,
            q: 1.1,
        }),
        &[("in", &[3])],
    );
    let trickle = wired(5, NodeKind::Gain(Gain { gain: 0.25 }), &[("in", &[4])]);
    let mix = wired(6, NodeKind::Gain(Gain { gain: 0.8 }), &[("in", &[2, 5])]);
    patch(
        vec![pump, stroke, stroking, noise, feed, trickle, mix],
        NodeId(6),
    )
}
