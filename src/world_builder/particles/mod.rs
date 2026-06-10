//! Hand-rolled CPU + ECS particle emitter for
//! [`crate::pds::GeneratorKind::ParticleSystem`].
//! WASM-friendly: no GPU compute, no `bevy_hanabi` dependency. Each
//! particle is a Bevy entity with a billboarded / velocity-aligned quad
//! mesh, ticked by three systems each frame:
//!
//! 1. [`update_emitter_motion`] — refreshes the per-emitter velocity
//!    used by velocity-inheritance. Tries avian3d `LinearVelocity` on
//!    the emitter or its nearest `RigidBody` ancestor first; falls back
//!    to a numerical derivative of the emitter's `GlobalTransform`.
//! 2. [`tick_emitter_spawn`] — drains the per-second emit accumulator,
//!    samples spawn positions from the emitter shape, samples initial
//!    velocity, applies inherit_velocity, and spawns particle entities
//!    up to the per-emitter `max_particles` cap.
//! 3. [`tick_particles`] — ages particles, integrates position with
//!    gravity / acceleration / linear drag, fades size and colour,
//!    optionally collides against terrain / water / arbitrary
//!    colliders, and despawns at end-of-life. Also re-orients the quad
//!    each frame (billboard vs velocity-aligned).
//!
//! Determinism: every emitter carries a `ChaCha8Rng` seeded by the
//! variant's `seed`. Spawning the same emitter on two peers with the
//! same dt path produces the same particle stream.
//!
//! ## Material strategy
//!
//! Particles do **not** own materials. Each emitter lazily bakes one
//! [`EmitterMaterialRamp`] — a small set of shared `StandardMaterial`
//! handles quantising the start→end colour fade — and every particle
//! swaps between those shared handles as it ages (an asset-id copy,
//! exactly like the atlas-frame mesh swap). The previous design gave
//! every particle its own material and mutated it every frame, which
//! both defeated Bevy's automatic mesh batching (one draw call per
//! particle) and re-prepared every particle's material bind group every
//! frame; see [`ramp`] for the details.
//!
//! ## Sub-module map
//!
//! * [`ramp`] — [`EmitterMaterialRamp`] + the quantised-fade builder.
//! * [`atlas`] — shared quad mesh, the per-frame atlas-mesh cache, and
//!   frame-index math.
//! * [`spawn`] — emitter spawn helpers, the record→snapshot translator,
//!   the per-frame emission system and per-particle spawn.
//! * [`tick`] — emitter motion tracking, particle integration /
//!   fade / collision / orientation, and end-of-life despawn.

mod atlas;
mod ramp;
mod spawn;
mod tick;

pub use atlas::{ParticleAtlasMeshes, ParticleQuadMesh};
pub use ramp::EmitterMaterialRamp;
pub(super) use spawn::{snapshot_from_record, spawn_particle_emitter_entity};
pub use spawn::{spawn_particle_emitter, tick_emitter_spawn};
pub use tick::{tick_particles, update_emitter_motion};

use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;

use crate::pds::{
    AnimationFrameMode, EmitterShape, ParticleBlendMode, SignSource, SimulationSpace, TextureAtlas,
    TextureFilter,
};

/// Per-emitter parameter snapshot. Stored on the emitter entity at
/// spawn time and consulted by [`tick_emitter_spawn`] / [`tick_particles`]
/// each frame so the systems don't need to chase back into the room
/// record. A patch to the room record despawns and re-spawns the
/// emitter, naturally refreshing this component.
#[derive(Component, Clone)]
pub struct ParticleEmitter {
    pub shape: EmitterShape,
    pub rate_per_second: f32,
    pub burst_count: u32,
    pub max_particles: u32,
    pub looping: bool,
    pub duration: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub speed_min: f32,
    pub speed_max: f32,
    pub gravity_multiplier: f32,
    pub acceleration: Vec3,
    pub linear_drag: f32,
    pub start_size: f32,
    pub end_size: f32,
    pub start_color: LinearRgba,
    pub end_color: LinearRgba,
    pub blend_mode: ParticleBlendMode,
    pub billboard: bool,
    pub simulation_space: SimulationSpace,
    pub inherit_velocity: f32,
    pub collide_terrain: bool,
    pub collide_water: bool,
    pub collide_colliders: bool,
    pub bounce: f32,
    pub friction: f32,
    /// Optional textured-particle source. `None` keeps v1 behaviour
    /// (solid coloured quads); `Some(_)` triggers a fetch via
    /// [`super::image_cache::BlobImageCache`] when the emitter's
    /// material ramp is baked, patching each ramp material with the
    /// resulting [`Handle<Image>`] — every particle shares those.
    pub texture: Option<SignSource>,
    /// Atlas dimensions when the loaded texture is a sprite sheet.
    /// `None` uses the whole image as a single frame.
    pub texture_atlas: Option<TextureAtlas>,
    /// Frame-cycling mode for atlas particles.
    pub frame_mode: AnimationFrameMode,
    /// Sampler filter applied when the image lands in
    /// `Assets<Image>` — `Linear` for soft sprites, `Nearest` for
    /// pixel-art looks.
    pub texture_filter: TextureFilter,
}

/// Mutable per-emitter scratch state: the spawn accumulator, the
/// deterministic RNG, the loop-cycle clock, and the running alive-count
/// used by the spawn cap. Separate from [`ParticleEmitter`] so the
/// snapshot can stay cheaply cloneable while this component owns the
/// non-Clone RNG.
#[derive(Component)]
pub struct EmitterState {
    pub age: f32,
    pub cycle_age: f32,
    pub spawn_accumulator: f32,
    pub rng: ChaCha8Rng,
    pub alive_count: u32,
}

/// Per-frame velocity tracker. Refreshed by [`update_emitter_motion`]
/// each frame so [`tick_emitter_spawn`] can apply
/// `inherit_velocity * world_velocity` at spawn time. The
/// `last_translation` field drives the numerical-derivative fallback
/// when the emitter (or its ancestors) carries no `LinearVelocity`.
#[derive(Component)]
pub struct EmitterMotionTracker {
    pub last_translation: Vec3,
    pub world_velocity: Vec3,
    pub initialised: bool,
}

impl Default for EmitterMotionTracker {
    fn default() -> Self {
        Self {
            last_translation: Vec3::ZERO,
            world_velocity: Vec3::ZERO,
            initialised: false,
        }
    }
}

/// Per-particle component. Position / orientation live on the entity's
/// own `Transform`; this component owns the parametric state the tick
/// system integrates each frame.
#[derive(Component)]
pub struct Particle {
    pub age: f32,
    pub lifetime: f32,
    pub velocity: Vec3,
    /// Source emitter, used by the tick system to read the parametric
    /// curves (start/end size + colour) from the snapshot. Resolved
    /// each frame so a despawned emitter no longer feeds its surviving
    /// particles' fade — they keep ticking off their existing state
    /// until they age out.
    pub emitter: Entity,
    /// Atlas dimensions baked at spawn — `(rows, cols)`. `None` when
    /// the particle has no atlas (single-frame texture or no texture
    /// at all). Stored on the particle so the tick system can resolve
    /// atlas mesh handles even after the source emitter despawns.
    pub atlas_dim: Option<(u32, u32)>,
    /// Frame index baked at spawn (`Still` / `RandomFrame`) or the
    /// current frame for `OverLifetime` particles. The tick system
    /// updates this every frame for `OverLifetime` and swaps the
    /// `Mesh3d` handle when the index changes.
    pub frame_index: u32,
    /// Per-particle frame mode snapshot. Captured at spawn so a
    /// despawned emitter doesn't strand the particle's animation
    /// behaviour.
    pub frame_mode: AnimationFrameMode,
    /// Index of the [`EmitterMaterialRamp`] bucket this particle's
    /// `MeshMaterial3d` currently points at. The tick system swaps the
    /// handle when the lifetime fraction crosses into the next bucket —
    /// an asset-id copy, never a material mutation.
    pub ramp_index: usize,
}

/// Marker so the room compiler's cleanup query reaches every emitter
/// alongside its surviving particle entities. Particles get [`super::RoomEntity`]
/// directly (in world simulation space) or are parented under the
/// emitter (local simulation space) so they ride the chassis's despawn
/// in either case.
#[derive(Component)]
pub struct ParticleEmitterMarker;

// Private to the particles tree; child modules reach it as
// `super::lerp_unit`.
fn lerp_unit(t: f32, a: f32, b: f32) -> f32 {
    a + (b - a) * t
}
