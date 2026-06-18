//! Rural "bring-it-to-life" helpers: hearth smoke and drifting chaff, plus
//! the creak of the windmill and an evening of crickets, hung on the kit's
//! signature elements.
//!
//! Particle emitters are returned as positioned [`Generator`] nodes that
//! drop straight into an [`assemble`](super::super::util::assemble) list;
//! counts stay small and within the particle sanitiser's bounds. Audio
//! patches return a [`SovereignAudioConfig`] for a node's `audio` field.

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape,
    NodeGraph, NodeId, NodeKind, SineOsc, WhiteNoise,
};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SimulationSpace, SovereignAudioConfig, SovereignPuffConfig, SovereignSoftDiscConfig,
    SovereignTextureConfig, TextureFilter, TransformData,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// The varying parameters of a small ambient emitter; the rest are filled
/// with shared defaults by [`Emitter::at`].
struct Emitter {
    shape: EmitterShape,
    rate: f32,
    max: u32,
    life: (f32, f32),
    speed: (f32, f32),
    gravity: f32,
    accel: [f32; 3],
    drag: f32,
    size: (f32, f32),
    start_color: [f32; 4],
    end_color: [f32; 4],
    blend: ParticleBlendMode,
    sprite: SovereignTextureConfig,
}

impl Emitter {
    /// Finish the emitter into a positioned [`Generator`] node, seeded for
    /// determinism.
    fn at(self, pos: [f32; 3], seed: u64) -> Generator {
        Generator {
            kind: GeneratorKind::ParticleSystem {
                emitter_shape: self.shape,
                rate_per_second: Fp(self.rate),
                burst_count: 0,
                max_particles: self.max,
                looping: true,
                duration: Fp(2.0),
                lifetime_min: Fp(self.life.0),
                lifetime_max: Fp(self.life.1),
                speed_min: Fp(self.speed.0),
                speed_max: Fp(self.speed.1),
                gravity_multiplier: Fp(self.gravity),
                acceleration: Fp3(self.accel),
                linear_drag: Fp(self.drag),
                start_size: Fp(self.size.0),
                end_size: Fp(self.size.1),
                start_color: Fp4(self.start_color),
                end_color: Fp4(self.end_color),
                blend_mode: self.blend,
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
                procedural_texture: self.sprite,
            },
            transform: TransformData {
                translation: Fp3(pos),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
            audio: SovereignAudioConfig::None,
        }
    }
}

/// A lazy grey column of hearth smoke from a chimney.
pub(super) fn chimney_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.26),
            height: Fp(0.4),
        },
        rate: 7.0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.4, 1.0),
        gravity: -0.05,
        accel: [0.12, 0.24, 0.0],
        drag: 0.6,
        size: (0.3, 1.4),
        start_color: [0.66, 0.65, 0.62, 0.32],
        end_color: [0.74, 0.73, 0.70, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0057_0E00) as u32,
            color_base: Fp3([0.70, 0.69, 0.66]),
            color_shadow: Fp3([0.44, 0.43, 0.40]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Golden chaff and hay dust drifting on the warm evening air off the
/// hayloft.
pub(super) fn chaff_drift(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([2.0, 0.6, 1.0]),
        },
        rate: 6.0,
        max: 50,
        life: (3.0, 6.0),
        speed: (0.15, 0.5),
        gravity: 0.01,
        accel: [0.2, 0.05, 0.0],
        drag: 0.6,
        size: (0.05, 0.03),
        start_color: [0.92, 0.78, 0.40, 0.85],
        end_color: [0.85, 0.66, 0.30, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
            seed: (seed ^ 0x0C4A_FF00) as u32,
            color_core: Fp3([1.0, 0.92, 0.6]),
            color_halo: Fp3([0.9, 0.74, 0.34]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patches
// ---------------------------------------------------------------------------

fn node(id: u32, kind: NodeKind) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        kind,
        inputs: std::collections::BTreeMap::new(),
    }
}

/// The slow groan and creak of a wind pump — a low band-passed noise rasp
/// swelling under a deep groan, turning lazily in the breeze.
pub(super) fn windmill_creak() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let bp = {
        let mut bp_in = std::collections::BTreeMap::new();
        bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: 520.0,
                q: 4.0,
            }),
            inputs: bp_in,
        }
    };
    // Slow uneven turn so the creak comes and goes.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.5,
            shape: LfoShape::Sine,
            depth: 0.85,
            offset: 0.1,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let creak = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    // Low groan of the timber tower under it.
    let groan = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 65.0,
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
        kind: NodeKind::Gain(Gain { gain: 0.6 }),
        inputs: mix_in,
    };
    let mut lp_in = std::collections::BTreeMap::new();
    lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(5))]);
    let lp = GraphNode {
        id: NodeId(6),
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 1400.0,
            q: 0.8,
        }),
        inputs: lp_in,
    };
    patch(vec![noise, bp, lfo, creak, groan, mix, lp], NodeId(6))
}

/// Evening crickets — two narrow high chirps pulsed at insect rates.
pub(super) fn crickets() -> SovereignAudioConfig {
    let voice = |base: u32, freq: f32, rate: f32| -> (Vec<GraphNode>, NodeId) {
        let osc = node(
            base,
            NodeKind::Sine(SineOsc {
                freq_hz: freq,
                phase_offset: 0.0,
                amplitude: 0.32,
            }),
        );
        let lfo = node(
            base + 1,
            NodeKind::Lfo(Lfo {
                rate_hz: rate,
                shape: LfoShape::Sine,
                depth: 0.97,
                offset: 0.02,
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
    };
    let (mut a, a_out) = voice(0, 4300.0, 14.0);
    let (b, b_out) = voice(3, 4650.0, 11.0);
    a.extend(b);
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![Connection::from_node(a_out), Connection::from_node(b_out)],
    );
    a.push(GraphNode {
        id: NodeId(6),
        kind: NodeKind::Gain(Gain { gain: 0.35 }),
        inputs: mix_in,
    });
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph {
            nodes: a,
            output: NodeId(6),
        },
    })
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}
