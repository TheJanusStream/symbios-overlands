//! Suburban "bring-it-to-life" helpers: a lawn-sprinkler mist and the
//! birdsong of a quiet street, hung on the kit's signature elements.
//!
//! Particle emitters are returned as positioned [`Generator`] nodes that
//! drop straight into an [`assemble`](super::super::util::assemble) list;
//! counts stay small and within the particle sanitiser's bounds. Audio
//! patches return a [`SovereignAudioConfig`] for a node's `audio` field.

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeGraph, NodeId,
    NodeKind, SineOsc, WhiteNoise,
};

use crate::catalogue::items::fx::{node, patch, wired};
use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    ParticleParams, SimulationSpace, SovereignAudioConfig, SovereignSoftDiscConfig,
    SovereignTextureConfig, TextureFilter, TransformData,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// A fine fan of sprinkler mist drifting up off the lawn and settling.
pub(super) fn sprinkler_mist(pos: [f32; 3], seed: u64) -> Generator {
    Generator {
        kind: GeneratorKind::ParticleSystem(Box::new(ParticleParams {
            emitter_shape: EmitterShape::Cone {
                half_angle: Fp(0.7),
                height: Fp(0.2),
            },
            rate_per_second: Fp(14.0),
            burst_count: 0,
            max_particles: 90,
            looping: true,
            duration: Fp(2.0),
            lifetime_min: Fp(1.2),
            lifetime_max: Fp(2.4),
            speed_min: Fp(1.2),
            speed_max: Fp(2.6),
            gravity_multiplier: Fp(0.5),
            acceleration: Fp3([0.0, 0.0, 0.0]),
            linear_drag: Fp(0.5),
            start_size: Fp(0.05),
            end_size: Fp(0.12),
            start_color: Fp4([0.80, 0.88, 0.95, 0.35]),
            end_color: Fp4([0.85, 0.92, 1.0, 0.0]),
            blend_mode: ParticleBlendMode::Alpha,
            billboard: true,
            simulation_space: SimulationSpace::World,
            inherit_velocity: Fp(0.0),
            collide_terrain: false,
            collide_water: false,
            collide_colliders: false,
            bounce: Fp(0.3),
            friction: Fp(0.5),
            seed,
            texture: None,
            texture_atlas: None,
            frame_mode: AnimationFrameMode::RandomFrame,
            texture_filter: TextureFilter::Linear,
            procedural_texture: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
                seed: (seed ^ 0x05F1_0E00) as u32,
                color_core: Fp3([0.88, 0.94, 1.0]),
                color_halo: Fp3([0.7, 0.82, 0.95]),
                ..Default::default()
            }),
        })),
        transform: TransformData {
            translation: Fp3(pos),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        children: Vec::new(),
        audio: SovereignAudioConfig::None,
    }
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

/// One chirp voice: a high sine pulsed on and off by an LFO, so it sounds as
/// intermittent calls rather than a steady tone. `base` is the first node id;
/// returns the three nodes and the id of the voice's output gain. `swing` is
/// the LFO's depth and offset at once, so each call falls to silence between
/// bursts and never inverts (#1348).
fn chirp(base: u32, freq: f32, rate: f32, swing: f32) -> (Vec<GraphNode>, NodeId) {
    let osc = node(
        base,
        NodeKind::Sine(SineOsc {
            freq_hz: freq,
            phase_offset: 0.0,
            amplitude: 0.4,
        }),
    );
    let lfo = node(
        base + 1,
        NodeKind::Lfo(Lfo {
            rate_hz: rate,
            shape: LfoShape::Sine,
            depth: swing,
            offset: swing,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(base))]);
    vca_in.insert(
        "gain".to_string(),
        vec![Connection::from_node(NodeId(base + 1))],
    );
    let vca = GraphNode {
        id: NodeId(base + 2),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    (vec![osc, lfo, vca], NodeId(base + 2))
}

/// Two warbling high chirp voices at different rates - the birdsong of a
/// quiet suburban street, mixed down low.
pub(super) fn birdsong() -> SovereignAudioConfig {
    let (mut a, a_out) = chirp(0, 3100.0, 11.0, 0.555);
    let (b, b_out) = chirp(3, 2550.0, 7.0, 0.554);
    a.extend(b);
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![Connection::from_node(a_out), Connection::from_node(b_out)],
    );
    let mix = GraphNode {
        id: NodeId(6),
        kind: NodeKind::Gain(Gain { gain: 0.4 }),
        inputs: mix_in,
    };
    a.push(mix);
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph {
            nodes: a,
            output: NodeId(6),
        },
    })
}

// ---------------------------------------------------------------------------
// Spatial audio (#1347)
// ---------------------------------------------------------------------------

/// A swing's chains creaking as the empty seats stir: a squeak of iron on
/// iron at the shackles, swelling once a second and fading to nothing
/// between.
pub(super) fn swing_creak() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.9 }));
    let shackle = wired(
        1,
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1150.0,
            q: 7.0,
        }),
        &[("in", &[0])],
    );
    // Offset equals depth, so the swell touches silence and never inverts;
    // applied twice it lingers there, a creak with a pause, not a whine.
    let swell = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 1.0,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.5,
        }),
    );
    let stirring = wired(
        3,
        NodeKind::Gain(Gain { gain: 0.0 }),
        &[("in", &[1]), ("gain", &[2])],
    );
    let creak = wired(
        4,
        NodeKind::Gain(Gain { gain: 0.0 }),
        &[("in", &[3]), ("gain", &[2])],
    );
    patch(vec![noise, shackle, swell, stirring, creak], NodeId(4))
}
