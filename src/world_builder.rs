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
use bevy::light::GlobalAmbientLight;
use bevy::math::Isometry3d;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy_symbios::LSystemMeshBuilder;
use bevy_symbios::materials::MaterialPalette;
use bevy_symbios_texture::ashlar::AshlarGenerator;
use bevy_symbios_texture::asphalt::AsphaltGenerator;
use bevy_symbios_texture::bark::BarkGenerator;
use bevy_symbios_texture::brick::BrickGenerator;
use bevy_symbios_texture::cobblestone::CobblestoneGenerator;
use bevy_symbios_texture::concrete::ConcreteGenerator;
use bevy_symbios_texture::corrugated::CorrugatedGenerator;
use bevy_symbios_texture::encaustic::EncausticGenerator;
use bevy_symbios_texture::generator::{TextureError, TextureGenerator, TextureMap};
use bevy_symbios_texture::ground::GroundGenerator;
use bevy_symbios_texture::iron_grille::IronGrilleGenerator;
use bevy_symbios_texture::leaf::LeafGenerator;
use bevy_symbios_texture::marble::MarbleGenerator;
use bevy_symbios_texture::metal::MetalGenerator;
use bevy_symbios_texture::pavers::PaversGenerator;
use bevy_symbios_texture::plank::PlankGenerator;
use bevy_symbios_texture::rock::RockGenerator;
use bevy_symbios_texture::shingle::ShingleGenerator;
use bevy_symbios_texture::stained_glass::StainedGlassGenerator;
use bevy_symbios_texture::stucco::StuccoGenerator;
use bevy_symbios_texture::thatch::ThatchGenerator;
use bevy_symbios_texture::twig::TwigGenerator;
use bevy_symbios_texture::wainscoting::WainscotingGenerator;
use bevy_symbios_texture::window::WindowGenerator;
use bevy_symbios_texture::{map_to_images, map_to_images_card};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use symbios::System;
use symbios_turtle_3d::{SkeletonProp, TurtleConfig, TurtleInterpreter};

use crate::config::terrain as tcfg;
use crate::pds::{
    Fp, Fp3, Fp4, Generator, Placement, PrimNode, PrimShape, PropMeshType, RoomRecord,
    ScatterBounds, SovereignMaterialSettings, SovereignTerrainConfig, SovereignTextureConfig,
    TransformData,
};
use crate::state::{AppState, CurrentRoomDid};
use crate::terrain::{FinishedHeightMap, TerrainMesh, WaterVolume};
use crate::water::{WaterExtension, WaterMaterial};

/// Marks an in-scene portal cube and carries the destination coordinates the
/// interaction system reads when the local player's sensor-collision set
/// touches it.
#[derive(Component, Clone)]
pub struct PortalMarker {
    pub target_did: String,
    pub target_pos: Vec3,
}

/// In-flight ATProto profile-picture fetch for the top face of a portal cube.
/// Drained by `poll_portal_avatar_tasks`; the task lives on its own entity so
/// the portal itself can be despawned by a room rebuild without having to
/// cancel the future explicitly.
#[derive(Component)]
pub struct PortalAvatarTask {
    task: bevy::tasks::Task<crate::avatar::AvatarFetchResult>,
    material: Handle<StandardMaterial>,
}

/// Marker attached to every entity spawned from the active `RoomRecord`.
/// Despawning all of these is how the compiler applies a record update
/// without double-spawning anything.
#[derive(Component)]
pub struct RoomEntity;

/// Tags the root entity of a `Placement::Absolute` with its index into the
/// live `RoomRecord::placements` vec. `editor_gizmo` reads this to map a
/// selected-in-UI placement to its 3D entity and to commit the gizmo's
/// final Transform back into the record when the user releases the mouse.
#[derive(Component)]
pub struct PlacementMarker(pub usize);

/// Tags every entity spawned from a node in a `Generator::Construct`
/// blueprint. Carries the generator's name plus the child-index chain from
/// the blueprint root so `editor_gizmo` can (a) find every live instance
/// matching a UI-selected prim and (b) resolve the dragged Transform back
/// to its slot in the recipe. The path for the blueprint root is an empty
/// `Vec`; each descendant appends its child index at each depth.
#[derive(Component, Clone)]
pub struct PrimMarker {
    pub generator_ref: String,
    pub path: Vec<usize>,
}

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

/// Cached geometry for a single L-system generator: the fingerprint of the
/// geometry-affecting settings that produced it, the shared per-material mesh
/// handles, and the skeleton's prop list. Props are stored raw because the
/// prop→mesh mapping and prop scale are resolved per-spawn against the
/// current generator settings.
struct CachedLSystemGeometry {
    geometry_hash: u64,
    mesh_buckets: Vec<(u8, Handle<Mesh>)>,
    props: Vec<SkeletonProp>,
}

/// Persistent cross-compile cache for L-system mesh geometry.
///
/// A `Placement::Scatter` with `count = 100_000` referencing an LSystem
/// generator would otherwise re-parse the grammar, re-derive the state,
/// re-interpret the turtle and re-upload a fresh `Handle<Mesh>` per scatter
/// point on the main thread. Because all scattered instances of the same
/// generator share identical geometry (only the parent transform varies),
/// we derive, interpret and mesh **once** per `(generator_ref, geometry_hash)`
/// pair and reuse the resulting `Handle<Mesh>` across every spawn.
///
/// Keyed by `generator_ref` and invalidated by hashing the geometry-relevant
/// fields (source, finalization, iterations, seed, angle/step/width/
/// elasticity, tropism, mesh resolution) in their fixed-point wire form.
/// Material settings are orthogonal — those live in `LSystemMaterialCache`
/// so a pure colour edit re-uses the cached mesh handles as-is.
///
/// Entries for `generator_ref`s not touched during a compile pass are dropped
/// at the end of that pass so stale meshes don't keep pinning `Assets<Mesh>`.
#[derive(Resource, Default)]
pub struct LSystemMeshCache {
    entries: HashMap<String, CachedLSystemGeometry>,
}

pub struct WorldBuilderPlugin;

impl Plugin for WorldBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<OverlandsFoliageTasks>()
            .init_resource::<LSystemMaterialCache>()
            .init_resource::<LSystemMeshCache>()
            .add_systems(Startup, setup_prop_assets)
            .add_systems(
                Update,
                (
                    compile_room_record,
                    apply_environment_state,
                    poll_overlands_foliage_tasks,
                    poll_portal_avatar_tasks,
                    draw_placement_visualizers,
                )
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
/// placement. Re-runs whenever the record resource is marked changed *or*
/// `FinishedHeightMap` is inserted/modified. The first frame inside
/// `AppState::InGame` counts as a change because the resource was just
/// inserted during Loading, which performs the initial compilation for free.
///
/// The heightmap trigger matters for scatter placements: when the owner
/// edits terrain config, `maybe_regenerate_terrain` tears down the old
/// heightmap and kicks off async regen in the same frame that
/// `record.is_changed()` fires. The initial rebuild scatters against the
/// stale heightmap; when the new one lands via `poll_terrain_task` the
/// record itself hasn't changed again, so without this second trigger
/// scatter y-positions would stay locked to the old ground.
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
    mut lsystem_mesh_cache: ResMut<LSystemMeshCache>,
    current_room: Option<Res<CurrentRoomDid>>,
) {
    let Some(record) = record else {
        return;
    };
    let heightmap_changed = heightmap.as_ref().is_some_and(|h| h.is_changed());
    if !record.is_changed() && !heightmap_changed {
        return;
    }

    // Step 1 — Cleanup. Despawn every entity previously compiled out of
    // this record. Terrain is NOT a `RoomEntity` (it is owned by the
    // terrain plugin's own lifecycle), so it survives the rebuild.
    //
    // `try_despawn` (instead of `despawn`) tolerates double-despawn: every
    // construct prim now carries its own `RoomEntity`, so when the parent
    // anchor's recursive-despawn removes the tree, subsequent iterations
    // for individual prims would log warnings otherwise. The extra marker
    // is load-bearing for gizmo-detached prims — they sit outside the
    // anchor's hierarchy, so the recursive sweep can't catch them, and the
    // flat `RoomEntity` iteration is the only thing that cleans them up.
    for e in &existing {
        commands.entity(e).try_despawn();
    }

    // Step 2 — Environment is applied by `apply_environment_state`, which
    // runs as its own system. Splitting it out keeps `compile_room_record`
    // under Bevy's 16-param limit on `IntoSystem` impls now that the
    // record carries sky / ambient / fog fields as well as the sun.

    // Cross-compile cache lives in `LSystemMaterialCache` (a persistent
    // Resource). Track which `(generator_ref, slot)` keys were touched this
    // pass so we can drop stale entries at the end — a generator removed
    // from the record would otherwise keep its handles pinned forever.
    let mut lsystem_cache_touched: HashSet<(String, u8)> = HashSet::new();
    // Parallel touch-set for the per-generator mesh cache (see `LSystemMeshCache`).
    let mut lsystem_mesh_touched: HashSet<String> = HashSet::new();

    // Step 3 — Placements. Walk the recipe; each scatter placement uses
    // its own deterministic RNG so every peer reproduces the same layout.
    let mut ctx = SpawnCtx {
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
        lsystem_mesh_cache: &mut lsystem_mesh_cache,
        lsystem_mesh_touched: &mut lsystem_mesh_touched,
        current_room: current_room.as_deref(),
    };

    for (placement_index, placement) in record.placements.iter().enumerate() {
        let (anchor_tf, snap) = match placement {
            Placement::Absolute { transform, snap_to_terrain, .. } => {
                (transform_from_data(transform).with_scale(Vec3::ONE), *snap_to_terrain)
            }
            Placement::Scatter { bounds, snap_to_terrain, .. } => {
                let center = match bounds {
                    ScatterBounds::Circle { center, .. } => Vec3::new(center.0[0], 0.0, center.0[1]),
                    ScatterBounds::Rect { center, .. } => Vec3::new(center.0[0], 0.0, center.0[1]),
                };
                let rot = match bounds {
                    ScatterBounds::Circle { .. } => Quat::IDENTITY,
                    ScatterBounds::Rect { rotation, .. } => Quat::from_rotation_y(rotation.0),
                };
                (Transform::from_translation(center).with_rotation(rot), *snap_to_terrain)
            }
            Placement::Grid { transform, snap_to_terrain, .. } => {
                (transform_from_data(transform).with_scale(Vec3::ONE), *snap_to_terrain)
            }
            Placement::Unknown => continue,
        };

        // Resolve Anchor world Y if snapped
        let mut anchor_world_tf = anchor_tf;
        if snap {
            if let Some(hm_res) = heightmap.as_deref() {
                let hm = &hm_res.0;
                let extent = (hm.width() - 1) as f32 * hm.scale();
                let half = extent * 0.5;
                let hm_x = (anchor_world_tf.translation.x + half).clamp(0.0, extent);
                let hm_z = (anchor_world_tf.translation.z + half).clamp(0.0, extent);
                anchor_world_tf.translation.y = hm.get_height_at(hm_x, hm_z);
            } else {
                anchor_world_tf.translation.y = 0.0;
            }
        }

        // The unified Anchor Entity
        let anchor = ctx.commands.spawn((
            anchor_world_tf,
            Visibility::default(),
            RigidBody::Static,
            PlacementMarker(placement_index),
            RoomEntity,
        )).id();

        match placement {
            Placement::Absolute { generator_ref, .. } => {
                if let Some(entity) = spawn_from_generator(&mut ctx, generator_ref, Transform::IDENTITY) {
                    ctx.commands.entity(anchor).add_child(entity);
                }
            }
            Placement::Grid { generator_ref, counts, gaps, random_yaw, .. } => {
                let [cx, cy, cz] = *counts;
                let [gx, gy, gz] = gaps.0;
                let start_x = -((cx as f32 - 1.0) * gx) / 2.0;
                let start_y = -((cy as f32 - 1.0) * gy) / 2.0;
                let start_z = -((cz as f32 - 1.0) * gz) / 2.0;

                // Per-placement RNG so yaw stays deterministic across peers
                // without adding a user-facing seed field to Grid.
                let mut rng = if *random_yaw {
                    Some(ChaCha8Rng::seed_from_u64(placement_index as u64))
                } else {
                    None
                };

                for ix in 0..cx {
                    for iy in 0..cy {
                        for iz in 0..cz {
                            let local_x = start_x + (ix as f32) * gx;
                            let local_y = start_y + (iy as f32) * gy;
                            let local_z = start_z + (iz as f32) * gz;

                            let mut final_local_y = local_y;
                            if snap {
                                let world_pos = anchor_world_tf.transform_point(Vec3::new(local_x, 0.0, local_z));
                                let world_y = if let Some(hm_res) = heightmap.as_deref() {
                                    let hm = &hm_res.0;
                                    let extent = (hm.width() - 1) as f32 * hm.scale();
                                    let half = extent * 0.5;
                                    let hm_x = (world_pos.x + half).clamp(0.0, extent);
                                    let hm_z = (world_pos.z + half).clamp(0.0, extent);
                                    hm.get_height_at(hm_x, hm_z)
                                } else {
                                    0.0
                                };
                                let local_snapped = anchor_world_tf.compute_affine().inverse().transform_point3(Vec3::new(world_pos.x, world_y, world_pos.z));
                                final_local_y = local_snapped.y + local_y;
                            }

                            let rotation = if let Some(rng) = rng.as_mut() {
                                let yaw = unit_f32(rng) * std::f32::consts::PI;
                                Quat::from_rotation_y(yaw)
                            } else {
                                Quat::IDENTITY
                            };
                            let child_tf = Transform::from_xyz(local_x, final_local_y, local_z)
                                .with_rotation(rotation);
                            if let Some(entity) = spawn_from_generator(&mut ctx, generator_ref, child_tf) {
                                ctx.commands.entity(anchor).add_child(entity);
                            }
                        }
                    }
                }
            }
            Placement::Scatter { generator_ref, bounds, count, local_seed, biome_filter, random_yaw, .. } => {
                let terrain_cfg = crate::pds::find_terrain_config(ctx.record);
                let water_level = find_water_level_for_filter(ctx.record);
                let max_attempts = count.saturating_mul(10).max(*count);
                let mut rng = ChaCha8Rng::seed_from_u64(*local_seed);
                let mut spawned = 0u32;
                let mut attempts = 0u32;

                while spawned < *count && attempts < max_attempts {
                    attempts += 1;
                    let (world_x, world_z) = sample_bounds(bounds, &mut rng);

                    let (world_y, keep) = if let Some(hm_res) = heightmap.as_deref() {
                        let hm = &hm_res.0;
                        let extent = (hm.width() - 1) as f32 * hm.scale();
                        let half = extent * 0.5;
                        let hm_x = (world_x + half).clamp(0.0, extent);
                        let hm_z = (world_z + half).clamp(0.0, extent);
                        let y = hm.get_height_at(hm_x, hm_z);
                        let keep = if biome_filter.is_noop() {
                            true
                        } else {
                            // Without a terrain generator the biome allow-list
                            // has no channel to resolve against; treat any
                            // non-empty list as "never matches" so accidental
                            // biome filters on dry-land records don't silently
                            // pass through. The water clause still evaluates.
                            let biome = if let Some(tcfg) = terrain_cfg {
                                let normal = hm.get_normal_at(hm_x, hm_z);
                                let slope = (1.0 - normal[1]).max(0.0);
                                dominant_biome(tcfg, y, slope)
                            } else {
                                255
                            };
                            biome_filter.accepts(biome, y, water_level)
                        };
                        (y, keep)
                    } else {
                        (0.0, biome_filter.is_noop())
                    };

                    if !keep { continue; }

                    // Make scatter children of the anchor so grabbing the Gizmo moves the whole forest live.
                    // Always draw from `rng` so disabling `random_yaw` doesn't shift downstream
                    // samples — the spawn stream stays byte-identical across peers regardless.
                    let local_pos = anchor_world_tf.compute_affine().inverse().transform_point3(Vec3::new(world_x, world_y, world_z));
                    let yaw_sample = unit_f32(&mut rng) * std::f32::consts::PI;
                    let rotation = if *random_yaw {
                        Quat::from_rotation_y(yaw_sample)
                    } else {
                        Quat::IDENTITY
                    };
                    let child_tf = Transform::from_translation(local_pos).with_rotation(rotation);

                    if let Some(entity) = spawn_from_generator(&mut ctx, generator_ref, child_tf) {
                        ctx.commands.entity(anchor).add_child(entity);
                    }
                    spawned += 1;
                }

                if spawned < *count {
                    debug!("Scatter `{}` placed {}/{} points", generator_ref, spawned, count);
                }
            }
            Placement::Unknown => {}
        }
    }

    // Drop cache entries whose `(generator_ref, slot)` was not touched this
    // compile pass — that slot is no longer referenced by the record, so
    // keeping the handle alive would pin a `StandardMaterial` (and any
    // baked foliage textures it points at) in `Assets` forever.
    lsystem_material_cache
        .entries
        .retain(|k, _| lsystem_cache_touched.contains(k));
    // Same GC for cached meshes so a generator removed from the record
    // stops pinning its `Handle<Mesh>` entries in `Assets<Mesh>`.
    lsystem_mesh_cache
        .entries
        .retain(|k, _| lsystem_mesh_touched.contains(k));
}

/// Apply the active `RoomRecord`'s `Environment` to every atmospheric
/// resource in the scene — sun, ambient, sky cuboid, clear colour, and
/// distance fog. Runs on every `RoomRecord` change so an editor slider
/// (or peer broadcast) retints the world without restarting the session.
///
/// Kept separate from [`compile_room_record`] because the combined
/// signature would exceed Bevy's 16-param `IntoSystem` limit; splitting
/// it out also lets Bevy schedule the two passes in parallel when their
/// resource borrows don't conflict.
fn apply_environment_state(
    record: Option<Res<RoomRecord>>,
    mut lights: Query<&mut DirectionalLight>,
    mut clear_color: ResMut<ClearColor>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
    mut fog: Query<&mut DistanceFog>,
    skybox: Query<&MeshMaterial3d<StandardMaterial>, With<crate::SkyBox>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(record) = record else {
        return;
    };
    if !record.is_changed() {
        return;
    }
    let env = &record.environment;

    let Fp3(sun_c) = env.sun_color;
    for mut light in lights.iter_mut() {
        light.color = Color::srgb(sun_c[0], sun_c[1], sun_c[2]);
        light.illuminance = env.sun_illuminance.0;
    }

    ambient_light.brightness = env.ambient_brightness.0;

    let Fp3(sky_c) = env.sky_color;
    clear_color.0 = Color::srgb(sky_c[0], sky_c[1], sky_c[2]);
    for material_handle in skybox.iter() {
        if let Some(mat) = std_materials.get_mut(&material_handle.0) {
            mat.base_color = Color::srgb(sky_c[0], sky_c[1], sky_c[2]);
        }
    }

    let Fp4(fog_c) = env.fog_color;
    let Fp4(fog_sun_c) = env.fog_sun_color;
    let Fp3(ext_c) = env.fog_extinction;
    let Fp3(in_c) = env.fog_inscattering;
    for mut dfog in fog.iter_mut() {
        dfog.color = Color::srgba(fog_c[0], fog_c[1], fog_c[2], fog_c[3]);
        dfog.directional_light_color =
            Color::srgba(fog_sun_c[0], fog_sun_c[1], fog_sun_c[2], fog_sun_c[3]);
        dfog.directional_light_exponent = env.fog_sun_exponent.0;
        dfog.falloff = FogFalloff::from_visibility_colors(
            env.fog_visibility.0,
            Color::srgb(ext_c[0], ext_c[1], ext_c[2]),
            Color::srgb(in_c[0], in_c[1], in_c[2]),
        );
    }
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

/// Stable content hash of the geometry-affecting fields of a `Generator::LSystem`.
/// Material / prop-mapping settings are deliberately excluded because those
/// are applied per-spawn on top of a shared mesh (see `LSystemMeshCache`).
/// Each `Fp` field is hashed via its fixed-point wire form so NaN/denormal
/// floats can't destabilise the key across compile passes.
#[allow(clippy::too_many_arguments)]
fn lsystem_geometry_fingerprint(
    source_code: &str,
    finalization_code: &str,
    iterations: u32,
    seed: u64,
    angle: Fp,
    step: Fp,
    width: Fp,
    elasticity: Fp,
    tropism: Option<Fp3>,
    mesh_resolution: u32,
) -> u64 {
    const FP_SCALE: f32 = 10_000.0;
    let fp = |v: f32| (v * FP_SCALE).round() as i32;
    let mut h = DefaultHasher::new();
    source_code.hash(&mut h);
    finalization_code.hash(&mut h);
    iterations.hash(&mut h);
    seed.hash(&mut h);
    fp(angle.0).hash(&mut h);
    fp(step.0).hash(&mut h);
    fp(width.0).hash(&mut h);
    fp(elasticity.0).hash(&mut h);
    match tropism {
        Some(t) => {
            1u8.hash(&mut h);
            fp(t.0[0]).hash(&mut h);
            fp(t.0[1]).hash(&mut h);
            fp(t.0[2]).hash(&mut h);
        }
        None => 0u8.hash(&mut h),
    }
    mesh_resolution.hash(&mut h);
    h.finish()
}

/// Pair of raw mesh buckets (keyed by material id) and the skeleton's prop
/// list — the cacheable output of an L-system build pass.
type LSystemGeometryBuild = (Vec<(u8, Mesh)>, Vec<SkeletonProp>);

/// Parse, derive, interpret and mesh an L-system generator. Returns the raw
/// mesh buckets keyed by material id, plus the skeleton's prop list. `None`
/// on grammar errors or empty state so the caller can skip the spawn.
///
/// Split out of `spawn_lsystem_entity` so `LSystemMeshCache` can invoke the
/// expensive pipeline at most once per `(generator_ref, geometry_hash)` pair.
#[allow(clippy::too_many_arguments)]
fn build_lsystem_geometry(
    source_code: &str,
    finalization_code: &str,
    iterations: u32,
    seed: u64,
    angle: Fp,
    step: Fp,
    width: Fp,
    elasticity: Fp,
    tropism: Option<Fp3>,
    mesh_resolution: u32,
    generator_ref: &str,
) -> Option<LSystemGeometryBuild> {
    let mut sys = System::new();
    sys.set_seed(seed);

    for (i, line) in source_code.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with('#') {
            if let Err(e) = sys.add_directive(trimmed) {
                warn!("L-system `{}` line {}: {}", generator_ref, i + 1, e);
                return None;
            }
            continue;
        }
        if let Some(axiom) = trimmed.strip_prefix("omega:") {
            if let Err(e) = sys.set_axiom(axiom.trim()) {
                warn!("L-system `{}` axiom error: {}", generator_ref, e);
                return None;
            }
            continue;
        }
        if let Err(e) = sys.add_rule(trimmed) {
            warn!("L-system `{}` rule error: {}", generator_ref, e);
            return None;
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
    for _ in 0..iterations {
        if let Err(e) = sys.derive(1) {
            warn!("L-system `{}` derivation error: {}", generator_ref, e);
            return None;
        }
        if sys.state.len() > MAX_LSYSTEM_STATE_LEN {
            warn!(
                "L-system `{}` state exceeded {} symbols — aborting derivation",
                generator_ref, MAX_LSYSTEM_STATE_LEN
            );
            return None;
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
                    return None;
                }
                continue;
            }
            if let Err(e) = sys.add_rule(trimmed) {
                warn!(
                    "L-system `{}` finalization rule error: {}",
                    generator_ref, e
                );
                return None;
            }
        }
        if let Err(e) = sys.derive(1) {
            warn!(
                "L-system `{}` finalization derivation error: {}",
                generator_ref, e
            );
            return None;
        }
        if sys.state.len() > MAX_LSYSTEM_STATE_LEN {
            warn!(
                "L-system `{}` finalization exceeded {} symbols — aborting",
                generator_ref, MAX_LSYSTEM_STATE_LEN
            );
            return None;
        }
    }

    if sys.state.is_empty() {
        return None;
    }

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

    // Each material ID produces a separate mesh bucket.
    let mesh_buckets: Vec<(u8, Mesh)> = LSystemMeshBuilder::new()
        .with_resolution(mesh_resolution.max(3))
        .build(&skeleton)
        .into_iter()
        .collect();

    Some((mesh_buckets, skeleton.props))
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
        ScatterBounds::Rect { center, extents, rotation } => {
            let lx = unit_f32(rng) * extents.0[0];
            let lz = unit_f32(rng) * extents.0[1];
            let rot = rotation.0;
            let rx = lx * rot.cos() - lz * rot.sin();
            let rz = lx * rot.sin() + lz * rot.cos();
            (center.0[0] + rx, center.0[1] + rz)
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

/// Compute the world-space Y of the first water generator's surface for use
/// by `BiomeFilter` water-relation checks. Walks generators in sorted key
/// order so every peer picks the same water level; when no water generator is
/// present we return `None` and the filter collapses to accept-by-default.
///
/// The computation mirrors `spawn_water_volume`: base sea level comes from
/// the compile-time `tcfg::water::LEVEL_FACTOR * HEIGHT_SCALE` constant, plus
/// the generator's `level_offset`, plus the water's placement-Y when the
/// record happens to place the volume off-origin.
fn find_water_level_for_filter(record: &RoomRecord) -> Option<f32> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in &keys {
        if let Some(Generator::Water { level_offset }) = record.generators.get(*k) {
            let placement_y = record.placements.iter().find_map(|p| match p {
                Placement::Absolute { generator_ref, transform, .. } if generator_ref == *k => {
                    Some(transform.translation.0[1])
                }
                _ => None,
            }).unwrap_or(0.0);
            let base_wl = tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE;
            let wl = (base_wl + level_offset.0).max(0.001);
            return Some(placement_y + wl);
        }
    }
    None
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
    /// Persistent mesh cache. A single scatter placement with `count=100_000`
    /// would otherwise re-derive / re-interpret / re-mesh the L-system on
    /// every spawn, pegging the main thread for minutes and allocating
    /// 100_000 unique `Handle<Mesh>` entries. The cache keys on
    /// `generator_ref` and reuses the baked `Handle<Mesh>` bucket across
    /// every scatter point whenever the geometry fingerprint matches.
    lsystem_mesh_cache: &'a mut LSystemMeshCache,
    /// `generator_ref` keys touched this compile pass so the caller can GC
    /// meshes belonging to generators removed from the record.
    lsystem_mesh_touched: &'a mut HashSet<String>,
    /// DID of the room we're currently compiling. Portal generators skip the
    /// ATProto profile-picture fetch when `target_did` equals this (an
    /// intra-room portal has no remote identity to paint onto its top face).
    current_room: Option<&'a CurrentRoomDid>,
}

fn spawn_from_generator(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator_ref: &str,
    transform: Transform,
) -> Option<Entity> {
    let Some(generator) = ctx.record.generators.get(generator_ref) else {
        warn!(
            "Placement references unknown generator `{}` — skipped",
            generator_ref
        );
        return None;
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
            // Terrain is never a placement root — its entities predate the
            // recipe compile pass and are owned by the terrain plugin.
            None
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
            Some(entity)
        }
        Generator::LSystem { .. } => {
            spawn_lsystem_entity(ctx, generator, generator_ref, transform)
        }
        Generator::Shape { .. } => {
            // Stub: symbios-shape integration lands in a follow-up.
            None
        }
        Generator::Construct { root } => Some(spawn_construct_entity(
            ctx,
            root,
            generator_ref,
            transform,
        )),
        Generator::Portal {
            target_did,
            target_pos,
        } => Some(spawn_portal_entity(
            ctx, target_did, target_pos, transform,
        )),
        Generator::Unknown => {
            warn!("Ignoring generator `{}` of unknown $type", generator_ref);
            None
        }
    }
}

/// Spawn a translucent sensor cube with a textured top face that the portal
/// interaction system reads. An inter-room portal kicks off an async profile
/// fetch for the target DID's avatar; an intra-room portal (`target_did` ==
/// current room) skips the fetch because the top face stays the fallback
/// white material.
fn spawn_portal_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    target_did: &str,
    target_pos: &Fp3,
    transform: Transform,
) -> Entity {
    let is_local = ctx.current_room.map(|r| r.0 == target_did).unwrap_or(false);

    let cube_mat = ctx.std_materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.8, 1.0, 0.4),
        alpha_mode: AlphaMode::Blend,
        emissive: LinearRgba::rgb(0.5, 1.0, 2.0),
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    let parent = ctx
        .commands
        .spawn((
            Mesh3d(ctx.meshes.add(Cuboid::new(1.5, 2.0, 1.5))),
            MeshMaterial3d(cube_mat),
            transform,
            Collider::cuboid(1.5, 2.0, 1.5),
            Sensor,
            PortalMarker {
                target_did: target_did.to_string(),
                target_pos: Vec3::from_array(target_pos.0),
            },
            RoomEntity,
        ))
        .id();

    // Top face — a thin plane pinned just above the cube's top so it renders
    // on top of the translucent volume without z-fighting. `unlit` keeps the
    // profile picture legible at any sun angle.
    let top_mat = ctx.std_materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });

    let top_face = ctx
        .commands
        .spawn((
            Mesh3d(ctx.meshes.add(Plane3d::new(Vec3::Y, Vec2::new(0.75, 0.75)))),
            MeshMaterial3d(top_mat.clone()),
            Transform::from_xyz(0.0, 1.01, 0.0),
        ))
        .id();
    ctx.commands.entity(parent).add_child(top_face);

    if !is_local {
        let pool = AsyncComputeTaskPool::get();
        let did_clone = target_did.to_string();
        let task = pool.spawn(async move {
            let fut = crate::avatar::fetch_avatar_bytes(did_clone);
            #[cfg(target_arch = "wasm32")]
            {
                fut.await
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(fut)
            }
        });
        ctx.commands.spawn(PortalAvatarTask {
            task,
            material: top_mat,
        });
    }

    parent
}

/// Drain finished portal-avatar fetches and paint the resulting texture onto
/// the portal top face's material. Failed fetches leave the material at the
/// fallback white so the portal is still visible and interactable.
fn poll_portal_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PortalAvatarTask)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let Some(bytes) = result.bytes else {
            continue;
        };
        let Ok(dyn_img) = image::load_from_memory(&bytes) else {
            continue;
        };
        let img = Image::from_dynamic(
            dyn_img,
            true,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        if let Some(mat) = materials.get_mut(&task.material) {
            mat.base_color_texture = Some(images.add(img));
        }
    }
}

/// Spawn a `Construct` hierarchy under an invisible anchor entity.
///
/// The anchor holds the Placement's **world-space** transform and owns the
/// rigid body; the blueprint tree is attached as its child so every prim
/// entity's `Transform` stays in **local-blueprint-space**. That separation
/// lets the in-world Gizmo drag a prim and commit its `Transform` straight
/// back into the recipe with no matrix inversion — dragging the root of a
/// placed construct would otherwise bake the world placement into the
/// blueprint, corrupting every other instance of the same generator.
///
/// Procedural materials (Bark/Leaf/Twig) are deduped by settings
/// fingerprint within the tree so a chair built from 20 `Bark` cubes only
/// spawns the texture task once.
fn spawn_construct_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    root: &PrimNode,
    generator_ref: &str,
    placement_tf: Transform,
) -> Entity {
    let mut material_cache: HashMap<u64, Handle<StandardMaterial>> = HashMap::new();

    // World-space anchor. Owns the rigid body and the `RoomEntity` marker so
    // the whole subtree despawns together on the next compile pass. No mesh
    // or material — it only exists to isolate world space from blueprint
    // space.
    let anchor = ctx
        .commands
        .spawn((
            placement_tf,
            Visibility::default(),
            RigidBody::Static,
            RoomEntity,
        ))
        .id();

    // Blueprint root, spawned in its own local transform. Attaching it as a
    // child of the anchor composes the placement's world transform with the
    // blueprint root's local transform via Bevy's hierarchy, so visually the
    // whole tree lands exactly where the `Placement::Absolute` asked.
    let mut path: Vec<usize> = Vec::new();
    let root_child = spawn_prim_tree(
        ctx,
        root,
        transform_from_data(&root.transform),
        &mut material_cache,
        generator_ref,
        &mut path,
    );
    ctx.commands.entity(anchor).add_child(root_child);

    apply_traits(ctx.commands, anchor, ctx.record, generator_ref);
    anchor
}

fn spawn_prim_tree(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    node: &PrimNode,
    tf: Transform,
    material_cache: &mut HashMap<u64, Handle<StandardMaterial>>,
    generator_ref: &str,
    path: &mut Vec<usize>,
) -> Entity {
    let mesh = mesh_for_prim_shape(ctx.meshes, &node.shape);

    let hash = settings_fingerprint(&node.material);
    let material = if let Some(h) = material_cache.get(&hash) {
        h.clone()
    } else {
        let h = spawn_procedural_material(ctx, &node.material);
        material_cache.insert(hash, h.clone());
        h
    };

    let mut cmd = ctx.commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        tf,
        PrimMarker {
            generator_ref: generator_ref.to_string(),
            path: path.clone(),
        },
        // Per-prim `RoomEntity` so the compile-pass cleanup finds every
        // prim directly, not just through the anchor's recursive despawn.
        // A gizmo-detached prim has no `ChildOf` link back to the anchor
        // and would otherwise survive the rebuild as a dangling ghost.
        RoomEntity,
    ));
    if node.solid
        && let Some(collider) = collider_for_prim_shape(&node.shape)
    {
        cmd.insert(collider);
    }
    let entity = cmd.id();

    for (i, child_node) in node.children.iter().enumerate() {
        path.push(i);
        let child_tf = transform_from_data(&child_node.transform);
        let child = spawn_prim_tree(
            ctx,
            child_node,
            child_tf,
            material_cache,
            generator_ref,
            path,
        );
        ctx.commands.entity(entity).add_child(child);
        path.pop();
    }
    entity
}

/// Build the parametric mesh for a [`PrimShape`]. The node's
/// [`TransformData::scale`] is applied via Bevy's transform hierarchy on
/// top of the shape's intrinsic dimensions.
fn mesh_for_prim_shape(meshes: &mut Assets<Mesh>, shape: &PrimShape) -> Handle<Mesh> {
    let mut mesh = match shape {
        PrimShape::Cuboid { size } => Cuboid::new(size.0[0], size.0[1], size.0[2]).mesh().build(),
        PrimShape::Sphere { radius, resolution } => Sphere::new(radius.0)
            .mesh()
            .ico(*resolution)
            .unwrap_or_else(|_| Sphere::new(radius.0).mesh().build()),
        PrimShape::Cylinder {
            radius,
            height,
            resolution,
        } => Cylinder::new(radius.0, height.0)
            .mesh()
            .resolution(*resolution)
            .build(),
        PrimShape::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
        } => Capsule3d::new(radius.0, length.0)
            .mesh()
            .latitudes(*latitudes)
            .longitudes(*longitudes)
            .build(),
        PrimShape::Cone {
            radius,
            height,
            resolution,
        } => Cone::new(radius.0, height.0)
            .mesh()
            .resolution(*resolution)
            .build(),
        PrimShape::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
        } => Torus {
            minor_radius: minor_radius.0,
            major_radius: major_radius.0,
        }
        .mesh()
        .minor_resolution(*minor_resolution as usize)
        .major_resolution(*major_resolution as usize)
        .build(),
        PrimShape::Plane { size, subdivisions } => {
            Plane3d::new(Vec3::Y, Vec2::new(size.0[0] / 2.0, size.0[1] / 2.0))
                .mesh()
                .subdivisions(*subdivisions)
                .build()
        }
        PrimShape::Tetrahedron { size } => {
            let s = size.0;
            let p0 = Vec3::new(0.0, 1.0, 0.0) * s;
            let p1 = Vec3::new(-1.0, -1.0, 1.0).normalize() * s;
            let p2 = Vec3::new(1.0, -1.0, 1.0).normalize() * s;
            let p3 = Vec3::new(0.0, -1.0, -1.0).normalize() * s;
            Tetrahedron::new(p0, p1, p2, p3).mesh().build()
        }
    };
    let _ = mesh.generate_tangents();
    meshes.add(mesh)
}

/// Build the Avian collider matching a [`PrimShape`]'s mesh. `Torus` and
/// `Plane` fall back to bounding cuboids because Avian 0.6 has no native
/// primitives for them; `Tetrahedron` uses a convex hull.
fn collider_for_prim_shape(shape: &PrimShape) -> Option<Collider> {
    Some(match shape {
        PrimShape::Cuboid { size } => Collider::cuboid(size.0[0], size.0[1], size.0[2]),
        PrimShape::Sphere { radius, .. } => Collider::sphere(radius.0),
        PrimShape::Cylinder { radius, height, .. } => Collider::cylinder(radius.0, height.0),
        PrimShape::Capsule { radius, length, .. } => Collider::capsule(radius.0, length.0),
        PrimShape::Cone { radius, height, .. } => Collider::cone(radius.0, height.0),
        PrimShape::Torus {
            minor_radius,
            major_radius,
            ..
        } => Collider::cuboid(
            major_radius.0 + minor_radius.0,
            minor_radius.0 * 2.0,
            major_radius.0 + minor_radius.0,
        ),
        PrimShape::Plane { size, .. } => Collider::cuboid(size.0[0], 0.01, size.0[1]),
        PrimShape::Tetrahedron { size } => {
            let s = size.0;
            let p0 = Vec3::new(0.0, 1.0, 0.0) * s;
            let p1 = Vec3::new(-1.0, -1.0, 1.0).normalize() * s;
            let p2 = Vec3::new(1.0, -1.0, 1.0).normalize() * s;
            let p3 = Vec3::new(0.0, -1.0, -1.0).normalize() * s;
            Collider::convex_hull(vec![p0, p1, p2, p3]).unwrap_or_else(|| Collider::sphere(s))
        }
    })
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
) -> Option<Entity> {
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
        return None;
    };

    // Reuse cached geometry when the geometry-affecting settings are
    // unchanged. A scatter placement with count=100_000 would otherwise
    // re-derive the grammar, re-walk the turtle and re-upload 100_000
    // `Handle<Mesh>` entries per scatter point on the main thread.
    ctx.lsystem_mesh_touched.insert(generator_ref.to_string());
    let geometry_hash = lsystem_geometry_fingerprint(
        source_code,
        finalization_code,
        *iterations,
        *seed,
        *angle,
        *step,
        *width,
        *elasticity,
        *tropism,
        *mesh_resolution,
    );
    let geometry = match ctx.lsystem_mesh_cache.entries.get(generator_ref) {
        Some(c) if c.geometry_hash == geometry_hash => {
            Some((c.mesh_buckets.clone(), c.props.clone()))
        }
        _ => None,
    };

    let (mesh_bucket_handles, props) = match geometry {
        Some(g) => g,
        None => {
            let Some((mesh_buckets_raw, skeleton_props)) = build_lsystem_geometry(
                source_code,
                finalization_code,
                *iterations,
                *seed,
                *angle,
                *step,
                *width,
                *elasticity,
                *tropism,
                *mesh_resolution,
                generator_ref,
            ) else {
                // Grammar rejected or empty state — evict any stale entry
                // so a later edit that fixes the grammar triggers a rebuild
                // instead of reusing invalid geometry.
                ctx.lsystem_mesh_cache.entries.remove(generator_ref);
                return None;
            };
            let bucket_handles: Vec<(u8, Handle<Mesh>)> = mesh_buckets_raw
                .into_iter()
                .map(|(mat_id, mesh)| (mat_id, ctx.meshes.add(mesh)))
                .collect();
            ctx.lsystem_mesh_cache.entries.insert(
                generator_ref.to_string(),
                CachedLSystemGeometry {
                    geometry_hash,
                    mesh_buckets: bucket_handles.clone(),
                    props: skeleton_props.clone(),
                },
            );
            (bucket_handles, skeleton_props)
        }
    };

    // Parent every mesh under a single transform so the placement's
    // rotation/position anchors the whole plant/shape as a unit.
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
                    let handle = spawn_procedural_material(ctx, settings);
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

    for (material_id, mesh_handle) in &mesh_bucket_handles {
        let material = slot_handles
            .get(material_id)
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
                Mesh3d(mesh_handle.clone()),
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
        for prop in &props {
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
    Some(parent)
}

/// Build a `StandardMaterial` from sovereign settings, enqueuing an async
/// texture-generation task for any [`SovereignTextureConfig`] variant.
/// Returns a handle that the caller installs on every strand / prop
/// belonging to the slot.
fn spawn_procedural_material(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    settings: &SovereignMaterialSettings,
) -> Handle<StandardMaterial> {
    build_procedural_material(ctx.std_materials, ctx.foliage_tasks, settings)
}

/// Free-function core of [`spawn_procedural_material`] — takes the two
/// resources it actually needs instead of the full [`SpawnCtx`], so avatar
/// builders can reuse it without constructing a world-builder context.
/// Returns a [`StandardMaterial`] handle whose texture slots are populated
/// asynchronously once the texture-generator task finishes.
pub fn build_procedural_material(
    std_materials: &mut Assets<StandardMaterial>,
    foliage_tasks: &mut OverlandsFoliageTasks,
    settings: &SovereignMaterialSettings,
) -> Handle<StandardMaterial> {
    let emissive = Color::srgb_from_array(settings.emission_color.0).to_linear()
        * settings.emission_strength.0;

    let (alpha_mode, double_sided, cull_mode, is_card) = settings.texture.render_properties();

    let handle = std_materials.add(StandardMaterial {
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
    macro_rules! spawn_gen {
        ($gen:ty, $cfg:expr) => {{
            let config = $cfg;
            let task = pool.spawn(async move { <$gen>::new(config).generate(512, 512) });
            foliage_tasks.tasks.push((task, handle.clone(), is_card));
        }};
    }

    match &settings.texture {
        SovereignTextureConfig::None | SovereignTextureConfig::Unknown => {}
        SovereignTextureConfig::Leaf(c) => spawn_gen!(LeafGenerator, c.to_native()),
        SovereignTextureConfig::Twig(c) => spawn_gen!(TwigGenerator, c.to_native()),
        SovereignTextureConfig::Bark(c) => spawn_gen!(BarkGenerator, c.to_native()),
        SovereignTextureConfig::Window(c) => spawn_gen!(WindowGenerator, c.to_native()),
        SovereignTextureConfig::StainedGlass(c) => {
            spawn_gen!(StainedGlassGenerator, c.to_native())
        }
        SovereignTextureConfig::IronGrille(c) => spawn_gen!(IronGrilleGenerator, c.to_native()),
        SovereignTextureConfig::Ground(c) => spawn_gen!(GroundGenerator, c.to_native()),
        SovereignTextureConfig::Rock(c) => spawn_gen!(RockGenerator, c.to_native()),
        SovereignTextureConfig::Brick(c) => spawn_gen!(BrickGenerator, c.to_native()),
        SovereignTextureConfig::Plank(c) => spawn_gen!(PlankGenerator, c.to_native()),
        SovereignTextureConfig::Shingle(c) => spawn_gen!(ShingleGenerator, c.to_native()),
        SovereignTextureConfig::Stucco(c) => spawn_gen!(StuccoGenerator, c.to_native()),
        SovereignTextureConfig::Concrete(c) => spawn_gen!(ConcreteGenerator, c.to_native()),
        SovereignTextureConfig::Metal(c) => spawn_gen!(MetalGenerator, c.to_native()),
        SovereignTextureConfig::Pavers(c) => spawn_gen!(PaversGenerator, c.to_native()),
        SovereignTextureConfig::Ashlar(c) => spawn_gen!(AshlarGenerator, c.to_native()),
        SovereignTextureConfig::Cobblestone(c) => {
            spawn_gen!(CobblestoneGenerator, c.to_native())
        }
        SovereignTextureConfig::Thatch(c) => spawn_gen!(ThatchGenerator, c.to_native()),
        SovereignTextureConfig::Marble(c) => spawn_gen!(MarbleGenerator, c.to_native()),
        SovereignTextureConfig::Corrugated(c) => spawn_gen!(CorrugatedGenerator, c.to_native()),
        SovereignTextureConfig::Asphalt(c) => spawn_gen!(AsphaltGenerator, c.to_native()),
        SovereignTextureConfig::Wainscoting(c) => {
            spawn_gen!(WainscotingGenerator, c.to_native())
        }
        SovereignTextureConfig::Encaustic(c) => spawn_gen!(EncausticGenerator, c.to_native()),
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

fn draw_placement_visualizers(
    mut gizmos: Gizmos,
    editor_state: Res<crate::ui::room::RoomEditorState>,
    record: Option<Res<RoomRecord>>,
    heightmap: Option<Res<FinishedHeightMap>>,
) {
    let Some(record) = record else { return; };
    if editor_state.selected_tab != crate::ui::room::EditorTab::Placements { return; }
    let Some(idx) = editor_state.selected_placement else { return; };
    let Some(placement) = record.placements.get(idx) else { return; };

    let get_y = |x: f32, z: f32| -> f32 {
        if let Some(hm_res) = heightmap.as_deref() {
            let hm = &hm_res.0;
            let extent = (hm.width() - 1) as f32 * hm.scale();
            let half = extent * 0.5;
            let hm_x = (x + half).clamp(0.0, extent);
            let hm_z = (z + half).clamp(0.0, extent);
            hm.get_height_at(hm_x, hm_z)
        } else {
            0.0
        }
    };

    let color = Color::srgb(0.0, 1.0, 0.5);

    match placement {
        Placement::Absolute { transform, snap_to_terrain, .. } => {
            let mut pos = Vec3::from_array(transform.translation.0);
            if *snap_to_terrain { pos.y = get_y(pos.x, pos.z); }
            gizmos.sphere(pos, 1.0, color);
        }
        Placement::Scatter { bounds, snap_to_terrain, .. } => {
            match bounds {
                ScatterBounds::Circle { center, radius } => {
                    let mut pos = Vec3::new(center.0[0], 0.0, center.0[1]);
                    if *snap_to_terrain { pos.y = get_y(pos.x, pos.z); }
                    let iso = Isometry3d::new(pos, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
                    gizmos.circle(iso, radius.0, color);
                }
                ScatterBounds::Rect { center, extents, rotation } => {
                    let mut pos = Vec3::new(center.0[0], 0.0, center.0[1]);
                    if *snap_to_terrain { pos.y = get_y(pos.x, pos.z); }
                    // Align the rect to lie flat on the XZ plane
                    let rot = Quat::from_rotation_y(rotation.0) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                    let size = Vec2::new(extents.0[0] * 2.0, extents.0[1] * 2.0);
                    gizmos.rect(Isometry3d::new(pos, rot), size, color);
                }
            }
        }
        Placement::Grid { transform, counts, gaps, snap_to_terrain, .. } => {
            let mut pos = Vec3::from_array(transform.translation.0);
            if *snap_to_terrain { pos.y = get_y(pos.x, pos.z); }
            let rot = Quat::from_array(transform.rotation.0);
            let w = ((counts[0] as f32) - 1.0).max(0.0) * gaps.0[0];
            let h = ((counts[1] as f32) - 1.0).max(0.0) * gaps.0[1];
            let d = ((counts[2] as f32) - 1.0).max(0.0) * gaps.0[2];

            // Draw 3 intersecting planes as an elegant bounding volume visualization
            let iso = Isometry3d::new(pos, rot);
            gizmos.rect(iso, Vec2::new(w + 1.0, d + 1.0), color);
            gizmos.rect(Isometry3d::new(pos, rot * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), Vec2::new(w + 1.0, h + 1.0), color);
            gizmos.rect(Isometry3d::new(pos, rot * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)), Vec2::new(d + 1.0, h + 1.0), color);
        }
        _ => {}
    }
}
