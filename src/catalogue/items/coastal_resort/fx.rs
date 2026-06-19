//! Coastal-Resort "bring-it-to-life" helpers: a fine sea-spray emitter the
//! pier hangs over the breaking water, and two spatial-audio patches — a
//! slow surf wash for the pier pilings and a soft sea breeze for the hotel
//! frontage.
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
    EmitterShape, Fp, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig, SovereignPuffConfig,
    SovereignTextureConfig,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A fine white veil of sea spray flung up off the pilings and blown
/// landward — the surf breaking under the pier's end.
pub(super) fn sea_mist(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.5),
            height: Fp(0.3),
        },
        rate: 10.0,
        burst: 0,
        max: 80,
        life: (1.6, 3.2),
        speed: (0.6, 1.4),
        gravity: 0.15,
        accel: [-0.35, 0.2, 0.0],
        drag: 0.7,
        size: (0.25, 1.1),
        start_color: [0.92, 0.95, 0.97, 0.4],
        end_color: [0.88, 0.92, 0.95, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x05EA_0F00) as u32,
            color_base: Fp3([0.94, 0.96, 0.98]),
            color_shadow: Fp3([0.66, 0.74, 0.80]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// A slow rolling surf wash — band-passed noise swelled by a very slow LFO
/// (the wave rhythm) over a low ocean rumble. The voice of the pier pilings.
pub(super) fn surf_wash() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.55 }));
    // Slow swell so the surf rolls in and recedes rather than hissing.
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.3,
            shape: LfoShape::Sine,
            depth: 0.7,
            offset: 0.3,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 520.0,
            q: 0.8,
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
    // Low ocean rumble under the wash.
    let rumble = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 58.0,
            phase_offset: 0.0,
            amplitude: 0.16,
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
        kind: NodeKind::Gain(Gain { gain: 0.7 }),
        inputs: mix_in,
    };
    patch(vec![noise, lfo, bp, wash, rumble, mix], NodeId(5))
}

/// A soft, airy sea breeze — band-limited noise breathing slowly through a
/// lowpass. The voice of the hotel frontage and the open promenade.
pub(super) fn sea_breeze() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.42 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 420.0,
                q: 1.0,
            }),
            inputs: lp_in,
        }
    };
    // Slow swell so the breeze rises and falls.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.2,
            shape: LfoShape::Sine,
            depth: 0.5,
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
    patch(vec![noise, lp, lfo, vca], NodeId(3))
}
