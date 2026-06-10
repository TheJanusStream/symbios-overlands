//! Emitter spawning + the per-frame emission system.

use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp3, Fp4, ParticleBlendMode, SignSource, SimulationSpace,
    TextureAtlas, TextureFilter,
};

use super::super::compile::SpawnCtx;
use super::atlas::{ParticleAtlasMeshes, ParticleQuadMesh};
use super::ramp::{EmitterMaterialRamp, build_emitter_ramp};
use super::{
    EmitterMotionTracker, EmitterState, Particle, ParticleEmitter, ParticleEmitterMarker, lerp_unit,
};

/// Spawn a ParticleSystem emitter entity. The entity carries the
/// parameter snapshot, the deterministic RNG, and the motion tracker;
/// its child particles are spawned per-frame by [`tick_emitter_spawn`].
pub(in super::super) fn spawn_particle_emitter_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    snapshot: ParticleEmitter,
    seed: u64,
    transform: Transform,
) -> Entity {
    // Room/world-builder emitters are room-owned unless we're compiling
    // a guest avatar (whose trail must survive room rebuilds).
    spawn_particle_emitter(
        ctx.commands,
        snapshot,
        seed,
        transform,
        !ctx.avatar_mode,
        ctx.placement_index,
    )
}

/// `Commands`-level emitter spawn — the `SpawnCtx`-free core, callable
/// from any system (e.g. the interaction-framework particle dispatcher,
/// which has `Commands` but no world-builder context). Spawns the
/// parameter snapshot + deterministic RNG + motion tracker; child
/// particles are produced per-frame by [`tick_emitter_spawn`], which
/// also bakes the emitter's [`EmitterMaterialRamp`] on first emission
/// (this function deliberately needs no `Assets` access).
///
/// `tag_room_entity` inserts [`super::super::RoomEntity`] so the
/// compile-pass cleanup sweeps the emitter on a room rebuild — pass
/// `false` for avatar-scoped / transient effect emitters that should
/// instead ride their own retirement (or their parent avatar's despawn).
///
/// `unit_owner` is the owning placement index for room-compiled
/// emitters ([`PlacementUnit::NONE`](super::super::PlacementUnit::NONE)
/// for runtime effect emitters) — it lets the incremental compiler's
/// flat unit sweep retire the emitter on a placement rebuild.
pub fn spawn_particle_emitter(
    commands: &mut Commands,
    snapshot: ParticleEmitter,
    seed: u64,
    transform: Transform,
    tag_room_entity: bool,
    unit_owner: usize,
) -> Entity {
    let rng = ChaCha8Rng::seed_from_u64(seed);

    let mut cmd = commands.spawn((
        snapshot,
        EmitterState {
            age: 0.0,
            cycle_age: 0.0,
            spawn_accumulator: 0.0,
            rng,
            alive_count: 0,
        },
        EmitterMotionTracker::default(),
        ParticleEmitterMarker,
        transform,
        Visibility::default(),
    ));
    if tag_room_entity {
        cmd.insert((
            super::super::RoomEntity,
            super::super::PlacementUnit(unit_owner),
        ));
    }
    cmd.id()
}

/// Translate a [`crate::pds::GeneratorKind::ParticleSystem`] payload
/// into a [`ParticleEmitter`] snapshot. Pulled out so the spawn arm in
/// `compile::spawn_generator` stays a one-liner.
#[allow(clippy::too_many_arguments)]
pub(in super::super) fn snapshot_from_record(
    emitter_shape: &EmitterShape,
    rate_per_second: f32,
    burst_count: u32,
    max_particles: u32,
    looping: bool,
    duration: f32,
    lifetime_min: f32,
    lifetime_max: f32,
    speed_min: f32,
    speed_max: f32,
    gravity_multiplier: f32,
    acceleration: &Fp3,
    linear_drag: f32,
    start_size: f32,
    end_size: f32,
    start_color: &Fp4,
    end_color: &Fp4,
    blend_mode: &ParticleBlendMode,
    billboard: bool,
    simulation_space: &SimulationSpace,
    inherit_velocity: f32,
    collide_terrain: bool,
    collide_water: bool,
    collide_colliders: bool,
    bounce: f32,
    friction: f32,
    texture: Option<SignSource>,
    texture_atlas: Option<TextureAtlas>,
    frame_mode: AnimationFrameMode,
    texture_filter: TextureFilter,
) -> ParticleEmitter {
    ParticleEmitter {
        shape: emitter_shape.clone(),
        rate_per_second,
        burst_count,
        max_particles,
        looping,
        duration,
        lifetime_min,
        lifetime_max,
        speed_min,
        speed_max,
        gravity_multiplier,
        acceleration: Vec3::from_array(acceleration.0),
        linear_drag,
        start_size,
        end_size,
        start_color: LinearRgba::new(
            start_color.0[0],
            start_color.0[1],
            start_color.0[2],
            start_color.0[3],
        ),
        end_color: LinearRgba::new(
            end_color.0[0],
            end_color.0[1],
            end_color.0[2],
            end_color.0[3],
        ),
        blend_mode: blend_mode.clone(),
        billboard,
        simulation_space: simulation_space.clone(),
        inherit_velocity,
        collide_terrain,
        collide_water,
        collide_colliders,
        bounce,
        friction,
        texture,
        texture_atlas,
        frame_mode,
        texture_filter,
    }
}

/// Drive each emitter's particle spawning. Each frame:
/// 1. age the emitter and the per-cycle clock; loop the cycle if
///    `looping`, otherwise stop emitting once `age >= duration`.
/// 2. add `dt * rate_per_second` to the spawn accumulator and spawn
///    `floor(accumulator)` particles, decrementing.
/// 3. on each cycle boundary fire `burst_count` particles at once.
/// 4. cap at `max_particles` (skip emit when full so we never exceed).
///
/// The emitter's [`EmitterMaterialRamp`] is baked lazily on the first
/// frame that actually emits: this system is the earliest point with
/// `Assets<StandardMaterial>` access on every emitter code path
/// ([`spawn_particle_emitter`] is deliberately `Commands`-only so the
/// interaction dispatcher can call it). The freshly-built ramp is used
/// directly this frame and inserted on the emitter for every later one.
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn tick_emitter_spawn(
    mut commands: Commands,
    time: Res<Time>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    quad_mesh: Res<ParticleQuadMesh>,
    mut atlas_meshes: ResMut<ParticleAtlasMeshes>,
    mut blob_image_cache: ResMut<super::super::image_cache::BlobImageCache>,
    mut emitters: Query<(
        Entity,
        &ParticleEmitter,
        &mut EmitterState,
        &EmitterMotionTracker,
        &GlobalTransform,
        Option<&EmitterMaterialRamp>,
        Has<super::super::RoomEntity>,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (entity, emitter, mut state, tracker, gxform, ramp, room_owned) in emitters.iter_mut() {
        let prev_cycle_age = state.cycle_age;
        state.age += dt;
        state.cycle_age += dt;

        // Cycle wrap. For non-looping emitters this is the once-only
        // "fire any remaining bursts after deactivation" guard; the
        // accumulator just stops growing.
        let mut cycle_wrapped = false;
        if state.cycle_age >= emitter.duration {
            if emitter.looping {
                state.cycle_age %= emitter.duration.max(1e-3);
                cycle_wrapped = true;
            } else {
                state.cycle_age = emitter.duration;
            }
        }
        // Initial burst on first activation: fires when cycle_age
        // crossed zero this frame (covers spawn-time + post-loop).
        let initial_burst = prev_cycle_age == 0.0;
        let active = emitter.looping || state.age <= emitter.duration;

        if !active {
            continue;
        }

        // Continuous emission accumulator.
        state.spawn_accumulator += dt * emitter.rate_per_second;
        let mut to_spawn = state.spawn_accumulator.floor() as u32;
        state.spawn_accumulator -= to_spawn as f32;

        if (initial_burst || cycle_wrapped) && emitter.burst_count > 0 {
            to_spawn = to_spawn.saturating_add(emitter.burst_count);
        }

        let budget = emitter.max_particles.saturating_sub(state.alive_count);
        let to_spawn = to_spawn.min(budget);
        if to_spawn == 0 {
            continue;
        }

        // Resolve the shared material ramp: reuse the baked component,
        // or bake it now (first emission) and pin it on the emitter for
        // subsequent frames. The local clone keeps this frame's spawns
        // working before the deferred insert lands.
        let ramp = match ramp {
            Some(r) => r.clone(),
            None => {
                let built = build_emitter_ramp(
                    &mut commands,
                    &mut std_materials,
                    &mut blob_image_cache,
                    emitter,
                );
                commands.entity(entity).insert(built.clone());
                built
            }
        };

        for _ in 0..to_spawn {
            spawn_one_particle(
                &mut commands,
                entity,
                emitter,
                &mut state,
                tracker,
                gxform,
                &ramp,
                &mut meshes,
                &mut atlas_meshes,
                &quad_mesh,
                room_owned,
            );
            state.alive_count += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_one_particle(
    commands: &mut Commands,
    emitter_entity: Entity,
    emitter: &ParticleEmitter,
    state: &mut EmitterState,
    tracker: &EmitterMotionTracker,
    emitter_gxform: &GlobalTransform,
    ramp: &EmitterMaterialRamp,
    meshes: &mut Assets<Mesh>,
    atlas_meshes: &mut ParticleAtlasMeshes,
    quad_mesh: &ParticleQuadMesh,
    emitter_room_owned: bool,
) {
    let (local_pos, mut local_dir) = sample_emitter_shape(&emitter.shape, &mut state.rng);
    if local_dir.length_squared() < 1e-6 {
        local_dir = Vec3::Y;
    }
    let speed = lerp_unit(
        unit_f32(&mut state.rng),
        emitter.speed_min,
        emitter.speed_max,
    );
    let lifetime = lerp_unit(
        unit_f32(&mut state.rng),
        emitter.lifetime_min,
        emitter.lifetime_max,
    );

    let local_velocity = local_dir.normalize_or_zero() * speed;
    let world_velocity = emitter_gxform.affine().transform_vector3(local_velocity)
        + emitter.inherit_velocity * tracker.world_velocity;

    // Materials are shared bucket handles from the emitter's ramp —
    // a fresh particle starts at bucket 0 (the start colour). Any
    // texture was registered against the ramp materials when the ramp
    // was baked, so there is nothing per-particle to fetch or patch.
    let material_handle = ramp.handle(0).clone();

    // Atlas state — `(rows, cols)` baked once at spawn from the
    // emitter's snapshot. `RandomFrame` draws an initial frame index
    // here; `Still` and `OverLifetime` start at 0 (OverLifetime
    // recomputes per frame in the tick system).
    let atlas_dim = emitter
        .texture_atlas
        .as_ref()
        .map(|a| (a.rows.max(1), a.cols.max(1)));
    let initial_frame = match emitter.frame_mode {
        AnimationFrameMode::RandomFrame => {
            let total = atlas_dim.map(|(r, c)| r * c).unwrap_or(1).max(1);
            (state.rng.next_u32() % total).min(total.saturating_sub(1))
        }
        _ => 0,
    };

    // Resolve the initial mesh handle: shared `quad_mesh` for the
    // no-atlas / no-texture path, atlas-frame mesh otherwise. Cells
    // get cached so a second particle with the same `(rows, cols,
    // frame)` reuses the existing handle.
    let mesh_handle = match atlas_dim {
        Some((rows, cols)) => atlas_meshes.get_or_create(meshes, rows, cols, initial_frame),
        None => quad_mesh.0.clone(),
    };

    let world_pos = emitter_gxform.affine().transform_point3(local_pos);
    let mut transform = Transform::from_translation(world_pos);
    transform.scale = Vec3::splat(emitter.start_size);

    let mut cmd = commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        transform,
        Particle {
            age: 0.0,
            lifetime,
            velocity: world_velocity,
            emitter: emitter_entity,
            atlas_dim,
            frame_index: initial_frame,
            frame_mode: emitter.frame_mode.clone(),
            ramp_index: 0,
        },
    ));

    match emitter.simulation_space {
        SimulationSpace::Local => {
            // Local space — re-parent under the emitter so the particle
            // moves with subsequent emitter transforms.
            cmd.insert(ChildOf(emitter_entity));
        }
        SimulationSpace::World | SimulationSpace::Unknown => {
            // World space — keep unparented. Tag with RoomEntity only
            // when the source emitter is itself room-owned, so the
            // compile-pass cleanup sweeps room exhaust/dust on rebuilds
            // without also wiping every guest avatar's vehicle trail
            // (which the emitter has skipped RoomEntity for in avatar
            // mode). Avatar-emitter particles ride out their natural
            // lifetime and despawn on age-out.
            if emitter_room_owned {
                cmd.insert(super::super::RoomEntity);
            }
        }
    }
}

/// Sample the emitter shape: returns `(local_position, default_dir)`.
/// The default direction is normalised by the caller, which handles the
/// edge case where the sample produces a zero vector.
fn sample_emitter_shape(shape: &EmitterShape, rng: &mut ChaCha8Rng) -> (Vec3, Vec3) {
    match shape {
        EmitterShape::Point => (Vec3::ZERO, Vec3::Y),
        EmitterShape::Sphere { radius } => {
            let v = sample_unit_sphere(rng) * radius.0;
            // Spawn anywhere inside the sphere; emit radially outward.
            let dir = if v.length_squared() > 1e-6 {
                v.normalize()
            } else {
                Vec3::Y
            };
            (v, dir)
        }
        EmitterShape::Box { half_extents } => {
            let v = Vec3::new(
                unit_signed(rng) * half_extents.0[0],
                unit_signed(rng) * half_extents.0[1],
                unit_signed(rng) * half_extents.0[2],
            );
            (v, Vec3::Y)
        }
        EmitterShape::Cone { half_angle, height } => {
            // Uniform sampling inside the cone volume (apex at origin,
            // axis +Y). Naïve linear sampling — `depth = U * height` and
            // `theta = U * half_angle` — clusters particles at the apex
            // and along the axis: cone volume scales as `depth³` (each
            // cross-section disk's area grows quadratically with depth)
            // and per-direction-cap surface scales as `(1 − cos θ)`, so
            // both axes need inverse-CDF sampling for a uniform fill.
            let depth = unit_f32(rng).cbrt() * height.0;
            let cos_max = half_angle.0.cos();
            let cos_t = 1.0 - unit_f32(rng) * (1.0 - cos_max);
            let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
            let phi = unit_f32(rng) * 2.0 * std::f32::consts::PI;
            let dir = Vec3::new(sin_t * phi.cos(), cos_t, sin_t * phi.sin());
            (dir * depth, dir)
        }
        EmitterShape::Unknown => (Vec3::ZERO, Vec3::Y),
    }
}

fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    rng.next_u32() as f32 / u32::MAX as f32
}
fn unit_signed(rng: &mut ChaCha8Rng) -> f32 {
    unit_f32(rng) * 2.0 - 1.0
}
fn sample_unit_sphere(rng: &mut ChaCha8Rng) -> Vec3 {
    // Rejection-sample inside the unit cube; cheap and avoids the bias
    // a sin/cos parametric sample produces for non-unit-radius shapes.
    // The accept rate is π/6 ≈ 52.3%, so 32 attempts drives the
    // fall-through probability to (1 − π/6)³² ≈ 1.5e-10 — effectively
    // unreachable. The previous 8-attempt cap let ~0.26% of all samples
    // hit the `Vec3::ZERO` fallback, which manifested as a permanent
    // dense dot at the centre of every spherical emitter. The fallback
    // now also returns a finite non-origin point so the truly
    // unreachable case can't reintroduce that artefact.
    for _ in 0..32 {
        let v = Vec3::new(unit_signed(rng), unit_signed(rng), unit_signed(rng));
        if v.length_squared() <= 1.0 {
            return v;
        }
    }
    Vec3::new(0.5, 0.0, 0.0)
}
