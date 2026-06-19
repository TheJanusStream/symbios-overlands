//! Medieval "bring-it-to-life" helpers: small nested particle emitters
//! and a spatial-audio patch that the kit's structures hang on signature
//! elements — sooty smoke and leaping sparks off the blacksmith's forge,
//! a thin curl of woodsmoke from the hovel's roof hole, and a fire crackle
//! on the forge hearth.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. The audio patch returns a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeGraph, NodeId,
    NodeKind, SineOsc, WhiteNoise,
};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SimulationSpace, SovereignAudioConfig, SovereignFlameConfig, SovereignPuffConfig,
    SovereignSparkConfig, SovereignTextureConfig, TextureFilter, TransformData,
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

/// A dark, lazy column of sooty woodsmoke rising and spreading from the
/// forge chimney.
pub(super) fn forge_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.26),
            height: Fp(0.4),
        },
        rate: 9.0,
        max: 80,
        life: (2.5, 5.0),
        speed: (0.4, 1.0),
        gravity: -0.05,
        accel: [0.12, 0.25, 0.0],
        drag: 0.6,
        size: (0.3, 1.5),
        start_color: [0.34, 0.33, 0.31, 0.45],
        end_color: [0.52, 0.51, 0.50, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0057_0E00) as u32,
            color_base: Fp3([0.40, 0.39, 0.37]),
            color_shadow: Fp3([0.20, 0.19, 0.18]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Glowing sparks struck off hot iron and carried up on the forge heat.
pub(super) fn forge_sparks(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.18) },
        rate: 6.0,
        max: 48,
        life: (0.5, 1.4),
        speed: (0.7, 1.7),
        gravity: -0.18,
        accel: [0.05, 0.35, 0.0],
        drag: 0.25,
        size: (0.05, 0.0),
        start_color: [1.0, 0.82, 0.40, 1.0],
        end_color: [0.92, 0.30, 0.06, 0.0],
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

/// A short licking flame for the forge mouth — small, low, additive.
pub(super) fn forge_flame(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.3),
            height: Fp(0.2),
        },
        rate: 16.0,
        max: 60,
        life: (0.4, 1.0),
        speed: (0.5, 1.1),
        gravity: -0.08,
        accel: [0.0, 0.25, 0.0],
        drag: 0.3,
        size: (0.28, 0.0),
        start_color: [1.0, 0.70, 0.22, 1.0],
        end_color: [0.80, 0.18, 0.05, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Flame(SovereignFlameConfig {
            seed: (seed ^ 0x00F1_A3E0) as u32,
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A thin grey-blue curl of woodsmoke seeping from a cottage roof hole.
pub(super) fn hearth_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.24),
            height: Fp(0.3),
        },
        rate: 6.0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.35, 0.9),
        gravity: -0.05,
        accel: [0.1, 0.22, 0.0],
        drag: 0.6,
        size: (0.25, 1.2),
        start_color: [0.60, 0.61, 0.64, 0.32],
        end_color: [0.70, 0.71, 0.74, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x00A1_7E00) as u32,
            color_base: Fp3([0.68, 0.69, 0.72]),
            color_shadow: Fp3([0.40, 0.41, 0.44]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

// ---------------------------------------------------------------------------
// Spatial audio patch
// ---------------------------------------------------------------------------

fn node(id: u32, kind: NodeKind) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        kind,
        inputs: std::collections::BTreeMap::new(),
    }
}

/// A warm, irregular fire crackle — band-passed noise pulsed by a slow LFO
/// over a low ember rumble. The voice of the forge hearth.
pub(super) fn fire_crackle() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.6 }));
    // Slow uneven pulse so the crackle comes in bursts, not a steady hiss.
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 6.5,
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
            center_hz: 1700.0,
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
    // Low ember rumble under the crackle.
    let rumble = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 72.0,
            phase_offset: 0.0,
            amplitude: 0.18,
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
