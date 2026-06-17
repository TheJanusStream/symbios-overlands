//! Cyberpunk "bring-it-to-life" helpers: small nested particle emitters and
//! spatial-audio patches that the kit's structures hang on signature
//! elements — a steam vent at the megatower base, sparks off a failing neon
//! sign, an electrical hum on the arcade board.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame, so
//! they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts are deliberately small (signature, not spectacle) to stay
//! light on the CPU particle budget. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position
//! (`world_builder::compile::dispatch`).

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape,
    NodeGraph, NodeId, NodeKind, SawtoothOsc, SineOsc, TriangleOsc, WhiteNoise,
};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SimulationSpace, SovereignAudioConfig, SovereignFlameConfig, SovereignPuffConfig,
    SovereignSoftDiscConfig, SovereignSparkConfig, SovereignTextureConfig, TextureFilter,
    TransformData,
};

// ---------------------------------------------------------------------------
// Particle emitters
// ---------------------------------------------------------------------------

/// The varying parameters of a small ambient emitter; the rest are filled
/// with shared defaults by [`Emitter::at`].
struct Emitter {
    shape: EmitterShape,
    rate: f32,
    burst: u32,
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
                burst_count: self.burst,
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

/// A bursty shower of hot sparks falling under gravity — failing neon,
/// shorting terminals, severed cables.
pub(super) fn spark_burst(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.1) },
        rate: 3.0,
        burst: 6,
        max: 48,
        life: (0.3, 0.8),
        speed: (1.0, 3.0),
        gravity: 1.0,
        accel: [0.0, 0.0, 0.0],
        drag: 0.2,
        size: (0.05, 0.0),
        start_color: [1.0, 0.92, 0.6, 1.0],
        end_color: [1.0, 0.4, 0.1, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Spark(SovereignSparkConfig {
            seed: (seed ^ 0x5A5A_5A5A) as u32,
            points: 4,
            color_core: Fp3([1.0, 1.0, 0.85]),
            color_tip: Fp3([1.0, 0.55, 0.15]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A slow column of pale steam / exhaust rising and spreading — vents,
/// cooling stacks at the base of big structures.
pub(super) fn steam_vent(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.3),
            height: Fp(0.4),
        },
        rate: 7.0,
        burst: 0,
        max: 60,
        life: (2.0, 4.0),
        speed: (0.4, 1.0),
        gravity: -0.04,
        accel: [0.0, 0.2, 0.0],
        drag: 0.6,
        size: (0.25, 1.0),
        start_color: [0.70, 0.72, 0.75, 0.30],
        end_color: [0.80, 0.82, 0.85, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0057_EA00) as u32,
            color_base: Fp3([0.80, 0.82, 0.85]),
            color_shadow: Fp3([0.55, 0.57, 0.60]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A small licking flame with embers — a brazier / burn barrel warming the
/// undercity.
pub(super) fn brazier_flame(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.35),
            height: Fp(0.2),
        },
        rate: 18.0,
        burst: 0,
        max: 80,
        life: (0.5, 1.2),
        speed: (0.5, 1.2),
        gravity: -0.08,
        accel: [0.0, 0.0, 0.0],
        drag: 0.3,
        size: (0.18, 0.0),
        start_color: [1.0, 0.70, 0.20, 1.0],
        end_color: [0.8, 0.15, 0.05, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Flame(SovereignFlameConfig {
            seed: (seed ^ 0x00F1_A3E0) as u32,
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Faint glowing motes drifting upward in `color` — data static around a
/// spire, holographic shimmer off a billboard.
pub(super) fn rising_motes(pos: [f32; 3], color: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([1.2, 0.3, 1.2]),
        },
        rate: 6.0,
        burst: 0,
        max: 48,
        life: (2.5, 5.0),
        speed: (0.1, 0.4),
        gravity: -0.02,
        accel: [0.0, 0.1, 0.0],
        drag: 0.5,
        size: (0.07, 0.02),
        start_color: [color[0], color[1], color[2], 0.9],
        end_color: [color[0], color[1], color[2], 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
            seed: (seed ^ 0x0060_7E50) as u32,
            color_core: Fp3(color),
            color_halo: Fp3(color),
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

/// A buzzing, faintly flickering neon-tube / sign hum.
pub(super) fn neon_buzz() -> SovereignAudioConfig {
    let saw = node(
        0,
        NodeKind::Sawtooth(SawtoothOsc {
            freq_hz: 120.0,
            polarity: Default::default(),
            amplitude: 0.45,
            anti_alias: Default::default(),
        }),
    );
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 9.0,
            shape: LfoShape::Sine,
            depth: 0.25,
            offset: 0.7,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 900.0,
            q: 2.0,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let vca = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![saw, lfo, bp, vca], NodeId(3))
}

/// A low, ominous transformer / mains hum — a 60 Hz fundamental and its
/// octave, darkened by a lowpass.
pub(super) fn transformer_hum() -> SovereignAudioConfig {
    let s1 = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 60.0,
            phase_offset: 0.0,
            amplitude: 0.4,
        }),
    );
    let s2 = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 120.0,
            phase_offset: 0.0,
            amplitude: 0.2,
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
        kind: NodeKind::Gain(Gain { gain: 0.6 }),
        inputs: mix_in,
    };
    let mut lp_in = std::collections::BTreeMap::new();
    lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    let lp = GraphNode {
        id: NodeId(3),
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 400.0,
            q: 1.0,
        }),
        inputs: lp_in,
    };
    patch(vec![s1, s2, mix, lp], NodeId(3))
}

/// Intermittent high electrical crackle / arcing — sparking cables and
/// shorting terminals.
pub(super) fn electric_crackle() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.7 }));
    // Fast sine mostly below the gain floor, peaking into brief crackle
    // bursts.
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 5.0,
            shape: LfoShape::Sine,
            depth: 0.85,
            offset: 0.12,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 3000.0,
            q: 3.0,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let vca = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![noise, lfo, bp, vca], NodeId(3))
}

/// A steady mid whir with a fast tremolo — a hovering drone's rotors.
pub(super) fn drone_whir() -> SovereignAudioConfig {
    let tri = node(
        0,
        NodeKind::Triangle(TriangleOsc {
            freq_hz: 240.0,
            amplitude: 0.4,
            anti_alias: Default::default(),
        }),
    );
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 18.0,
            shape: LfoShape::Sine,
            depth: 0.3,
            offset: 0.7,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1200.0,
            q: 1.5,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let vca = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![tri, lfo, bp, vca], NodeId(3))
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}
