//! World compiler: turns a `RoomRecord` recipe into ECS entities.
//!
//! This plugin owns every entity spawned from the active room recipe. When
//! the owner edits the record — locally through the world editor or
//! remotely via a `RoomStateUpdate` broadcast — the whole recipe is
//! replaced, the compiler despawns every previously-spawned `RoomEntity`,
//! and re-walks the placement graph. That strict rebuild is the only way
//! to avoid double-spawning colliders (Avian crashes if two heightfields
//! coexist at the origin) whenever a patch lands.
//!
//! Terrain heightmap generation stays in `terrain.rs` because the collider
//! must be solid before `AppState::InGame` starts; the recipe's
//! `Terrain` generator is recorded here as a no-op spawn but its `traits`
//! are still applied to the already-existing terrain mesh. Water, shapes
//! and l-systems are compiled fresh on every rebuild.
//!
//! **Determinism:** scatter placements use `ChaCha8Rng` seeded by the
//! placement's `local_seed` so every peer visiting the same DID sees the
//! same objects in the same locations. `thread_rng()` is explicitly
//! forbidden here — OS entropy would desynchronise the shared reality.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy_symbios::LSystemMeshBuilder;
use bevy_symbios::materials::MaterialPalette;
use bevy_symbios_texture::bark::BarkGenerator;
use bevy_symbios_texture::generator::{TextureError, TextureGenerator, TextureMap};
use bevy_symbios_texture::leaf::LeafGenerator;
use bevy_symbios_texture::twig::TwigGenerator;
use bevy_symbios_texture::{map_to_images, map_to_images_card};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use symbios::System;
use symbios_turtle_3d::{TurtleConfig, TurtleInterpreter};

use crate::config::terrain as tcfg;
use crate::pds::{
    Fp3, Generator, Placement, PropMeshType, RoomRecord, ScatterBounds, SovereignMaterialSettings,
    SovereignTerrainConfig, SovereignTextureType, TransformData,
};
use crate::state::AppState;
use crate::terrain::{FinishedHeightMap, TerrainMesh, WaterVolume};
use crate::water::{WaterExtension, WaterMaterial};

/// Marker attached to every entity spawned from the active `RoomRecord`.
/// Despawning all of these is how the compiler applies a record update
/// without double-spawning anything.
#[derive(Component)]
pub struct RoomEntity;

/// Base meshes for each [`PropMeshType`] — built once at startup so every
/// L-system spawn can share the same handles. Foliage variants (Leaf, Twig)
/// are billboard cards whose UV layout matches the upstream
/// `bevy_symbios_texture` card convention (V=1 at the base).
#[derive(Resource)]
pub struct PropMeshAssets {
    pub meshes: HashMap<PropMeshType, Handle<Mesh>>,
}

/// A single in-flight foliage texture task: the async generator future, the
/// material handle whose textures should be populated when the result
/// arrives, and a `is_card` flag selecting between `map_to_images` (tileable)
/// and `map_to_images_card` (clamp-to-edge) upload paths.
pub type FoliageTask = (
    Task<Result<TextureMap, TextureError>>,
    Handle<StandardMaterial>,
    bool,
);

/// In-flight foliage texture tasks, drained by `poll_overlands_foliage_tasks`.
#[derive(Resource, Default)]
pub struct OverlandsFoliageTasks {
    pub tasks: Vec<FoliageTask>,
}

/// One cached L-system slot material: the content hash of the settings that
/// built it, plus the resulting PBR handle.
struct CachedLSystemMaterial {
    settings_hash: u64,
    handle: Handle<StandardMaterial>,
}

/// Persistent cross-compile cache for L-system `StandardMaterial` handles.
///
/// Without this, every `RoomRecord` change rebuilds every generator's
/// material — enqueuing fresh foliage texture tasks for configs that haven't
/// moved. Keyed by `(generator_ref, slot_id)` and invalidated by hashing the
/// canonical (fixed-point) serialisation of `SovereignMaterialSettings`, so
/// a record edit that touches *only* (say) the scatter count re-uses last
/// pass's baked textures instead of re-baking them.
///
/// Entries for `(generator_ref, slot)` pairs not touched during a compile
/// pass are dropped at the end of that pass so stale generators stop
/// pinning their handles in `Assets<StandardMaterial>`.
#[derive(Resource, Default)]
pub struct LSystemMaterialCache {
    entries: HashMap<(String, u8), CachedLSystemMaterial>,
}

pub struct WorldBuilderPlugin;

impl Plugin for WorldBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<OverlandsFoliageTasks>()
            .init_resource::<LSystemMaterialCache>()
            .add_systems(Startup, setup_prop_assets)
            .add_systems(
                Update,
                (compile_room_record, poll_overlands_foliage_tasks)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Billboard quad with its pivot at the base centre. Matches the layout in
/// `lsystem-explorer/src/visuals/assets.rs` so the same foliage cards swap
/// in cleanly.
fn create_foliage_card(width: f32, height: f32) -> Mesh {
    let positions: Vec<[f32; 3]> = vec![
        [-width / 2.0, 0.0, 0.0],
        [width / 2.0, 0.0, 0.0],
        [width / 2.0, height, 0.0],
        [-width / 2.0, height, 0.0],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; 4];
    let uvs: Vec<[f32; 2]> = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let indices = Indices::U32(vec![0, 1, 2, 0, 2, 3]);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(indices);
    let _ = mesh.generate_tangents();
    mesh
}

/// Startup system that populates [`PropMeshAssets`] with the shared prop
/// meshes (one handle per `PropMeshType`).
fn setup_prop_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut prop_meshes = HashMap::new();
    prop_meshes.insert(
        PropMeshType::Leaf,
        meshes.add(create_foliage_card(0.5, 0.8)),
    );
    prop_meshes.insert(
        PropMeshType::Twig,
        meshes.add(create_foliage_card(0.7, 1.0)),
    );
    prop_meshes.insert(
        PropMeshType::Sphere,
        meshes.add(Sphere::new(0.2).mesh().ico(2).unwrap()),
    );
    prop_meshes.insert(
        PropMeshType::Cone,
        meshes.add(Cone::new(0.15, 0.4).mesh().resolution(8)),
    );
    prop_meshes.insert(
        PropMeshType::Cylinder,
        meshes.add(Cylinder::new(0.1, 0.5).mesh().resolution(8)),
    );
    prop_meshes.insert(PropMeshType::Cube, meshes.add(Cuboid::new(0.3, 0.3, 0.3)));

    commands.insert_resource(PropMeshAssets {
        meshes: prop_meshes,
    });
}

/// Walks the active `RoomRecord` and produces ECS entities for every
/// placement. Re-runs automatically whenever the record resource is marked
/// changed; the first frame inside `AppState::InGame` counts as a change
/// because the resource was just inserted during Loading, which performs
/// the initial compilation for free.
#[allow(clippy::too_many_arguments)]
fn compile_room_record(
    mut commands: Commands,
    record: Option<Res<RoomRecord>>,
    existing: Query<Entity, With<RoomEntity>>,
    terrain_meshes: Query<Entity, With<TerrainMesh>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    palette: Option<Res<MaterialPalette>>,
    prop_assets: Option<Res<PropMeshAssets>>,
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    mut lsystem_material_cache: ResMut<LSystemMaterialCache>,
    mut lights: Query<&mut DirectionalLight>,
) {
    let Some(record) = record else {
        return;
    };
    if !record.is_changed() {
        return;
    }

    // Step 1 — Cleanup. Despawn every entity previously compiled out of
    // this record. Terrain is NOT a `RoomEntity` (it is owned by the
    // terrain plugin's own lifecycle), so it survives the rebuild.
    for e in &existing {
        commands.entity(e).despawn();
    }

    // Step 2 — Environment. Patch the shared directional light in place
    // so a record update takes effect on every connected peer.
    let Fp3(c) = record.environment.sun_color;
    for mut light in lights.iter_mut() {
        light.color = Color::srgb(c[0], c[1], c[2]);
    }

    // Cross-compile cache lives in `LSystemMaterialCache` (a persistent
    // Resource). Track which `(generator_ref, slot)` keys were touched this
    // pass so we can drop stale entries at the end — a generator removed
    // from the record would otherwise keep its handles pinned forever.
    let mut lsystem_cache_touched: HashSet<(String, u8)> = HashSet::new();

    // Step 3 — Placements. Walk the recipe; each scatter placement uses
    // its own deterministic RNG so every peer reproduces the same layout.
    for placement in &record.placements {
        match placement {
            Placement::Absolute {
                generator_ref,
                transform,
            } => {
                let ctx = SpawnCtx {
                    commands: &mut commands,
                    record: &record,
                    meshes: &mut meshes,
                    std_materials: &mut std_materials,
                    water_materials: &mut water_materials,
                    palette: palette.as_deref(),
                    heightmap: heightmap.as_deref(),
                    terrain_meshes: &terrain_meshes,
                    prop_assets: prop_assets.as_deref(),
                    foliage_tasks: &mut foliage_tasks,
                    lsystem_material_cache: &mut lsystem_material_cache,
                    lsystem_cache_touched: &mut lsystem_cache_touched,
                };
                spawn_from_generator(ctx, generator_ref, transform_from_data(transform));
            }
            Placement::Scatter {
                generator_ref,
                bounds,
                count,
                local_seed,
                biome_filter,
            } => {
                // Deterministic pick — see `find_terrain_config` doc comment.
                let terrain_cfg = crate::pds::find_terrain_config(&record);
                let max_attempts = count.saturating_mul(10).max(*count);
                let mut rng = ChaCha8Rng::seed_from_u64(*local_seed);
                let mut spawned = 0u32;
                let mut attempts = 0u32;

                while spawned < *count && attempts < max_attempts {
                    attempts += 1;
                    let (x, z) = sample_bounds(bounds, &mut rng);

                    let (y, keep) = if let (Some(hm_res), Some(target), Some(tcfg)) =
                        (heightmap.as_deref(), *biome_filter, terrain_cfg)
                    {
                        let hm = &hm_res.0;
                        // Re-centre: scatter coords are world-centred, the
                        // heightmap is origin-local (see spawn_terrain_mesh
                        // translation of `-half`).
                        let extent = (hm.width() - 1) as f32 * hm.scale();
                        let half = extent * 0.5;
                        let hm_x = (x + half).clamp(0.0, extent);
                        let hm_z = (z + half).clamp(0.0, extent);
                        let y = hm.get_height_at(hm_x, hm_z);
                        let normal = hm.get_normal_at(hm_x, hm_z);
                        let slope = (1.0 - normal[1]).max(0.0);
                        let dominant = dominant_biome(tcfg, y, slope);
                        (y, dominant == target)
                    } else if let Some(hm_res) = heightmap.as_deref() {
                        let hm = &hm_res.0;
                        let extent = (hm.width() - 1) as f32 * hm.scale();
                        let half = extent * 0.5;
                        let hm_x = (x + half).clamp(0.0, extent);
                        let hm_z = (z + half).clamp(0.0, extent);
                        (hm.get_height_at(hm_x, hm_z), true)
                    } else {
                        (0.0, true)
                    };

                    if !keep {
                        continue;
                    }

                    // Inject a deterministic random yaw so scattered forests
                    // look organic — every peer reuses the same local RNG so
                    // the rotation stays shared-world-consistent.
                    let yaw = unit_f32(&mut rng) * std::f32::consts::PI;
                    let tf = Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(yaw));
                    let ctx = SpawnCtx {
                        commands: &mut commands,
                        record: &record,
                        meshes: &mut meshes,
                        std_materials: &mut std_materials,
                        water_materials: &mut water_materials,
                        palette: palette.as_deref(),
                        heightmap: heightmap.as_deref(),
                        terrain_meshes: &terrain_meshes,
                        prop_assets: prop_assets.as_deref(),
                        foliage_tasks: &mut foliage_tasks,
                        lsystem_material_cache: &mut lsystem_material_cache,
                        lsystem_cache_touched: &mut lsystem_cache_touched,
                    };
                    spawn_from_generator(ctx, generator_ref, tf);
                    spawned += 1;
                }

                if spawned < *count {
                    debug!(
                        "Scatter `{}` placed {}/{} points (biome filter {:?}, {} attempts)",
                        generator_ref, spawned, count, biome_filter, attempts
                    );
                }
            }
            Placement::Unknown => {
                warn!("Skipping placement with unknown $type");
            }
        }
    }

    // Drop cache entries whose `(generator_ref, slot)` was not touched this
    // compile pass — that slot is no longer referenced by the record, so
    // keeping the handle alive would pin a `StandardMaterial` (and any
    // baked foliage textures it points at) in `Assets` forever.
    lsystem_material_cache
        .entries
        .retain(|k, _| lsystem_cache_touched.contains(k));
}

/// Stable content hash of a `SovereignMaterialSettings` for the L-system
/// material cache. Serde already rounds every `f32`/`f64` field to the
/// fixed-point `i32` wire form (see `Fp`/`Fp3`/`Fp64` impls in `pds`), so
/// hashing the JSON bytes yields a representation-equal fingerprint with
/// no manual field walking — and skips the NaN/denormal footguns hashing
/// raw floats would bring.
fn settings_fingerprint(settings: &SovereignMaterialSettings) -> u64 {
    let mut hasher = DefaultHasher::new();
    match serde_json::to_vec(settings) {
        Ok(bytes) => bytes.hash(&mut hasher),
        // Serialisation of a plain struct of scalars cannot fail in
        // practice; if it somehow does, fall back to a distinct sentinel
        // so the match arm below treats every lookup as a miss (forcing a
        // rebuild) rather than collapsing all failures onto the same key.
        Err(_) => {
            0xDEAD_BEEF_u64.hash(&mut hasher);
            (settings as *const SovereignMaterialSettings as usize).hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn transform_from_data(t: &TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}

/// Uniform sample inside the scatter region. Circle bounds use rejection
/// sampling so the distribution stays flat instead of clumping at the
/// centre (which a naïve `radius * random()` would produce).
fn sample_bounds(bounds: &ScatterBounds, rng: &mut ChaCha8Rng) -> (f32, f32) {
    match bounds {
        ScatterBounds::Rect { center, extents } => {
            let x = center.0[0] + unit_f32(rng) * extents.0[0];
            let z = center.0[1] + unit_f32(rng) * extents.0[1];
            (x, z)
        }
        ScatterBounds::Circle { center, radius } => loop {
            let x = unit_f32(rng);
            let z = unit_f32(rng);
            if x * x + z * z <= 1.0 {
                return (center.0[0] + x * radius.0, center.0[1] + z * radius.0);
            }
        },
    }
}

/// Deterministic `[-1, 1]` sample from a `ChaCha8Rng`.
fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    let v = rng.next_u32() as f32 / u32::MAX as f32;
    v * 2.0 - 1.0
}

// ---------------------------------------------------------------------------
// Biome evaluation
// ---------------------------------------------------------------------------

/// Inline port of `SplatRule::weight` so we can evaluate a single
/// world-space point without running a full `SplatMapper::generate` pass
/// over the whole heightmap on every scatter attempt.
fn rule_weight(r: &crate::pds::SovereignSplatRule, h: f32, slope: f32) -> f32 {
    let h_w = smooth_range(h, r.height_min.0, r.height_max.0, r.sharpness.0);
    let s_w = smooth_range(slope, r.slope_min.0, r.slope_max.0, r.sharpness.0);
    h_w * s_w
}

fn smooth_range(value: f32, lo: f32, hi: f32, sharpness: f32) -> f32 {
    if lo >= hi {
        return if (value - lo).abs() < f32::EPSILON {
            1.0
        } else {
            0.0
        };
    }
    let mid = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let dist = (value - mid).abs();
    (1.0 - (dist / half).min(1.0)).powf(sharpness.max(0.001))
}

/// Return the dominant biome index (0=Grass, 1=Dirt, 2=Rock, 3=Snow) at the
/// given world-space (height, slope) pair, using the terrain generator's
/// splat rules. The splat rules expect *normalised* heights so we divide
/// by `height_scale` first.
fn dominant_biome(cfg: &SovereignTerrainConfig, height_world: f32, slope: f32) -> u8 {
    let height_norm = if cfg.height_scale.0.abs() > f32::EPSILON {
        height_world / cfg.height_scale.0
    } else {
        0.0
    };
    let weights = [
        rule_weight(&cfg.material.rules[0], height_norm, slope),
        rule_weight(&cfg.material.rules[1], height_norm, slope),
        rule_weight(&cfg.material.rules[2], height_norm, slope),
        rule_weight(&cfg.material.rules[3], height_norm, slope),
    ];
    let mut best = 0;
    let mut max_w = weights[0];
    for (i, &w) in weights.iter().enumerate().skip(1) {
        if w > max_w {
            max_w = w;
            best = i;
        }
    }
    best as u8
}

// ---------------------------------------------------------------------------
// Generator-specific spawners
// ---------------------------------------------------------------------------

/// Parameter bundle for recursive generator spawning — a plain struct
/// keeps the call sites readable while avoiding a 12-argument signature.
/// Commands and Query carry separate `('w, 's)` lifetimes from the
/// SystemParam pair; we can't unify them here without making the borrow
/// checker invariance rules break at the call site, so they get independent
/// parameters.
struct SpawnCtx<'a, 'wc, 'sc, 'wq, 'sq> {
    commands: &'a mut Commands<'wc, 'sc>,
    record: &'a RoomRecord,
    meshes: &'a mut Assets<Mesh>,
    std_materials: &'a mut Assets<StandardMaterial>,
    water_materials: &'a mut Assets<WaterMaterial>,
    palette: Option<&'a MaterialPalette>,
    heightmap: Option<&'a FinishedHeightMap>,
    terrain_meshes: &'a Query<'wq, 'sq, Entity, With<TerrainMesh>>,
    prop_assets: Option<&'a PropMeshAssets>,
    foliage_tasks: &'a mut OverlandsFoliageTasks,
    /// Persistent, hash-invalidated material cache. A single scatter
    /// placement with count=100 would otherwise allocate 100 fresh
    /// `StandardMaterial`s *and* enqueue 100 identical foliage texture
    /// tasks for the same slot — and across compile passes an unchanged
    /// slot would re-bake every time the record is patched. The cache
    /// keys on `(generator_ref, slot)` and reuses the handle whenever the
    /// content hash of `SovereignMaterialSettings` is identical.
    lsystem_material_cache: &'a mut LSystemMaterialCache,
    /// `(generator_ref, slot)` keys touched this compile pass. Populated
    /// as we resolve material handles so the caller can GC stale entries.
    lsystem_cache_touched: &'a mut HashSet<(String, u8)>,
}

fn spawn_from_generator(
    mut ctx: SpawnCtx<'_, '_, '_, '_, '_>,
    generator_ref: &str,
    transform: Transform,
) {
    let Some(generator) = ctx.record.generators.get(generator_ref) else {
        warn!(
            "Placement references unknown generator `{}` — skipped",
            generator_ref
        );
        return;
    };
    match generator {
        Generator::Terrain(_) => {
            // Terrain is generated and meshed by `terrain.rs` during the
            // Loading state (so the heightfield collider is ready before
            // gameplay begins). The recipe still participates through
            // `traits`, which we apply here to every existing terrain
            // mesh entity.
            //
            // Because terrain entities survive a `RoomEntity` rebuild,
            // first wipe any previously-attached trait components — if a
            // trait was removed from the record, the diff must actually
            // take effect on the live mesh.
            for terrain_entity in ctx.terrain_meshes.iter() {
                reset_traits(ctx.commands, terrain_entity);
                apply_traits(ctx.commands, terrain_entity, ctx.record, generator_ref);
            }
        }
        Generator::Water { level_offset } => {
            // Size the water volume to the *active* heightmap extent so it
            // continues to cover the map when the room owner scales
            // `grid_size` / `cell_scale` outside the compile-time defaults.
            // Without this, `buoyancy` and the visual water plane drift
            // apart (see `apply_buoyancy_forces` — it bounds lift by the
            // same heightmap extent) and a guest driving off the edge of
            // a stale 1022 m² cube lands in a valley still floating.
            let world_extent = ctx
                .heightmap
                .map(|hm| (hm.0.width() - 1) as f32 * hm.0.scale())
                .unwrap_or_else(|| (tcfg::GRID_SIZE - 1) as f32 * tcfg::CELL_SCALE);
            let entity = spawn_water_volume(
                ctx.commands,
                level_offset.0,
                transform,
                world_extent,
                ctx.meshes,
                ctx.water_materials,
            );
            apply_traits(ctx.commands, entity, ctx.record, generator_ref);
        }
        Generator::LSystem { .. } => {
            spawn_lsystem_entity(&mut ctx, generator, generator_ref, transform);
        }
        Generator::Shape { .. } => {
            // Stub: symbios-shape integration lands in a follow-up.
        }
        Generator::Unknown => {
            warn!("Ignoring generator `{}` of unknown $type", generator_ref);
        }
    }
}

/// Spawn the translucent water cuboid scaled to cover the whole terrain.
/// `world_extent` is the active heightmap's side length so the water plane
/// matches whatever `grid_size × cell_scale` the room owner configured.
fn spawn_water_volume(
    commands: &mut Commands,
    level_offset: f32,
    placement_tf: Transform,
    world_extent: f32,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<WaterMaterial>,
) -> Entity {
    let base_wl = tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE;
    let wl = (base_wl + level_offset).max(0.001);

    let water_mat = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(
                tcfg::water::COLOR[0],
                tcfg::water::COLOR[1],
                tcfg::water::COLOR[2],
                tcfg::water::COLOR[3],
            ),
            perceptual_roughness: tcfg::water::ROUGHNESS,
            metallic: tcfg::water::METALLIC,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        },
        extension: WaterExtension::default(),
    });

    let mut tf = placement_tf;
    tf.translation.y += wl / 2.0;
    tf.scale = Vec3::new(world_extent, wl, world_extent);

    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(water_mat),
            tf,
            WaterVolume,
            RoomEntity,
        ))
        .id()
}

/// Compile + mesh an `LSystem` generator at the given transform. Materials
/// are resolved against the palette that `bevy_symbios::materials::sync_*`
/// maintains; if the palette isn't ready yet we fall back to the per-slot
/// config baked into a fresh `StandardMaterial`.
fn spawn_lsystem_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator: &Generator,
    generator_ref: &str,
    transform: Transform,
) {
    let Generator::LSystem {
        source_code,
        finalization_code,
        iterations,
        seed,
        angle,
        step,
        width,
        elasticity,
        tropism,
        materials: lsys_materials,
        prop_mappings,
        prop_scale,
        mesh_resolution,
        ..
    } = generator
    else {
        return;
    };

    // 1. Parse + derive via the standard `symbios::System` pipeline.
    let mut sys = System::new();
    sys.set_seed(*seed);

    for (i, line) in source_code.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with('#') {
            if let Err(e) = sys.add_directive(trimmed) {
                warn!("L-system `{}` line {}: {}", generator_ref, i + 1, e);
                return;
            }
            continue;
        }
        if let Some(axiom) = trimmed.strip_prefix("omega:") {
            if let Err(e) = sys.set_axiom(axiom.trim()) {
                warn!("L-system `{}` axiom error: {}", generator_ref, e);
                return;
            }
            continue;
        }
        if let Err(e) = sys.add_rule(trimmed) {
            warn!("L-system `{}` rule error: {}", generator_ref, e);
            return;
        }
    }

    // Cap the derived state length so a malicious record can't weaponise a
    // productive grammar (e.g. an axiom expanding >10× per step) into a
    // multi-gigabyte symbol buffer that locks the main thread inside the
    // turtle interpreter. 2^20 symbols is well past the largest legitimate
    // L-system our shipping presets produce.
    const MAX_LSYSTEM_STATE_LEN: usize = 1 << 20;
    // Force the hard cap into symbios's own back-buffer so the derivation
    // engine returns `CapacityOverflow` before the single-step expansion
    // can allocate past our budget. Without this, a rule like
    // `A -> [16 KB of junk]` applied to a 1M-symbol state could try to
    // allocate tens of billions of symbols inside a single `derive(1)`
    // call — the post-derive length check fires too late to prevent the
    // OOM that allocation triggers.
    sys.max_capacity = MAX_LSYSTEM_STATE_LEN;
    for _ in 0..*iterations {
        if let Err(e) = sys.derive(1) {
            warn!("L-system `{}` derivation error: {}", generator_ref, e);
            return;
        }
        if sys.state.len() > MAX_LSYSTEM_STATE_LEN {
            warn!(
                "L-system `{}` state exceeded {} symbols — aborting derivation",
                generator_ref, MAX_LSYSTEM_STATE_LEN
            );
            return;
        }
    }

    if !finalization_code.trim().is_empty() {
        sys.rules.clear();
        sys.ignored_symbols.clear();
        for (i, line) in finalization_code.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("omega:") {
                continue;
            }
            if trimmed.starts_with('#') {
                if let Err(e) = sys.add_directive(trimmed) {
                    warn!(
                        "L-system `{}` finalization line {}: {}",
                        generator_ref,
                        i + 1,
                        e
                    );
                    return;
                }
                continue;
            }
            if let Err(e) = sys.add_rule(trimmed) {
                warn!(
                    "L-system `{}` finalization rule error: {}",
                    generator_ref, e
                );
                return;
            }
        }
        if let Err(e) = sys.derive(1) {
            warn!(
                "L-system `{}` finalization derivation error: {}",
                generator_ref, e
            );
            return;
        }
        // Mirror the main-loop guard: the owner's finalization rules can
        // carry their own 16 KB replacement strings, so a 1M-symbol
        // carrier state × those rules must not be allowed to slip past
        // the engine's back-buffer cap through the single finalization
        // derive step. `sys.max_capacity` (set above) handles the
        // allocation-side clamp; this guard keeps the interpreter off
        // a runaway state if a future symbios release relaxes the
        // internal `CapacityOverflow` contract.
        if sys.state.len() > MAX_LSYSTEM_STATE_LEN {
            warn!(
                "L-system `{}` finalization exceeded {} symbols — aborting",
                generator_ref, MAX_LSYSTEM_STATE_LEN
            );
            return;
        }
    }

    if sys.state.is_empty() {
        return;
    }

    // 2. Interpret into a 3D skeleton.
    let turtle_config = TurtleConfig {
        default_step: step.0.max(0.001),
        default_angle: angle.0.to_radians(),
        initial_width: width.0.max(0.001),
        tropism: tropism.as_ref().map(|t| Vec3::from_array(t.0)),
        elasticity: elasticity.0,
        max_stack_depth: 1024,
    };
    let mut interpreter = TurtleInterpreter::new(turtle_config);
    interpreter.populate_standard_symbols(&sys.interner);
    let skeleton = interpreter.build_skeleton(&sys.state);

    // 3. Mesh the skeleton. Each material ID produces a separate mesh.
    let mesh_buckets = LSystemMeshBuilder::new()
        .with_resolution((*mesh_resolution).max(3))
        .build(&skeleton);

    // 4. Parent every mesh under a single transform so the placement's
    //    rotation/position anchors the whole plant/shape as a unit.
    let parent = ctx
        .commands
        .spawn((transform, Visibility::default(), RoomEntity))
        .id();

    // Build material handles per slot. For foliage slots (Leaf/Twig/Bark)
    // we *also* spawn a texture-generation task so the handle receives its
    // procedural albedo/normal/ORM maps on a later frame. The palette path
    // still wins when `bevy_symbios::materials::sync_*` has already
    // resolved a shared palette slot for us — in that case we skip the
    // task, because the palette owns texture sync.
    let mut slot_handles: HashMap<u8, Handle<StandardMaterial>> = HashMap::new();
    for (&slot, settings) in lsys_materials.iter() {
        let handle = if let Some(palette) = ctx.palette
            && let Some(h) = palette.materials.get(&slot)
        {
            h.clone()
        } else {
            let key = (generator_ref.to_string(), slot);
            let hash = settings_fingerprint(settings);
            ctx.lsystem_cache_touched.insert(key.clone());
            match ctx.lsystem_material_cache.entries.get(&key) {
                Some(cached) if cached.settings_hash == hash => cached.handle.clone(),
                _ => {
                    let handle = spawn_foliage_material(ctx, settings);
                    ctx.lsystem_material_cache.entries.insert(
                        key,
                        CachedLSystemMaterial {
                            settings_hash: hash,
                            handle: handle.clone(),
                        },
                    );
                    handle
                }
            }
        };
        slot_handles.insert(slot, handle);
    }

    for (material_id, mesh) in mesh_buckets {
        let material = slot_handles
            .get(&material_id)
            .cloned()
            .unwrap_or_else(|| ctx.std_materials.add(StandardMaterial::default()));

        // NB: no `RoomEntity` marker on child meshes. The parent below
        // carries it, and Bevy 0.18's recursive `despawn` tears down
        // children automatically. Marking children with `RoomEntity` too
        // causes the logout / room-rebuild cleanup queries to yield both
        // parent and child, and whichever lands first cascades the
        // despawn, leaving the other as an "entity despawned" warning.
        let child = ctx
            .commands
            .spawn((
                Mesh3d(ctx.meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::IDENTITY,
            ))
            .id();
        ctx.commands.entity(parent).add_child(child);
    }

    // Spawn prop billboards/primitives. Each prop inherits its material
    // from `slot_handles`, so foliage props share the same handle as the
    // branch meshes — when the async texture task finishes, the prop picks
    // up the albedo automatically. A prop whose `prop_id` has no mapping
    // falls back to `PropMeshType::Leaf`.
    if let Some(prop_assets) = ctx.prop_assets {
        let ps = prop_scale.0.max(0.0);
        for prop in &skeleton.props {
            let mesh_type = prop_mappings
                .get(&prop.prop_id)
                .copied()
                .unwrap_or(PropMeshType::Leaf);
            let Some(mesh_handle) = prop_assets.meshes.get(&mesh_type) else {
                continue;
            };
            let material = slot_handles
                .get(&prop.material_id)
                .cloned()
                .unwrap_or_else(|| ctx.std_materials.add(StandardMaterial::default()));

            let child = ctx
                .commands
                .spawn((
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material),
                    Transform {
                        translation: prop.position,
                        rotation: prop.rotation,
                        scale: prop.scale * ps,
                    },
                ))
                .id();
            ctx.commands.entity(parent).add_child(child);
        }
    }

    apply_traits(ctx.commands, parent, ctx.record, generator_ref);
    // Silence unused-binding warnings when the heightmap is unused here.
    let _ = ctx.heightmap;
}

/// Build a `StandardMaterial` from sovereign settings, enqueuing an async
/// texture-generation task for foliage variants. Returns a handle that the
/// caller installs on every strand / prop belonging to the slot.
fn spawn_foliage_material(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    settings: &SovereignMaterialSettings,
) -> Handle<StandardMaterial> {
    let emissive = Color::srgb_from_array(settings.emission_color.0).to_linear()
        * settings.emission_strength.0;

    let (alpha_mode, double_sided, cull_mode, is_card) = match settings.texture_type {
        SovereignTextureType::Leaf | SovereignTextureType::Twig => {
            (AlphaMode::Mask(0.5), true, None, true)
        }
        SovereignTextureType::Bark => (AlphaMode::Opaque, false, Some(Face::Back), false),
        _ => (AlphaMode::Opaque, false, Some(Face::Back), false),
    };

    let handle = ctx.std_materials.add(StandardMaterial {
        base_color: Color::srgb_from_array(settings.base_color.0),
        perceptual_roughness: settings.roughness.0,
        metallic: settings.metallic.0,
        emissive,
        alpha_mode,
        double_sided,
        cull_mode,
        ..default()
    });

    let pool = AsyncComputeTaskPool::get();
    match settings.texture_type {
        SovereignTextureType::Leaf => {
            let config = settings.leaf_config.to_leaf_config();
            let task = pool.spawn(async move { LeafGenerator::new(config).generate(512, 512) });
            ctx.foliage_tasks
                .tasks
                .push((task, handle.clone(), is_card));
        }
        SovereignTextureType::Twig => {
            let config = settings.twig_config.to_twig_config();
            let task = pool.spawn(async move { TwigGenerator::new(config).generate(512, 512) });
            ctx.foliage_tasks
                .tasks
                .push((task, handle.clone(), is_card));
        }
        SovereignTextureType::Bark => {
            let config = settings.bark_config.to_bark_config();
            let task = pool.spawn(async move { BarkGenerator::new(config).generate(512, 512) });
            ctx.foliage_tasks
                .tasks
                .push((task, handle.clone(), is_card));
        }
        _ => {}
    }

    handle
}

/// Drains completed foliage texture tasks and copies the generated images
/// onto their target `StandardMaterial` handles. Runs every frame.
pub fn poll_overlands_foliage_tasks(
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut finished: Vec<(
        Handle<StandardMaterial>,
        Result<TextureMap, TextureError>,
        bool,
    )> = Vec::new();

    foliage_tasks.tasks.retain_mut(|(task, handle, is_card)| {
        if let Some(result) = block_on(future::poll_once(task)) {
            finished.push((handle.clone(), result, *is_card));
            false
        } else {
            true
        }
    });

    for (handle, result, is_card) in finished {
        let map = match result {
            Ok(m) => m,
            Err(e) => {
                error!("Foliage texture generation failed: {e}");
                continue;
            }
        };

        let handles = if is_card {
            map_to_images_card(map, &mut images)
        } else {
            map_to_images(map, &mut images)
        };

        if let Some(mat) = materials.get_mut(&handle) {
            mat.base_color_texture = Some(handles.albedo);
            mat.normal_map_texture = Some(handles.normal);
            mat.metallic_roughness_texture = Some(handles.roughness);
        }
    }
}

/// Attach any ECS components listed under `record.traits[generator_ref]`
/// to `entity`. The trait engine is the main extension point — new
/// lexicon tokens map cleanly to Bevy components without schema churn.
fn apply_traits(commands: &mut Commands, entity: Entity, record: &RoomRecord, generator_ref: &str) {
    let Some(traits) = record.traits.get(generator_ref) else {
        return;
    };
    for t in traits {
        if t == "sensor" {
            commands.entity(entity).insert(Sensor);
        }
    }
}

/// Remove every component that `apply_traits` could have attached. Used on
/// long-lived entities (e.g. the terrain mesh) that survive a room rebuild
/// so a trait deletion actually lands on the live entity instead of
/// leaving the old component stuck in place.
fn reset_traits(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<Sensor>();
}
