//! Feudal-Japan "bring-it-to-life" helpers: small nested particle emitters
//! and spatial-audio patches that the kit's structures hang on signature
//! elements — blossom shed off a pagoda, an incense wisp by the lantern,
//! hearth smoke from a farmhouse, a deep temple-bell ring, and the trickle
//! of a garden water basin.
//!
//! Particle emitters are returned as [`Generator`] nodes positioned in the
//! prop's world frame so they drop straight into an
//! [`assemble`](super::super::util::assemble) list; counts stay small and
//! well within the particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field.

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape,
    NodeGraph, NodeId, NodeKind, SineOsc, WhiteNoise,
};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SimulationSpace, SovereignAudioConfig, SovereignPetalConfig, SovereignPuffConfig,
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

/// A gentle local fall of cherry petals shed from a structure's eaves —
/// signature blossom, tighter and closer than the room-wide accent.
pub(super) fn falling_petals(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([2.5, 0.3, 2.5]),
        },
        rate: 6.0,
        burst: 0,
        max: 60,
        life: (4.0, 8.0),
        speed: (0.2, 0.6),
        gravity: 0.02,
        accel: [0.25, 0.0, 0.1],
        drag: 0.7,
        size: (0.14, 0.14),
        start_color: [1.0, 0.80, 0.86, 0.95],
        end_color: [1.0, 0.72, 0.80, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Petal(SovereignPetalConfig {
            seed: (seed ^ 0x0BE7_A100) as u32,
            color_base: Fp3([0.99, 0.78, 0.86]),
            color_edge: Fp3([0.95, 0.58, 0.72]),
            color_throat: Fp3([1.0, 0.90, 0.70]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A thin thread of incense smoke curling up — a censer by the lantern or
/// the tea-garden gate.
pub(super) fn incense_wisp(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.03) },
        rate: 5.0,
        burst: 0,
        max: 40,
        life: (2.5, 4.5),
        speed: (0.1, 0.35),
        gravity: -0.04,
        accel: [0.08, 0.18, 0.0],
        drag: 0.5,
        size: (0.08, 0.5),
        start_color: [0.70, 0.72, 0.74, 0.28],
        end_color: [0.78, 0.80, 0.82, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0117_CE00) as u32,
            color_base: Fp3([0.74, 0.76, 0.78]),
            color_shadow: Fp3([0.5, 0.52, 0.54]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A lazy grey-blue column of hearth smoke rising and spreading — the
/// farmhouse hearth venting through its roof.
pub(super) fn hearth_smoke(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.28),
            height: Fp(0.4),
        },
        rate: 8.0,
        burst: 0,
        max: 70,
        life: (2.5, 5.0),
        speed: (0.4, 1.0),
        gravity: -0.05,
        accel: [0.1, 0.25, 0.0],
        drag: 0.6,
        size: (0.3, 1.4),
        start_color: [0.62, 0.63, 0.66, 0.35],
        end_color: [0.72, 0.73, 0.76, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0057_0E00) as u32,
            color_base: Fp3([0.70, 0.71, 0.74]),
            color_shadow: Fp3([0.42, 0.43, 0.46]),
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

/// A deep bronze temple-bell ring — a low fundamental and its octave under
/// a slow tremolo, darkened by a lowpass: the sustained hum of a struck
/// bonshō.
pub(super) fn temple_bell_ring() -> SovereignAudioConfig {
    let s1 = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 82.0,
            phase_offset: 0.0,
            amplitude: 0.4,
        }),
    );
    let s2 = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 164.0,
            phase_offset: 0.0,
            amplitude: 0.22,
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
    // Slow swell so the bell breathes.
    let lfo = node(
        3,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.4,
            shape: LfoShape::Sine,
            depth: 0.4,
            offset: 0.5,
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
    let mut lp_in = std::collections::BTreeMap::new();
    lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(4))]);
    let lp = GraphNode {
        id: NodeId(5),
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 600.0,
            q: 1.0,
        }),
        inputs: lp_in,
    };
    patch(vec![s1, s2, mix, lfo, vca, lp], NodeId(5))
}

/// The soft trickle of a garden water basin — band-passed noise pulsed by a
/// gentle LFO, kept low under a lowpass. A tsukubai by the tea house.
pub(super) fn water_basin() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.4 }));
    let bp = {
        let mut bp_in = std::collections::BTreeMap::new();
        bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: 1100.0,
                q: 1.4,
            }),
            inputs: bp_in,
        }
    };
    // Uneven gentle pulse so the water laps rather than hisses.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 2.5,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.3,
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
    let mut lp_in = std::collections::BTreeMap::new();
    lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    let lp = GraphNode {
        id: NodeId(4),
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 2200.0,
            q: 0.8,
        }),
        inputs: lp_in,
    };
    patch(vec![noise, bp, lfo, vca, lp], NodeId(4))
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}
