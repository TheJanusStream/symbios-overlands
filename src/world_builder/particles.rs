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

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::hierarchy::ChildOf;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp3, Fp4, ParticleBlendMode, SignSource, SimulationSpace,
    TextureAtlas, TextureFilter,
};

use super::compile::SpawnCtx;
use super::image_cache::{SamplerFilter, request_blob_image_filtered};

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
    /// [`super::image_cache::BlobImageCache`] and patches every alive
    /// particle's material with the resulting [`Handle<Image>`].
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
}

/// Marker so the room compiler's cleanup query reaches every emitter
/// alongside its surviving particle entities. Particles get [`super::RoomEntity`]
/// directly (in world simulation space) or are parented under the
/// emitter (local simulation space) so they ride the chassis's despawn
/// in either case.
#[derive(Component)]
pub struct ParticleEmitterMarker;

/// Cached unit quad mesh — every untextured / single-frame particle
/// uses this handle. The quad is a 1×1 square in the local XY plane
/// facing local +Z; the tick system rotates it to face the camera
/// (billboard) or align with velocity each frame, and `Transform.scale`
/// applies the per-particle size.
#[derive(Resource)]
pub struct ParticleQuadMesh(pub Handle<Mesh>);

impl FromWorld for ParticleQuadMesh {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        ParticleQuadMesh(meshes.add(atlas_frame_mesh(1, 1, 0)))
    }
}

/// Lazily-built per-frame quad mesh cache for atlas-textured particles.
/// Keyed by `(rows, cols, frame_idx)`. Bounded at 256 unique entries
/// per `(rows, cols)` configuration by the sanitiser's
/// `MAX_PARTICLE_ATLAS_DIM = 16` cap, so cache memory stays well-defined.
#[derive(Resource, Default)]
pub struct ParticleAtlasMeshes {
    pub by_frame: std::collections::HashMap<(u32, u32, u32), Handle<Mesh>>,
}

impl ParticleAtlasMeshes {
    /// Look up or build the quad mesh for a given atlas frame. Cells
    /// are addressed in row-major order (`frame_idx = row * cols +
    /// col`) so cycling animations sweep across each row before
    /// dropping to the next.
    pub fn get_or_create(
        &mut self,
        meshes: &mut Assets<Mesh>,
        rows: u32,
        cols: u32,
        frame_idx: u32,
    ) -> Handle<Mesh> {
        let key = (rows, cols, frame_idx);
        if let Some(handle) = self.by_frame.get(&key) {
            return handle.clone();
        }
        let handle = meshes.add(atlas_frame_mesh(rows, cols, frame_idx));
        self.by_frame.insert(key, handle.clone());
        handle
    }
}

/// Build a 4-vertex quad whose UVs map to one cell of an
/// `rows × cols` sprite-sheet atlas. `(rows=1, cols=1, frame_idx=0)`
/// reproduces the full-image UVs that the v1 untextured quad used,
/// keeping the cache key uniform for the no-atlas case.
fn atlas_frame_mesh(rows: u32, cols: u32, frame_idx: u32) -> Mesh {
    let positions: Vec<[f32; 3]> = vec![
        [-0.5, -0.5, 0.0],
        [0.5, -0.5, 0.0],
        [0.5, 0.5, 0.0],
        [-0.5, 0.5, 0.0],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; 4];

    // Atlas frame UVs — assumes row-major order, top-left origin.
    let cols = cols.max(1);
    let rows = rows.max(1);
    let total = rows * cols;
    let frame = frame_idx.min(total.saturating_sub(1));
    let row = frame / cols;
    let col = frame % cols;
    let u_step = 1.0 / cols as f32;
    let v_step = 1.0 / rows as f32;
    let u0 = col as f32 * u_step;
    let v0 = row as f32 * v_step;
    let u1 = u0 + u_step;
    let v1 = v0 + v_step;
    // The quad winds bottom-left → bottom-right → top-right →
    // top-left in local XY. Map `v0` (top of cell) to local-top
    // verts and `v1` (bottom of cell) to local-bottom verts so atlas
    // frames render right-side-up.
    let uvs: Vec<[f32; 2]> = vec![[u0, v1], [u1, v1], [u1, v0], [u0, v0]];

    let indices = Indices::U32(vec![0, 1, 2, 0, 2, 3]);
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(indices);
    let _ = mesh.generate_tangents();
    mesh
}

/// Resolve the per-particle [`SamplerFilter`] from a record's
/// [`TextureFilter`]. Unknown forward-compat values fall back to
/// Linear so a forward-compat record renders smooth-filtered.
fn sampler_filter_for(filter: &TextureFilter) -> SamplerFilter {
    match filter {
        TextureFilter::Nearest => SamplerFilter::Nearest,
        TextureFilter::Linear | TextureFilter::Unknown => SamplerFilter::Linear,
    }
}

/// Compute the active frame index for a particle given its frame
/// mode, age, and the atlas dimensions. `Still` always returns 0;
/// `RandomFrame` returns the spawn-baked index unchanged;
/// `OverLifetime` cycles through frames at the configured `fps`,
/// modulo the total cell count.
fn current_frame_index(
    mode: &AnimationFrameMode,
    age: f32,
    spawn_index: u32,
    atlas_dim: Option<(u32, u32)>,
) -> u32 {
    let (rows, cols) = atlas_dim.unwrap_or((1, 1));
    let total = (rows.max(1) * cols.max(1)).max(1);
    match mode {
        AnimationFrameMode::Still => 0,
        AnimationFrameMode::RandomFrame => spawn_index % total,
        AnimationFrameMode::OverLifetime { fps } => {
            let idx = (age * fps.0).floor().max(0.0) as u32;
            idx % total
        }
        AnimationFrameMode::Unknown => 0,
    }
}

/// Spawn a ParticleSystem emitter entity. The entity carries the
/// parameter snapshot, the deterministic RNG, and the motion tracker;
/// its child particles are spawned per-frame by [`tick_emitter_spawn`].
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_particle_emitter_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    snapshot: ParticleEmitter,
    seed: u64,
    transform: Transform,
) -> Entity {
    let rng = ChaCha8Rng::seed_from_u64(seed);

    let mut cmd = ctx.commands.spawn((
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
    if !ctx.avatar_mode {
        cmd.insert(super::RoomEntity);
    }
    cmd.id()
}

/// Translate a [`crate::pds::GeneratorKind::ParticleSystem`] payload
/// into a [`ParticleEmitter`] snapshot. Pulled out so the spawn arm in
/// `compile::spawn_generator` stays a one-liner.
#[allow(clippy::too_many_arguments)]
pub(super) fn snapshot_from_record(
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

/// Update each emitter's tracked world velocity. Walks up the parent
/// chain looking for a `LinearVelocity` (avian3d `RigidBody`); if none
/// is found, falls back to numerical differentiation of
/// `GlobalTransform::translation` over one frame. The first frame after
/// spawn primes `last_translation` and reports zero velocity to avoid
/// emitting a one-frame teleport spike.
pub fn update_emitter_motion(
    time: Res<Time>,
    mut emitters: Query<(
        &mut EmitterMotionTracker,
        &GlobalTransform,
        Option<&ChildOf>,
    )>,
    velocity_query: Query<&LinearVelocity>,
    parent_query: Query<&ChildOf>,
) {
    let dt = time.delta_secs().max(1e-6);
    for (mut tracker, gxform, child_of) in emitters.iter_mut() {
        let world_pos = gxform.translation();

        // Resolve velocity: walk up the parent chain looking for a
        // `LinearVelocity`. The walk bounds at 16 steps so a pathological
        // hierarchy can't pin a frame in this query.
        let mut velocity_from_physics: Option<Vec3> = None;
        let mut probe: Option<Entity> = child_of.map(|p| p.parent());
        for _ in 0..16 {
            let Some(p) = probe else { break };
            if let Ok(lv) = velocity_query.get(p) {
                velocity_from_physics = Some(lv.0);
                break;
            }
            probe = parent_query.get(p).ok().map(|pp| pp.parent());
        }

        if !tracker.initialised {
            tracker.last_translation = world_pos;
            tracker.world_velocity = velocity_from_physics.unwrap_or(Vec3::ZERO);
            tracker.initialised = true;
            continue;
        }

        tracker.world_velocity =
            velocity_from_physics.unwrap_or_else(|| (world_pos - tracker.last_translation) / dt);
        tracker.last_translation = world_pos;
    }
}

/// Drive each emitter's particle spawning. Each frame:
/// 1. age the emitter and the per-cycle clock; loop the cycle if
///    `looping`, otherwise stop emitting once `age >= duration`.
/// 2. add `dt * rate_per_second` to the spawn accumulator and spawn
///    `floor(accumulator)` particles, decrementing.
/// 3. on each cycle boundary fire `burst_count` particles at once.
/// 4. cap at `max_particles` (skip emit when full so we never exceed).
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn tick_emitter_spawn(
    mut commands: Commands,
    time: Res<Time>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    quad_mesh: Res<ParticleQuadMesh>,
    mut atlas_meshes: ResMut<ParticleAtlasMeshes>,
    mut blob_image_cache: ResMut<super::image_cache::BlobImageCache>,
    mut emitters: Query<(
        Entity,
        &ParticleEmitter,
        &mut EmitterState,
        &EmitterMotionTracker,
        &GlobalTransform,
        Has<super::RoomEntity>,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (entity, emitter, mut state, tracker, gxform, room_owned) in emitters.iter_mut() {
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

        for _ in 0..to_spawn {
            spawn_one_particle(
                &mut commands,
                entity,
                emitter,
                &mut state,
                tracker,
                gxform,
                &mut std_materials,
                &mut meshes,
                &mut atlas_meshes,
                &mut blob_image_cache,
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
    std_materials: &mut Assets<StandardMaterial>,
    meshes: &mut Assets<Mesh>,
    atlas_meshes: &mut ParticleAtlasMeshes,
    blob_image_cache: &mut super::image_cache::BlobImageCache,
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

    let mut material = StandardMaterial {
        base_color: emitter.start_color.into(),
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    };
    material.alpha_mode = match emitter.blend_mode {
        ParticleBlendMode::Alpha => AlphaMode::Blend,
        // Bevy's StandardMaterial does expose `AlphaMode::Add` for
        // brightness-additive blending (sparks / fire / glow). Routing
        // the start colour through `emissive` lights the additive
        // accumulator on dark backgrounds where pure-alpha would wash
        // out.
        ParticleBlendMode::Additive | ParticleBlendMode::Unknown => AlphaMode::Add,
    };
    if matches!(
        emitter.blend_mode,
        ParticleBlendMode::Additive | ParticleBlendMode::Unknown
    ) {
        material.emissive = emitter.start_color;
    }
    let material_handle = std_materials.add(material);

    // Optional texture: register against the shared blob image cache;
    // the poll system patches the material's `base_color_texture` once
    // the bytes arrive. Sampler filter is baked into the cache key so
    // a Linear-filtered request and a Nearest-filtered request for the
    // same source produce two distinct GPU images. No-op when the
    // emitter has no texture configured. Subsequent particles from the
    // same emitter share the cache entry and either paint
    // synchronously (Ready) or enqueue (Pending) — exactly the same
    // behaviour Sign generators get.
    if let Some(source) = &emitter.texture {
        request_blob_image_filtered(
            commands,
            blob_image_cache,
            std_materials,
            &material_handle,
            source,
            sampler_filter_for(&emitter.texture_filter),
        );
    }

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
                cmd.insert(super::RoomEntity);
            }
        }
    }
}

/// Tick every alive particle: integrate velocity / position, fade size
/// and colour, optionally collide, despawn at end-of-life. Decrements
/// the source emitter's `alive_count` on despawn so the spawn cap stays
/// accurate.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn tick_particles(
    mut commands: Commands,
    time: Res<Time>,
    gravity: Res<Gravity>,
    spatial: SpatialQuery,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut atlas_meshes: ResMut<ParticleAtlasMeshes>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut emitters: Query<&mut EmitterState>,
    emitter_lookup: Query<&ParticleEmitter>,
    mut particles: Query<(
        Entity,
        &mut Particle,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let camera_pos = cameras.iter().next().map(|t| t.translation());

    for (entity, mut particle, mut transform, material_handle, mut mesh3d) in particles.iter_mut() {
        particle.age += dt;
        if particle.age >= particle.lifetime {
            commands.entity(entity).despawn();
            if let Ok(mut state) = emitters.get_mut(particle.emitter) {
                state.alive_count = state.alive_count.saturating_sub(1);
            }
            continue;
        }

        let Ok(emitter) = emitter_lookup.get(particle.emitter) else {
            // Source emitter was despawned (room rebuild). Let the
            // particle finish its own lifetime by reading its current
            // age vs lifetime — no further integration once the
            // emitter's snapshot is gone (we don't know its blend
            // mode / curves any more).
            continue;
        };

        // Apply forces.
        let accel = emitter.acceleration + emitter.gravity_multiplier * gravity.0;
        particle.velocity += accel * dt;
        // Exponential drag: v *= exp(-drag * dt).
        if emitter.linear_drag > 0.0 {
            let factor = (-emitter.linear_drag * dt).exp();
            particle.velocity *= factor;
        }

        let mut new_pos = transform.translation + particle.velocity * dt;

        if emitter.collide_terrain || emitter.collide_water || emitter.collide_colliders {
            apply_collisions(
                &spatial,
                emitter,
                &mut particle.velocity,
                &mut new_pos,
                transform.translation,
                dt,
            );
        }

        transform.translation = new_pos;

        // Fade size / colour over lifetime.
        let t = (particle.age / particle.lifetime).clamp(0.0, 1.0);
        let size = lerp_unit(t, emitter.start_size, emitter.end_size);
        transform.scale = Vec3::splat(size);

        if let Some(material) = materials.get_mut(&material_handle.0) {
            let color = LinearRgba::new(
                lerp_unit(t, emitter.start_color.red, emitter.end_color.red),
                lerp_unit(t, emitter.start_color.green, emitter.end_color.green),
                lerp_unit(t, emitter.start_color.blue, emitter.end_color.blue),
                lerp_unit(t, emitter.start_color.alpha, emitter.end_color.alpha),
            );
            material.base_color = color.into();
            if matches!(
                emitter.blend_mode,
                ParticleBlendMode::Additive | ParticleBlendMode::Unknown
            ) {
                material.emissive = color;
            }
        }

        // Update the active atlas frame (only meaningful for
        // OverLifetime; the other modes pinned a frame at spawn). When
        // the index advances, swap the Mesh3d handle to the cached
        // mesh for the new cell — the atlas-mesh cache reuses handles
        // across particles so the swap is just an asset-id copy, not
        // a new allocation.
        if let AnimationFrameMode::OverLifetime { .. } = particle.frame_mode
            && let Some((rows, cols)) = particle.atlas_dim
        {
            let new_frame = current_frame_index(
                &particle.frame_mode,
                particle.age,
                particle.frame_index,
                particle.atlas_dim,
            );
            if new_frame != particle.frame_index {
                particle.frame_index = new_frame;
                let handle = atlas_meshes.get_or_create(meshes.as_mut(), rows, cols, new_frame);
                mesh3d.0 = handle;
            }
        }

        // Orient the quad: billboard towards the camera, or align with
        // the velocity vector. If neither is meaningful (no camera in
        // scene, zero velocity), leave the previous orientation.
        if emitter.billboard {
            if let Some(cam) = camera_pos {
                let to_cam = cam - transform.translation;
                if to_cam.length_squared() > 1e-6 {
                    transform.look_at(cam, Vec3::Y);
                }
            }
        } else if particle.velocity.length_squared() > 1e-6 {
            let dir = particle.velocity.normalize();
            transform.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
        }
    }
}

/// Resolve collisions for a particle stepping from `prev_pos` along
/// `velocity * dt`. Uses a single ray cast (sphere-cast would be more
/// accurate but ray is cheap enough at 512-particle scale and matches
/// the visual fidelity of an unrotated quad). Reflects velocity with
/// `bounce`, scales tangential by `(1 - friction)`, and snaps the
/// particle to the contact point. Honours the three collide_* flags
/// only insofar as the ray hits any matching collider (terrain / water
/// finite surfaces / arbitrary) — we don't currently distinguish them
/// at the avian level, so the flags act as a single "collide vs
/// pure-visual" toggle in v1. Future work can split them when avian
/// gains layer support.
fn apply_collisions(
    spatial: &SpatialQuery,
    emitter: &ParticleEmitter,
    velocity: &mut Vec3,
    new_pos: &mut Vec3,
    prev_pos: Vec3,
    dt: f32,
) {
    let delta = *new_pos - prev_pos;
    let dist = delta.length();
    if dist <= 1e-5 {
        return;
    }
    let dir = delta / dist;
    let Ok(ray_dir) = Dir3::new(dir) else {
        return;
    };
    let filter = SpatialQueryFilter::default();
    if let Some(hit) = spatial.cast_ray(prev_pos, ray_dir, dist, true, &filter) {
        let normal = hit.normal;
        // Reflect: v' = v - (1 + bounce) * (v · n) n
        let vn = velocity.dot(normal);
        let reflected = *velocity - (1.0 + emitter.bounce) * vn * normal;
        // Apply tangential friction.
        let tangential = reflected - reflected.dot(normal) * normal;
        let normal_component = reflected - tangential;
        *velocity = normal_component + tangential * (1.0 - emitter.friction);
        *new_pos = prev_pos + dir * hit.distance;
        // Step away from the surface a hair so the next frame's ray
        // doesn't immediately re-hit and produce a runaway reflection.
        *new_pos += normal * 1e-3;
        // Damp out tangential velocity that's nearly stopped — keeps a
        // dust particle from sliding forever along a slope.
        if velocity.length_squared() < 1e-3 {
            *velocity = Vec3::ZERO;
        }
        let _ = dt;
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
            // Sample a random direction inside the cone (apex at origin,
            // axis +Y), then scale by a random depth in [0, height] so
            // particles can spawn anywhere inside the volume.
            let depth = unit_f32(rng) * height.0;
            let theta = unit_f32(rng) * half_angle.0;
            let phi = unit_f32(rng) * 2.0 * std::f32::consts::PI;
            let sin_t = theta.sin();
            let dir = Vec3::new(sin_t * phi.cos(), theta.cos(), sin_t * phi.sin());
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
    for _ in 0..8 {
        let v = Vec3::new(unit_signed(rng), unit_signed(rng), unit_signed(rng));
        if v.length_squared() <= 1.0 {
            return v;
        }
    }
    Vec3::ZERO
}

fn lerp_unit(t: f32, a: f32, b: f32) -> f32 {
    a + (b - a) * t
}
