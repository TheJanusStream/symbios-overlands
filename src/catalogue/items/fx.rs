//! Shared "bring-it-to-life" fx kit for catalogue entries: the small
//! ambient [`Emitter`] particle builder and the spatial-audio [`node`] /
//! [`patch`] helpers that every theme's `fx.rs` hangs its signature smoke,
//! flame, ember and crackle on.
//!
//! These were copy-pasted verbatim into each per-theme `fx.rs`; the copies
//! had drifted (several kits ran an older [`Emitter`] missing the `burst`
//! field), so they live here once. A theme's `fx.rs` keeps only its own
//! emitter *recipes* (the per-prop colours, rates and shapes) and builds
//! them through this shared [`Emitter`].
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame, so
//! they drop straight into an [`assemble`](super::util::assemble) list.
//! Counts stay small (signature, not spectacle) and well within the particle
//! sanitiser's bounds. The audio helpers wrap a node graph into a
//! mute-defaulted [`SovereignAudioConfig`] the world compiler plays spatially
//! at the owning node's position.

use bevy_symbios_audio::{AudioPatch, GraphNode, NodeGraph, NodeId, NodeKind};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SimulationSpace, SovereignAudioConfig, SovereignTextureConfig, TextureFilter, TransformData,
};

/// The varying parameters of a small ambient emitter; the rest are filled
/// with shared defaults by [`Emitter::at`].
///
/// Used by the per-theme catalogue `fx.rs` kits and by the avatar FX
/// builder ([`crate::pds::avatar::default_visuals`]), hence `pub(crate)`.
pub(crate) struct Emitter {
    pub(crate) shape: EmitterShape,
    pub(crate) rate: f32,
    pub(crate) burst: u32,
    pub(crate) max: u32,
    pub(crate) life: (f32, f32),
    pub(crate) speed: (f32, f32),
    pub(crate) gravity: f32,
    pub(crate) accel: [f32; 3],
    pub(crate) drag: f32,
    pub(crate) size: (f32, f32),
    pub(crate) start_color: [f32; 4],
    pub(crate) end_color: [f32; 4],
    pub(crate) blend: ParticleBlendMode,
    pub(crate) sprite: SovereignTextureConfig,
}

impl Emitter {
    /// Finish the emitter into a positioned [`Generator`] node, seeded for
    /// determinism.
    pub(crate) fn at(self, pos: [f32; 3], seed: u64) -> Generator {
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

/// A graph node with the given id and kind and no inputs wired yet.
pub(crate) fn node(id: u32, kind: NodeKind) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        kind,
        inputs: std::collections::BTreeMap::new(),
    }
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
pub(crate) fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}
