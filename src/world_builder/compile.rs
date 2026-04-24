//! Room-record → ECS compilation pass: despawns last pass's `RoomEntity`
//! set, re-walks `RoomRecord::placements`, dispatches into the per-generator
//! spawners, applies atmospheric `Environment` state, and carries the
//! scattered-sample math and helpers used by `Placement::Scatter` /
//! `Placement::Grid`.
//!
//! `SpawnCtx` is the shared per-pass context the spawner submodules
//! (`lsystem`, `prim`, `portal`, `material`) write into.

use avian3d::prelude::*;
use bevy::light::GlobalAmbientLight;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use std::collections::HashSet;

use crate::config::terrain as tcfg;
use crate::pds::{
    Fp3, Fp4, Generator, Placement, RoomRecord, ScatterBounds, SovereignTerrainConfig,
    TransformData,
};
use crate::state::CurrentRoomDid;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::WaterMaterial;

use crate::pds::ConstructNode;

use super::lsystem::{LSystemMaterialCache, LSystemMeshCache, spawn_lsystem_entity};
use super::material::{spawn_procedural_material, spawn_water_volume};
use super::portal::spawn_portal_entity;
use super::prim::{build_primitive_mesh, collider_for_primitive};
use super::{
    OverlandsFoliageTasks, PlacementMarker, PrimMarker, PropMeshAssets, RoomEntity, apply_traits,
    reset_traits,
};

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

        // The unified Anchor Entity
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

                for ix in 0..cx {
                    for iy in 0..cy {
                        for iz in 0..cz {
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
                            let child_tf = Transform::from_xyz(local_x, final_local_y, local_z)
                                .with_rotation(rotation);
                            if let Some(entity) =
                                dispatch_top_level(&mut ctx, generator_ref, child_tf)
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

                    if !keep {
                        continue;
                    }

                    // Make scatter children of the anchor so grabbing the Gizmo moves the whole forest live.
                    // Always draw from `rng` so disabling `random_yaw` doesn't shift downstream
                    // samples — the spawn stream stays byte-identical across peers regardless.
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
                    let child_tf = Transform::from_translation(local_pos).with_rotation(rotation);

                    if let Some(entity) = dispatch_top_level(&mut ctx, generator_ref, child_tf) {
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
pub(super) fn apply_environment_state(
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

pub(super) fn transform_from_data(t: &TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}

/// Uniform sample inside the scatter region. Circle bounds use rejection
/// sampling so the distribution stays flat instead of clumping at the
/// centre (which a naïve `radius * random()` would produce).
pub(super) fn sample_bounds(bounds: &ScatterBounds, rng: &mut ChaCha8Rng) -> (f32, f32) {
    match bounds {
        ScatterBounds::Rect {
            center,
            extents,
            rotation,
        } => {
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
pub(super) fn find_water_level_for_filter(record: &RoomRecord) -> Option<f32> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in &keys {
        if let Some(Generator::Water { level_offset }) = record.generators.get(*k) {
            let placement_y = record
                .placements
                .iter()
                .find_map(|p| match p {
                    Placement::Absolute {
                        generator_ref,
                        transform,
                        ..
                    } if generator_ref == *k => Some(transform.translation.0[1]),
                    _ => None,
                })
                .unwrap_or(0.0);
            let base_wl = tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE;
            let wl = (base_wl + level_offset.0).max(0.001);
            return Some(placement_y + wl);
        }
    }
    None
}

/// Deterministic `[-1, 1]` sample from a `ChaCha8Rng`.
pub(super) fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    let v = rng.next_u32() as f32 / u32::MAX as f32;
    v * 2.0 - 1.0
}

// ---------------------------------------------------------------------------
// Biome evaluation
// ---------------------------------------------------------------------------

/// Inline port of `SplatRule::weight` so we can evaluate a single
/// world-space point without running a full `SplatMapper::generate` pass
/// over the whole heightmap on every scatter attempt.
pub(super) fn rule_weight(r: &crate::pds::SovereignSplatRule, h: f32, slope: f32) -> f32 {
    let h_w = smooth_range(h, r.height_min.0, r.height_max.0, r.sharpness.0);
    let s_w = smooth_range(slope, r.slope_min.0, r.slope_max.0, r.sharpness.0);
    h_w * s_w
}

pub(super) fn smooth_range(value: f32, lo: f32, hi: f32, sharpness: f32) -> f32 {
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
pub(super) fn dominant_biome(cfg: &SovereignTerrainConfig, height_world: f32, slope: f32) -> u8 {
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
pub(super) struct SpawnCtx<'a, 'wc, 'sc, 'wq, 'sq> {
    pub(super) commands: &'a mut Commands<'wc, 'sc>,
    pub(super) record: &'a RoomRecord,
    pub(super) meshes: &'a mut Assets<Mesh>,
    pub(super) std_materials: &'a mut Assets<StandardMaterial>,
    pub(super) water_materials: &'a mut Assets<WaterMaterial>,
    pub(super) palette: Option<&'a MaterialPalette>,
    pub(super) heightmap: Option<&'a FinishedHeightMap>,
    pub(super) terrain_meshes:
        &'a Query<'wq, 'sq, Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    pub(super) prop_assets: Option<&'a PropMeshAssets>,
    pub(super) foliage_tasks: &'a mut OverlandsFoliageTasks,
    /// Persistent, hash-invalidated material cache. A single scatter
    /// placement with count=100 would otherwise allocate 100 fresh
    /// `StandardMaterial`s *and* enqueue 100 identical foliage texture
    /// tasks for the same slot — and across compile passes an unchanged
    /// slot would re-bake every time the record is patched. The cache
    /// keys on `(generator_ref, slot)` and reuses the handle whenever the
    /// content hash of `SovereignMaterialSettings` is identical.
    pub(super) lsystem_material_cache: &'a mut LSystemMaterialCache,
    /// `(generator_ref, slot)` keys touched this compile pass. Populated
    /// as we resolve material handles so the caller can GC stale entries.
    pub(super) lsystem_cache_touched: &'a mut HashSet<(String, u8)>,
    /// Persistent mesh cache. A single scatter placement with `count=100_000`
    /// would otherwise re-derive / re-interpret / re-mesh the L-system on
    /// every spawn, pegging the main thread for minutes and allocating
    /// 100_000 unique `Handle<Mesh>` entries. The cache keys on
    /// `generator_ref` and reuses the baked `Handle<Mesh>` bucket across
    /// every scatter point whenever the geometry fingerprint matches.
    pub(super) lsystem_mesh_cache: &'a mut LSystemMeshCache,
    /// `generator_ref` keys touched this compile pass so the caller can GC
    /// meshes belonging to generators removed from the record.
    pub(super) lsystem_mesh_touched: &'a mut HashSet<String>,
    /// DID of the room we're currently compiling. Portal generators skip the
    /// ATProto profile-picture fetch when `target_did` equals this (an
    /// intra-room portal has no remote identity to paint onto its top face).
    pub(super) current_room: Option<&'a CurrentRoomDid>,
}

/// Entry point called by the top-level `Placement` loop. Resolves the
/// generator by name and routes it into [`spawn_generator`] with an empty
/// blueprint path. The returned entity is the placement's root (caller
/// adopts it as a child of the placement anchor).
///
/// Traits are applied here rather than inside `spawn_generator` because
/// only a top-level placement is keyed directly by `generator_ref` in the
/// record's `traits` table — children inside a Construct share the
/// Construct's traits via the anchor and should not double-apply.
pub(super) fn dispatch_top_level(
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

    // Terrain is special: it's already spawned by the terrain plugin, so we
    // just apply the record's traits to the existing heightmap entities and
    // return — there is no per-placement entity to attach to the anchor.
    if matches!(generator, Generator::Terrain(_)) {
        for terrain_entity in ctx.terrain_meshes.iter() {
            reset_traits(ctx.commands, terrain_entity);
            apply_traits(ctx.commands, terrain_entity, ctx.record, generator_ref);
        }
        return None;
    }

    // `spawn_generator` walks the generator tree without borrowing
    // `ctx.record.generators` a second time. Pull the generator out by
    // cloning — it's only the placement root, so we pay the clone once per
    // placement (not per scatter sample, which keys off the already-cloned
    // root).
    //
    // Rather than cloning, we side-step the borrow by getting the slot
    // pointer; the record is not mutated by the spawner, only its
    // generators map is read, and we clone only if needed. Since we need a
    // live reference to `ctx` which has `&RoomRecord`, we use a small
    // helper that owns a clone.
    let generator = generator.clone();
    let entity = spawn_generator(ctx, &generator, generator_ref, &[], transform);
    if let Some(entity) = entity {
        apply_traits(ctx.commands, entity, ctx.record, generator_ref);
    }
    entity
}

/// Unified recursive spawner. Builds the entity tree for `generator`,
/// parented under a `base_ref`-qualified synthetic path so nested L-system
/// and procedural-texture caches stay collision-free across fractal
/// Construct nestings.
///
/// * `base_ref` is the top-level generator's key in `RoomRecord::generators`.
/// * `path` records the child-index chain from the base Construct's root
///   down to this node. It is `&[]` for a top-level placement or the root
///   of a Construct blueprint, and grows by one index at each recursion
///   into `ConstructNode::children`.
///
/// The returned entity is the node's visible/physical root. Trait
/// application is the caller's responsibility — this function deliberately
/// does not apply traits so recursion into a Construct's children doesn't
/// double-attach `Sensor` or `collider_heightfield` components.
pub(super) fn spawn_generator(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator: &Generator,
    base_ref: &str,
    path: &[usize],
    transform: Transform,
) -> Option<Entity> {
    let cache_key = synthetic_cache_key(base_ref, path);
    let in_blueprint = !path.is_empty();

    let entity = match generator {
        // Terrain/Water are room-scoped and only valid at top level. The
        // sanitizer overwrites Terrain/Water found inside a ConstructNode,
        // but we defend against a stale in-memory record by skipping here
        // too so a nested one never spawns a second heightmap collider.
        Generator::Terrain(_) => {
            if in_blueprint {
                warn!("Terrain generator ignored inside Construct blueprint at `{cache_key}`");
                return None;
            }
            // Top-level Terrain was handled in `dispatch_top_level`; reaching
            // this branch here would mean direct recursion into a Terrain
            // value we didn't clone from the record, which shouldn't happen.
            None
        }
        Generator::Water { level_offset } => {
            if in_blueprint {
                warn!("Water generator ignored inside Construct blueprint at `{cache_key}`");
                return None;
            }
            let world_extent = ctx
                .heightmap
                .map(|hm| (hm.0.width() - 1) as f32 * hm.0.scale())
                .unwrap_or_else(|| (tcfg::GRID_SIZE - 1) as f32 * tcfg::CELL_SCALE);
            Some(spawn_water_volume(
                ctx.commands,
                level_offset.0,
                transform,
                world_extent,
                ctx.meshes,
                ctx.water_materials,
            ))
        }
        Generator::LSystem { .. } => {
            // Synthetic cache key keeps a nested L-system distinct from any
            // siblings (and from its outer Construct) so `LSystemMeshCache`
            // entries don't clobber each other. Scattering 1000 Constructs
            // each containing the same L-system at path=[0] reuses the same
            // "<base_ref>/0" cache entry — 1 derivation, 999 handle clones.
            spawn_lsystem_entity(ctx, generator, &cache_key, transform)
        }
        Generator::Portal {
            target_did,
            target_pos,
        } => Some(spawn_portal_entity(ctx, target_did, target_pos, transform)),
        Generator::Cuboid { .. }
        | Generator::Sphere { .. }
        | Generator::Cylinder { .. }
        | Generator::Capsule { .. }
        | Generator::Cone { .. }
        | Generator::Torus { .. }
        | Generator::Plane { .. }
        | Generator::Tetrahedron { .. } => Some(spawn_primitive_entity(ctx, generator, transform)),
        Generator::Construct { root } => {
            Some(spawn_construct_tree(ctx, root, base_ref, path, transform))
        }
        Generator::Unknown => {
            warn!("Ignoring generator `{cache_key}` of unknown $type");
            None
        }
    };

    // Attach a PrimMarker to blueprint-internal entities so the editor
    // gizmo can map a UI-selected ConstructNode back to its live Bevy
    // entity. Top-level placements get PlacementMarker from the caller
    // instead — a PrimMarker on them would confuse the gizmo's prim
    // detach/reparent path.
    if in_blueprint && let Some(e) = entity {
        ctx.commands.entity(e).insert(PrimMarker {
            generator_ref: base_ref.to_string(),
            path: path.to_vec(),
        });
    }

    entity
}

fn synthetic_cache_key(base_ref: &str, path: &[usize]) -> String {
    if path.is_empty() {
        base_ref.to_string()
    } else {
        let suffix = path
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("/");
        format!("{base_ref}/{suffix}")
    }
}

/// Spawn a parametric primitive entity: build its mesh (with vertex torture
/// when configured), pair it with a PBR material handle, and attach the
/// matching collider if the node is solid. Always carries `RoomEntity` so
/// the compile-pass cleanup sweeps it even when detached from the anchor
/// hierarchy by the gizmo.
fn spawn_primitive_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator: &Generator,
    transform: Transform,
) -> Entity {
    let (solid, material_settings) = match generator {
        Generator::Cuboid {
            solid, material, ..
        }
        | Generator::Sphere {
            solid, material, ..
        }
        | Generator::Cylinder {
            solid, material, ..
        }
        | Generator::Capsule {
            solid, material, ..
        }
        | Generator::Cone {
            solid, material, ..
        }
        | Generator::Torus {
            solid, material, ..
        }
        | Generator::Plane {
            solid, material, ..
        }
        | Generator::Tetrahedron {
            solid, material, ..
        } => (*solid, material.clone()),
        _ => unreachable!("spawn_primitive_entity called on non-primitive generator"),
    };

    let raw_mesh = build_primitive_mesh(generator);
    let collider = if solid {
        collider_for_primitive(generator, &raw_mesh)
    } else {
        None
    };
    let mesh_handle = ctx.meshes.add(raw_mesh);
    let material_handle = spawn_procedural_material(ctx, &material_settings);

    let mut cmd = ctx.commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        transform,
        RoomEntity,
    ));
    if let Some(collider) = collider {
        cmd.insert(collider);
    }
    cmd.id()
}

/// Spawn a Construct blueprint: an intermediate anchor at the node's
/// `transform`, then the root generator as its child, then each of the
/// root's children recursively. Preserves the double-anchor pattern the
/// legacy `spawn_construct_entity` used so a placement-level rotation still
/// rotates the whole blueprint as a rigid group even when the root is
/// itself a primitive.
fn spawn_construct_tree(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    root: &ConstructNode,
    base_ref: &str,
    base_path: &[usize],
    placement_tf: Transform,
) -> Entity {
    let inner_anchor = ctx
        .commands
        .spawn((
            placement_tf,
            Visibility::default(),
            RigidBody::Static,
            RoomEntity,
        ))
        .id();

    let root_tf = transform_from_data(&root.transform);
    let root_entity = spawn_generator(ctx, &root.generator, base_ref, base_path, root_tf);
    if let Some(root_entity) = root_entity {
        ctx.commands.entity(inner_anchor).add_child(root_entity);
        spawn_construct_children(ctx, root, root_entity, base_ref, base_path);
    }

    inner_anchor
}

/// Recursive walk of a ConstructNode's children. Each child is spawned as
/// a direct child of `parent_entity` (which is the generated entity for the
/// parent node, not its anchor), with its path extended by the child index.
fn spawn_construct_children(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    parent_node: &ConstructNode,
    parent_entity: Entity,
    base_ref: &str,
    parent_path: &[usize],
) {
    for (i, child) in parent_node.children.iter().enumerate() {
        let mut child_path = parent_path.to_vec();
        child_path.push(i);
        let child_tf = transform_from_data(&child.transform);
        if let Some(child_entity) =
            spawn_generator(ctx, &child.generator, base_ref, &child_path, child_tf)
        {
            ctx.commands.entity(parent_entity).add_child(child_entity);
            spawn_construct_children(ctx, child, child_entity, base_ref, &child_path);
        }
    }
}
