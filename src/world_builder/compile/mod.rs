//! Room-record → ECS compilation pass: despawns last pass's `RoomEntity`
//! set, re-walks `RoomRecord::placements`, dispatches into the per-generator
//! spawners, applies atmospheric `Environment` state, and carries the
//! scattered-sample math and helpers used by `Placement::Scatter` /
//! `Placement::Grid`.
//!
//! ## Sub-module map
//!
//! * [`spawn_ctx`] — [`SpawnCtx`] (the per-pass write-context shared with
//!   every sibling spawner module), [`GeneratorCaches`] system param,
//!   [`MAX_ROOM_ENTITIES`] cap + [`budget_exceeded`] gate, and
//!   [`transform_from_data`] helper.
//! * [`environment`] — [`apply_environment_state`] (runs as its own
//!   system; split out so the combined signature stays under Bevy's
//!   16-param `IntoSystem` ceiling).
//! * [`scatter`] — placement sampling helpers ([`sample_bounds`],
//!   [`unit_f32`]) and the biome-rule evaluator ([`rule_weight`],
//!   [`smooth_range`], [`dominant_biome`]).
//! * [`dispatch`] — recursive [`spawn_generator`] + [`dispatch_top_level`]
//!   walker. Routes each [`GeneratorKind`] variant into its sibling
//!   spawner module (`prim`, `lsystem`, `shape`, `sign`, `portal`,
//!   `particles`, `material::spawn_water_volume`).

mod dispatch;
mod environment;
mod scatter;
mod spawn_ctx;

// External callers (`super::compile::SpawnCtx` etc.) reach these names
// through this re-export. Behavioural surface is identical to the
// pre-refactor flat `compile.rs`.
pub(super) use dispatch::dispatch_top_level;
pub use dispatch::spawn_generator;
pub(super) use environment::apply_environment_state;
pub(super) use spawn_ctx::transform_from_data;
pub use spawn_ctx::{GeneratorCaches, SpawnCtx, budget_exceeded};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use std::collections::HashSet;

use crate::pds::{Placement, RoomRecord, ScatterBounds};
use crate::state::CurrentRoomDid;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterSurfaces};

use super::image_cache::BlobImageCache;
use super::{OverlandsFoliageTasks, PlacementMarker, PropMeshAssets, RoomEntity};

use scatter::{dominant_biome, sample_bounds, unit_f32};

#[allow(clippy::too_many_arguments)]
pub(super) fn compile_room_record(
    mut commands: Commands,
    record: Option<Res<RoomRecord>>,
    existing: Query<Entity, With<RoomEntity>>,
    terrain_meshes: Query<Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    palette: Option<Res<MaterialPalette>>,
    prop_assets: Option<Res<PropMeshAssets>>,
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    mut generator_caches: GeneratorCaches,
    current_room: Option<Res<CurrentRoomDid>>,
    mut blob_image_cache: ResMut<BlobImageCache>,
    mut water_surfaces: ResMut<WaterSurfaces>,
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
    // child prim now carries its own `RoomEntity`, so when the parent
    // anchor's recursive-despawn removes the tree, subsequent iterations
    // for individual prims would log warnings otherwise. The extra marker
    // is load-bearing for gizmo-detached prims — they sit outside the
    // anchor's hierarchy, so the recursive sweep can't catch them, and the
    // flat `RoomEntity` iteration is the only thing that cleans them up.
    for e in &existing {
        commands.entity(e).try_despawn();
    }

    // The runtime water-surface registry is rebuilt every compile pass.
    // Cleared here so a record patch that removes a water generator drops
    // the corresponding entry from the lookup.
    water_surfaces.planes.clear();

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
    // Sister touch-sets for the Shape generator caches.
    let mut shape_material_touched: HashSet<(String, String)> = HashSet::new();
    let mut shape_mesh_touched: HashSet<String> = HashSet::new();

    // Multiplicative spawn budget. Per-axis sanitiser caps already bound a
    // single placement; this catches the case where many bounded placements
    // multiply into a frame-killing total.
    let mut entities_spawned: u32 = 0;
    let mut budget_warned: bool = false;

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
        lsystem_material_cache: &mut generator_caches.lsystem_material,
        lsystem_cache_touched: &mut lsystem_cache_touched,
        lsystem_mesh_cache: &mut generator_caches.lsystem_mesh,
        lsystem_mesh_touched: &mut lsystem_mesh_touched,
        shape_material_cache: &mut generator_caches.shape_material,
        shape_material_touched: &mut shape_material_touched,
        shape_mesh_cache: &mut generator_caches.shape_mesh,
        shape_mesh_touched: &mut shape_mesh_touched,
        current_room: current_room.as_deref(),
        entities_spawned: &mut entities_spawned,
        budget_warned: &mut budget_warned,
        blob_image_cache: &mut blob_image_cache,
        water_surfaces: &mut water_surfaces,
        avatar_mode: false,
        local_avatar_mode: false,
    };

    for (placement_index, placement) in record.placements.iter().enumerate() {
        if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
            break;
        }
        let (anchor_tf, snap) = match placement {
            Placement::Absolute {
                transform,
                snap_to_terrain,
                ..
            } => (
                transform_from_data(transform).with_scale(Vec3::ONE),
                *snap_to_terrain,
            ),
            Placement::Scatter {
                bounds,
                snap_to_terrain,
                ..
            } => {
                let center = match bounds {
                    ScatterBounds::Circle { center, .. } => {
                        Vec3::new(center.0[0], 0.0, center.0[1])
                    }
                    ScatterBounds::Rect { center, .. } => Vec3::new(center.0[0], 0.0, center.0[1]),
                };
                let rot = match bounds {
                    ScatterBounds::Circle { .. } => Quat::IDENTITY,
                    ScatterBounds::Rect { rotation, .. } => Quat::from_rotation_y(rotation.0),
                };
                (
                    Transform::from_translation(center).with_rotation(rot),
                    *snap_to_terrain,
                )
            }
            Placement::Grid {
                transform,
                snap_to_terrain,
                ..
            } => (
                transform_from_data(transform).with_scale(Vec3::ONE),
                *snap_to_terrain,
            ),
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

        // The unified outer Anchor entity. Every placement gets one, so a
        // top-level Cuboid and a deeply-nested fractal blueprint share the
        // same gizmo-friendly two-level layout: outer anchor at placement
        // pose, generator entity (and its descendants) at their own poses
        // beneath.
        let anchor = ctx
            .commands
            .spawn((
                anchor_world_tf,
                Visibility::default(),
                RigidBody::Static,
                PlacementMarker(placement_index),
                RoomEntity,
            ))
            .id();

        match placement {
            Placement::Absolute { generator_ref, .. } => {
                if let Some(entity) =
                    dispatch_top_level(&mut ctx, generator_ref, Transform::IDENTITY)
                {
                    ctx.commands.entity(anchor).add_child(entity);
                }
            }
            Placement::Grid {
                generator_ref,
                counts,
                gaps,
                random_yaw,
                ..
            } => {
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

                'grid: for ix in 0..cx {
                    for iy in 0..cy {
                        for iz in 0..cz {
                            if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
                                break 'grid;
                            }
                            let local_x = start_x + (ix as f32) * gx;
                            let local_y = start_y + (iy as f32) * gy;
                            let local_z = start_z + (iz as f32) * gz;

                            let mut final_local_y = local_y;
                            if snap {
                                let world_pos = anchor_world_tf
                                    .transform_point(Vec3::new(local_x, 0.0, local_z));
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
                                let local_snapped = anchor_world_tf
                                    .compute_affine()
                                    .inverse()
                                    .transform_point3(Vec3::new(world_pos.x, world_y, world_pos.z));
                                final_local_y = local_snapped.y + local_y;
                            }

                            let rotation = if let Some(rng) = rng.as_mut() {
                                let yaw = unit_f32(rng) * std::f32::consts::PI;
                                Quat::from_rotation_y(yaw)
                            } else {
                                Quat::IDENTITY
                            };
                            // Per-cell placement transform composes on top
                            // of the generator's own root transform inside
                            // `dispatch_top_level`. Yaw spins each cell
                            // around its own Y axis so identical
                            // blueprints don't all face the same way.
                            let cell_tf = Transform::from_xyz(local_x, final_local_y, local_z)
                                .with_rotation(rotation);
                            if let Some(entity) =
                                dispatch_top_level(&mut ctx, generator_ref, cell_tf)
                            {
                                ctx.commands.entity(anchor).add_child(entity);
                            }
                        }
                    }
                }
            }
            Placement::Scatter {
                generator_ref,
                bounds,
                count,
                local_seed,
                biome_filter,
                random_yaw,
                ..
            } => {
                let terrain_cfg = crate::pds::find_terrain_config(ctx.record);
                // Resolve the biome-filter water threshold from the runtime
                // registry rather than the record. Each scatter uses a
                // single global Y (matching the previous behavior), sampled
                // at the scatter's centre — placements that come before the
                // home-water spawn collapse to "no water" and the filter
                // accepts by default. Realistic rooms put Terrain first so
                // home water is in the registry by the time scatters run.
                let scatter_center_xz = match bounds {
                    ScatterBounds::Circle { center, .. } => Vec2::new(center.0[0], center.0[1]),
                    ScatterBounds::Rect { center, .. } => Vec2::new(center.0[0], center.0[1]),
                };
                let water_level = ctx
                    .water_surfaces
                    .surface_at(scatter_center_xz)
                    .map(|(_, y)| y);
                let max_attempts = count.saturating_mul(10).max(*count);
                let mut rng = ChaCha8Rng::seed_from_u64(*local_seed);
                let mut spawned = 0u32;
                let mut attempts = 0u32;

                while spawned < *count && attempts < max_attempts {
                    if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
                        break;
                    }
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

                    if !keep {
                        continue;
                    }

                    // Make scatter children of the anchor so grabbing the
                    // gizmo moves the whole forest live. Always draw from
                    // `rng` so disabling `random_yaw` doesn't shift
                    // downstream samples — the spawn stream stays
                    // byte-identical across peers regardless.
                    let local_pos = anchor_world_tf
                        .compute_affine()
                        .inverse()
                        .transform_point3(Vec3::new(world_x, world_y, world_z));
                    let yaw_sample = unit_f32(&mut rng) * std::f32::consts::PI;
                    let rotation = if *random_yaw {
                        Quat::from_rotation_y(yaw_sample)
                    } else {
                        Quat::IDENTITY
                    };
                    let cell_tf = Transform::from_translation(local_pos).with_rotation(rotation);

                    if let Some(entity) = dispatch_top_level(&mut ctx, generator_ref, cell_tf) {
                        ctx.commands.entity(anchor).add_child(entity);
                    }
                    spawned += 1;
                }

                if spawned < *count {
                    debug!(
                        "Scatter `{}` placed {}/{} points",
                        generator_ref, spawned, count
                    );
                }
            }
            Placement::Unknown => {}
        }
    }

    // Drop cache entries whose `(generator_ref, slot)` was not touched this
    // compile pass — that slot is no longer referenced by the record, so
    // keeping the handle alive would pin a `StandardMaterial` (and any
    // baked foliage textures it points at) in `Assets` forever.
    generator_caches
        .lsystem_material
        .entries
        .retain(|k, _| lsystem_cache_touched.contains(k));
    // Same GC for cached meshes so a generator removed from the record
    // stops pinning its `Handle<Mesh>` entries in `Assets<Mesh>`.
    generator_caches
        .lsystem_mesh
        .entries
        .retain(|k, _| lsystem_mesh_touched.contains(k));
    // Mirror the GC pass for the Shape generator caches.
    generator_caches
        .shape_material
        .entries
        .retain(|k, _| shape_material_touched.contains(k));
    generator_caches
        .shape_mesh
        .entries
        .retain(|k, _| shape_mesh_touched.contains(k));
}
