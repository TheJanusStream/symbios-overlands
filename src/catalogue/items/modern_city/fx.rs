//! Modern-City "bring-it-to-life" helpers: a rooftop/street steam vent and
//! the low hums of the city — distant traffic and rooftop air handling — that
//! the kit's structures hang on signature elements.
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
    max: u32,
    life: (f32, f32),
    speed: (f32, f32),
    gravity: f32,
    accel: [f32; 3],
    drag: f32,
    size: (f32, f32),
    start_color: [f32; 4],
    end_color: [f32; 4],
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

/// A pale column of steam rising and spreading — a rooftop AC unit, a street
/// grate, a vent stack.
pub(super) fn vent_steam(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.3),
            height: Fp(0.4),
        },
        rate: 7.0,
        max: 60,
        life: (2.0, 4.0),
        speed: (0.4, 1.0),
        gravity: -0.04,
        accel: [0.1, 0.22, 0.0],
        drag: 0.6,
        size: (0.25, 1.1),
        start_color: [0.78, 0.80, 0.82, 0.28],
        end_color: [0.85, 0.87, 0.89, 0.0],
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0057_EA00) as u32,
            color_base: Fp3([0.82, 0.84, 0.86]),
            color_shadow: Fp3([0.58, 0.60, 0.62]),
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

/// A low, broadband traffic hum — band-limited road rumble over a deep
/// fundamental, swelling slowly as traffic ebbs and flows.
pub(super) fn traffic_hum() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let lp = {
        let mut lp_in = std::collections::BTreeMap::new();
        lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        GraphNode {
            id: NodeId(1),
            kind: NodeKind::BiquadLowpass(BiquadLowpass {
                cutoff_hz: 260.0,
                q: 1.1,
            }),
            inputs: lp_in,
        }
    };
    // Slow swell.
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.18,
            shape: LfoShape::Sine,
            depth: 0.35,
            offset: 0.5,
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
    // Deep fundamental under the rumble.
    let sub = node(
        4,
        NodeKind::Sine(SineOsc {
            freq_hz: 48.0,
            phase_offset: 0.0,
            amplitude: 0.22,
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
    patch(vec![noise, lp, lfo, vca, sub, mix], NodeId(5))
}

/// A steady mechanical air-handler hum — a mid sine with a touch of
/// band-passed fan noise, the drone of a rooftop AC unit.
pub(super) fn ac_hum() -> SovereignAudioConfig {
    let tone = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 110.0,
            phase_offset: 0.0,
            amplitude: 0.3,
        }),
    );
    let noise = node(1, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.3 }));
    let bp = {
        let mut bp_in = std::collections::BTreeMap::new();
        bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(1))]);
        GraphNode {
            id: NodeId(2),
            kind: NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: 700.0,
                q: 1.5,
            }),
            inputs: bp_in,
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
    let mut lp_in = std::collections::BTreeMap::new();
    lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
    let lp = GraphNode {
        id: NodeId(4),
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 1200.0,
            q: 0.8,
        }),
        inputs: lp_in,
    };
    patch(vec![tone, noise, bp, mix, lp], NodeId(4))
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}
