//! AncientClassical "bring-it-to-life" helpers: small nested particle
//! emitters and a spatial-audio patch for the kit's one firelit element,
//! the [`brazier`](super::brazier) — a low altar flame, drifting embers,
//! and a fire crackle.
//!
//! Particle emitters are returned as [`Generator`] nodes positioned in the
//! prop's world frame, so they drop straight into an
//! [`assemble`](super::super::util::assemble) list. Counts stay small. The
//! audio patch returns a [`SovereignAudioConfig`] to assign to a node's
//! `audio` field; the world compiler plays it spatially at that node.

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeGraph, NodeId,
    NodeKind, SineOsc, WhiteNoise,
};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SimulationSpace, SovereignAudioConfig, SovereignFlameConfig, SovereignSparkConfig,
    SovereignTextureConfig, TextureFilter, TransformData,
};

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

/// A low altar flame licking up out of the brazier bowl.
pub(super) fn brazier_flame(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.3),
            height: Fp(0.25),
        },
        rate: 18.0,
        max: 80,
        life: (0.5, 1.2),
        speed: (0.6, 1.4),
        gravity: -0.1,
        accel: [0.0, 0.3, 0.0],
        drag: 0.3,
        size: (0.32, 0.0),
        start_color: [1.0, 0.72, 0.24, 1.0],
        end_color: [0.80, 0.20, 0.05, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Flame(SovereignFlameConfig {
            seed: (seed ^ 0x00F1_A3E0) as u32,
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Glowing embers drifting up off the coals into the warm air.
pub(super) fn brazier_embers(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.22) },
        rate: 5.0,
        max: 44,
        life: (1.0, 2.4),
        speed: (0.5, 1.4),
        gravity: -0.22,
        accel: [0.04, 0.36, 0.0],
        drag: 0.22,
        size: (0.05, 0.0),
        start_color: [1.0, 0.80, 0.38, 1.0],
        end_color: [0.9, 0.30, 0.06, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Spark(SovereignSparkConfig {
            seed: (seed ^ 0x0E_3BE0) as u32,
            points: 4,
            color_core: Fp3([1.0, 0.95, 0.7]),
            color_tip: Fp3([1.0, 0.5, 0.12]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

fn node(id: u32, kind: NodeKind) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        kind,
        inputs: std::collections::BTreeMap::new(),
    }
}

/// A warm, irregular fire crackle — band-passed noise pulsed by a slow LFO
/// over a low ember rumble. The voice of the brazier coals.
pub(super) fn fire_crackle() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.55 }));
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 6.0,
            shape: LfoShape::Sine,
            depth: 0.8,
            offset: 0.18,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1600.0,
            q: 2.0,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let crackle = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let rumble = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 68.0,
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
    patch(vec![noise, lfo, bp, crackle, rumble, mix], NodeId(5))
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}
