//! Sports / Recreation "bring-it-to-life" helpers: a faint dust drifting
//! over the pitch, and two spatial-audio patches — a swelling crowd murmur
//! for the stadium bowl and a low tannoy hum for the scoreboard.
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame,
//! so they drop straight into an [`assemble`](super::super::util::assemble)
//! list. Counts stay small (signature, not spectacle) and well within the
//! particle sanitiser's bounds. Audio patches return a
//! [`SovereignAudioConfig`] to assign to a node's `audio` field; the world
//! compiler plays it spatially at that node's position.

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape,
    NodeGraph, NodeId, NodeKind, SineOsc, WhiteNoise,
};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SimulationSpace, SovereignAudioConfig, SovereignPuffConfig, SovereignTextureConfig,
    TextureFilter, TransformData,
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

/// A faint drift of pale dust over the pitch — chalk and grit lifted off the
/// field on the breeze.
pub(super) fn field_dust(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Box {
            half_extents: Fp3([6.0, 0.4, 4.0]),
        },
        rate: 6.0,
        burst: 0,
        max: 60,
        life: (2.5, 5.0),
        speed: (0.2, 0.7),
        gravity: 0.03,
        accel: [0.3, 0.05, 0.0],
        drag: 0.6,
        size: (0.2, 0.9),
        start_color: [0.82, 0.82, 0.76, 0.18],
        end_color: [0.84, 0.84, 0.78, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x05F0_0700) as u32,
            color_base: Fp3([0.86, 0.86, 0.80]),
            color_shadow: Fp3([0.60, 0.60, 0.54]),
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

/// A swelling crowd murmur — vocal-band noise rising and falling on a slow
/// LFO, the breathing hubbub of a full stand.
pub(super) fn crowd_murmur() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.6 }));
    // Vocal band so the noise reads as voices, not hiss.
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(1),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 700.0,
            q: 0.7,
        }),
        inputs: bp_in,
    };
    // Slow swell as the crowd surges and settles.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.45,
            shape: LfoShape::Sine,
            depth: 0.6,
            offset: 0.4,
        }),
    );
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let murmur = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    let mix = GraphNode {
        id: NodeId(4),
        kind: NodeKind::Gain(Gain { gain: 0.7 }),
        inputs: mix_in,
    };
    patch(vec![noise, bp, lfo, murmur, mix], NodeId(4))
}

/// A low tannoy hum — a mains tone under a lowpassed hiss, the idle PA at
/// the scoreboard.
pub(super) fn tannoy_hum() -> SovereignAudioConfig {
    let hum = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 100.0,
            phase_offset: 0.0,
            amplitude: 0.14,
        }),
    );
    let noise = node(1, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.25 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
        GraphNode {
            id: NodeId(2),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 220.0,
                q: 1.0,
            }),
            inputs: lp_in,
        }
    };
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(2)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.5 }),
        inputs: mix_in,
    };
    patch(vec![hum, noise, lp, mix], NodeId(3))
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}
