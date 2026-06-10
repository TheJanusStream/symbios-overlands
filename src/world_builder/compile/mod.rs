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
//!   [`MAX_ROOM_ENTITIES`](spawn_ctx::MAX_ROOM_ENTITIES) cap +
//!   [`budget_exceeded`] gate, and
//!   [`transform_from_data`] helper.
//! * [`environment`] — [`apply_environment_state`] (runs as its own
//!   system; split out so the combined signature stays under Bevy's
//!   16-param `IntoSystem` ceiling).
//! * [`scatter`] — placement sampling helpers ([`sample_bounds`],
//!   [`unit_f32`]) and the biome-rule evaluator (`convert_rule` +
//!   [`dominant_biome`]).
//! * [`dispatch`] — recursive [`spawn_generator`] + [`dispatch_top_level`]
//!   walker. Routes each [`GeneratorKind`] variant into its sibling
//!   spawner module (`prim`, `lsystem`, `shape`, `sign`, `portal`,
//!   `particles`, `material::spawn_water_volume`).
//! * [`contact_recipes`] — [`apply_contact_recipes`] system that
//!   translates the active [`crate::pds::ContactEffects`] block into the
//!   runtime [`crate::interaction::recipes::ContactRecipeRegistry`] each
//!   time the room record changes.

mod contact_recipes;
mod dispatch;
mod environment;
mod scatter;
mod spawn_ctx;

// External callers (`super::compile::SpawnCtx` etc.) reach these names
// through this re-export. Behavioural surface is identical to the
// pre-refactor flat `compile.rs`.
pub(super) use contact_recipes::apply_contact_recipes;
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

use crate::pds::{GeneratorKind, Placement, RoomRecord, ScatterBounds};
use crate::state::{CurrentRoomDid, LiveRoomRecord};
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterSurfaces};

use super::image_cache::BlobImageCache;
use super::{PlacementMarker, PropMeshAssets, RoomEntity};

use scatter::{dominant_biome, sample_bounds, unit_f32};

#[allow(clippy::too_many_arguments)]
pub(super) fn compile_room_record(
    mut commands: Commands,
    record: Option<Res<LiveRoomRecord>>,
    existing: Query<Entity, With<RoomEntity>>,
    terrain_meshes: Query<Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
    palette: Option<Res<MaterialPalette>>,
    prop_assets: Option<Res<PropMeshAssets>>,
    mut generator_caches: GeneratorCaches,
    current_room: Option<Res<CurrentRoomDid>>,
    mut blob_image_cache: ResMut<BlobImageCache>,
    mut blob_audio_cache: ResMut<super::audio_resolver::BlobAudioCache>,
    mut water_surfaces: ResMut<WaterSurfaces>,
) {
    let Some(record) = record else {
        return;
    };
    let heightmap_changed = heightmap.as_ref().is_some_and(|h| h.is_changed());
    if !record.is_changed() && !heightmap_changed {
        return;
    }
    // The change tick above is read off the `Res<LiveRoomRecord>`
    // wrapper; everything below wants the inner `RoomRecord`.
    let record = &record.0;

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
    let mut lsystem_cache_touched: HashSet<(String, u16)> = HashSet::new();
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
        record,
        meshes: &mut meshes,
        std_materials: &mut std_materials,
        water_materials: &mut water_materials,
        images: &mut images,
        palette: palette.as_deref(),
        heightmap: heightmap.as_deref(),
        terrain_meshes: &terrain_meshes,
        prop_assets: prop_assets.as_deref(),
        lsystem_material_cache: &mut generator_caches.lsystem_material,
        lsystem_cache_touched: &mut lsystem_cache_touched,
        lsystem_mesh_cache: &mut generator_caches.lsystem_mesh,
        lsystem_mesh_touched: &mut lsystem_mesh_touched,
        shape_material_cache: &mut generator_caches.shape_material,
        shape_material_touched: &mut shape_material_touched,
        shape_mesh_cache: &mut generator_caches.shape_mesh,
        upstream_shape_mesh_cache: &mut generator_caches.upstream_shape_mesh,
        shape_mesh_touched: &mut shape_mesh_touched,
        current_room: current_room.as_deref(),
        entities_spawned: &mut entities_spawned,
        budget_warned: &mut budget_warned,
        blob_image_cache: &mut blob_image_cache,
        blob_audio_cache: &mut blob_audio_cache,
        water_surfaces: &mut water_surfaces,
        avatar_mode: false,
        local_avatar_mode: false,
    };

    let room_water_y = room_water_level(record);

    for (placement_index, placement) in record.placements.iter().enumerate() {
        if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
            break;
        }
        let (anchor_tf, snap, avoid_water) = match placement {
            Placement::Absolute {
                transform,
                snap_to_terrain,
                avoid_water,
                avoid_water_clearance,
                ..
            } => (
                transform_from_data(transform).with_scale(Vec3::ONE),
                *snap_to_terrain,
                // Clearance scales with the placement's uniform scale so
                // a 1.2× landmark demands a 1.2× dry disc.
                avoid_water.then_some(avoid_water_clearance.0 * transform.scale.0[0].max(0.0)),
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
                    None,
                )
            }
            Placement::Grid {
                transform,
                snap_to_terrain,
                ..
            } => (
                transform_from_data(transform).with_scale(Vec3::ONE),
                *snap_to_terrain,
                None,
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
                // Water-avoiding placements slide to dry land before
                // the height sample (may move X/Z, preserves bearing).
                if let Some(clearance) = avoid_water
                    && let Some(water_y) = room_water_y
                {
                    relocate_above_water(
                        hm,
                        extent,
                        half,
                        &mut anchor_world_tf.translation,
                        water_y,
                        clearance,
                    );
                }
                let hm_x = (anchor_world_tf.translation.x + half).clamp(0.0, extent);
                let hm_z = (anchor_world_tf.translation.z + half).clamp(0.0, extent);
                // Absolute placements keep their authored Y as an
                // offset from the snapped terrain height (the seeded
                // landmark sinks its foundations 0.35 m); Scatter /
                // Grid anchors keep the historical replace semantics.
                let authored_y = if matches!(placement, Placement::Absolute { .. }) {
                    anchor_world_tf.translation.y
                } else {
                    0.0
                };
                anchor_world_tf.translation.y = hm.get_height_at(hm_x, hm_z) + authored_y;
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

/// The room's sea level: the highest Water child under any
/// Terrain-rooted generator (the canonical homeworld layout puts the
/// room's water plane there), or `None` for dry rooms. Water world Y
/// is the child's translation because the terrain anchor sits at the
/// origin unsnapped.
fn room_water_level(record: &RoomRecord) -> Option<f32> {
    record
        .generators
        .values()
        .filter(|g| matches!(g.kind, GeneratorKind::Terrain(_)))
        .flat_map(|g| g.children.iter())
        .filter(|c| matches!(c.kind, GeneratorKind::Water { .. }))
        .map(|c| c.transform.translation.0[1])
        .fold(None, |acc: Option<f32>, y| {
            Some(acc.map_or(y, |a| a.max(y)))
        })
}

/// Slide a water-avoiding anchor along its bearing through the origin
/// — alternating outward / inward in `DRY_STEP`-metre increments — to
/// the first probe where the terrain rises above the room's water
/// line plus a freeboard margin. Bearing-aligned steps keep a
/// spawn-facing yaw valid, and the walk is a pure function of the
/// shared heightmap, so every peer relocates the anchor identically.
/// Gives up after `DRY_MAX_PROBES` probes and leaves the anchor in
/// place (a flooded landmark beats a missing one).
fn relocate_above_water(
    hm: &bevy_symbios_ground::HeightMap,
    extent: f32,
    half: f32,
    translation: &mut Vec3,
    water_y: f32,
    clearance: f32,
) {
    /// Probe spacing along the bearing (m).
    const DRY_STEP: f32 = 6.0;
    /// Probe budget: 30 outward + 30 inward = ±180 m of shoreline hunt.
    const DRY_MAX_PROBES: u32 = 60;
    /// Required terrain clearance over the water line (m) — enough
    /// that a structure's plinth course stays dry.
    const FREEBOARD: f32 = 0.75;

    let sample = |x: f32, z: f32| {
        hm.get_height_at((x + half).clamp(0.0, extent), (z + half).clamp(0.0, extent))
    };
    // A candidate is dry when its centre and (for non-zero clearance) a
    // ring of eight points at the clearance radius all clear the water
    // line — a wide building can't pass on a dry anchor while its far
    // wing floods.
    let dry = |x: f32, z: f32| {
        if sample(x, z) < water_y + FREEBOARD {
            return false;
        }
        if clearance <= 0.0 {
            return true;
        }
        (0..8).all(|i| {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            sample(x + a.sin() * clearance, z + a.cos() * clearance) >= water_y + FREEBOARD
        })
    };
    let (x0, z0) = (translation.x, translation.z);
    if dry(x0, z0) {
        return;
    }
    let r0 = (x0 * x0 + z0 * z0).sqrt();
    if r0 < 1e-3 {
        // Anchored on the origin: no bearing to walk.
        return;
    }
    let (dx, dz) = (x0 / r0, z0 / r0);
    for i in 1..=DRY_MAX_PROBES {
        // Alternate +1, -1, +2, -2, … steps along the bearing.
        let sign = if i % 2 == 1 { 1.0 } else { -1.0 };
        let k = i.div_ceil(2) as f32 * DRY_STEP * sign;
        let r = r0 + k;
        // Inward probes stop short of the spawn square; outward ones
        // stay inside the heightmap.
        if !(4.0..=half).contains(&r) {
            continue;
        }
        let (x, z) = (dx * r, dz * r);
        if dry(x, z) {
            translation.x = x;
            translation.z = z;
            return;
        }
    }
}

#[cfg(test)]
mod water_avoidance_tests {
    use super::*;

    #[test]
    fn room_water_level_reads_seeded_record() {
        let record = RoomRecord::default_for_did("did:test:water");
        let level = room_water_level(&record).expect("seeded rooms always carry water");
        assert!(
            level >= 0.0,
            "seeded water sits at or above the terrain base"
        );
    }

    #[test]
    fn landmark_placement_opts_into_water_avoidance() {
        let record = RoomRecord::default_for_did("did:test:water");
        let landmark_avoids = record.placements.iter().any(|p| {
            matches!(
                p,
                Placement::Absolute {
                    generator_ref,
                    avoid_water: true,
                    snap_to_terrain: true,
                    ..
                } if generator_ref == "landmark"
            )
        });
        assert!(landmark_avoids, "seeded landmark must carry avoid_water");
    }

    #[test]
    fn dry_land_walk_slides_along_bearing_to_shore() {
        // Synthetic 129×129 heightmap, scale 1.0 → world X/Z in
        // [-64, 64]. Dry plateau (y = 5) where world X > 20, seabed
        // (y = 0) elsewhere; water line at y = 2.
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        for z in 0..129 {
            for x in 0..129 {
                let world_x = x as f32 - 64.0;
                hm.set(x, z, if world_x > 20.0 { 5.0 } else { 0.0 });
            }
        }
        let (extent, half) = (128.0, 64.0);

        // Submerged anchor at (10, 0), bearing +X: must slide outward
        // past the shoreline without leaving the bearing line.
        let mut t = Vec3::new(10.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut t, 2.0, 0.0);
        assert!(t.x > 20.0, "anchor should cross the shoreline: {t:?}");
        assert_eq!(t.z, 0.0, "walk must stay on the bearing line");

        // Already-dry anchors stay exactly put.
        let mut dry = Vec3::new(40.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut dry, 2.0, 0.0);
        assert_eq!(dry.x, 40.0);

        // A fully-drowned bearing gives up and leaves the anchor in
        // place rather than teleporting it somewhere arbitrary.
        let mut hopeless = Vec3::new(0.0, 0.0, -30.0);
        relocate_above_water(&hm, extent, half, &mut hopeless, 2.0, 0.0);
        assert_eq!((hopeless.x, hopeless.z), (0.0, -30.0));

        // Clearance ring: an anchor just past the shoreline (x = 22) is
        // dry at its centre but a 10 m footprint ring dips back into
        // the sea — the walk must push it further inland until the
        // whole disc clears.
        let mut wide = Vec3::new(22.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut wide, 2.0, 10.0);
        assert!(
            wide.x > 30.0,
            "ring-sampled anchor must move until the footprint clears: {wide:?}"
        );
    }
}
