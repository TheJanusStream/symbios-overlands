//! Gothic-Horror "bring-it-to-life" helpers: a creeping ground mist and two
//! spatial-audio patches — a cold hollow wind for the bell tower and an eerie
//! ghostly drone for the cathedral.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{
    BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind, SineOsc,
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

/// A low, slow creep of grey graveyard mist hugging the ground — the cold
/// breath of the necropolis.
pub(super) fn ground_mist(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([4.0, 0.2, 4.0]),
        },
        rate: 6.0,
        burst: 0,
        max: 60,
        life: (4.0, 8.0),
        speed: (0.1, 0.4),
        gravity: 0.0,
        accel: [0.12, 0.0, 0.05],
        drag: 0.7,
        size: (0.8, 2.4),
        start_color: [0.62, 0.64, 0.66, 0.22],
        end_color: [0.66, 0.68, 0.70, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x60F0_0700) as u32,
            color_base: Fp3([0.66, 0.68, 0.70]),
            color_shadow: Fp3([0.40, 0.42, 0.46]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A cold hollow wind — band-limited noise breathing slowly through a lowpass,
/// keening through the bell-tower louvers.
pub(super) fn cold_wind() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 300.0,
                q: 1.4,
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

/// An eerie ghostly drone — two close-detuned low sines beating slowly under a
/// gloomy tremolo, the dread that hangs in the nave.
pub(super) fn ghostly_drone() -> SovereignAudioConfig {
    let a = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 73.0,
            phase_offset: 0.0,
            amplitude: 0.2,
        }),
    );
    let b = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 74.4,
            phase_offset: 0.0,
            amplitude: 0.18,
        }),
    );
    // A faint sickly fifth above.
    let c = node(
        2,
        NodeKind::Sine(SineOsc {
            freq_hz: 110.0,
            phase_offset: 0.0,
            amplitude: 0.07,
        }),
    );
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(1)),
            Connection::from_node(NodeId(2)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 1.0 }),
        inputs: mix_in,
    };
    let lfo = node(
        4,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.25,
            shape: LfoShape::Sine,
            depth: 0.4,
            offset: 0.55,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(4))]);
    let vca = GraphNode {
        id: NodeId(5),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![a, b, c, mix, lfo, vca], NodeId(5))
}
