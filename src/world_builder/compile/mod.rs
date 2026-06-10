//! Room-record → ECS compile engine: incremental (per-placement diff)
//! and time-sliced (per-frame budget).
//!
//! [`compile_room_record`] runs two phases:
//!
//! 1. **Plan** (on every record / heightmap change): fingerprint each
//!    placement ([`job::unit_fingerprint`]) and diff against
//!    [`job::CompiledWorld`]. Stale units are despawned immediately
//!    (anchor-recursive, plus their water planes); changed indices are
//!    queued ascending. Heightmap swaps and placement-count changes
//!    force a full rebuild (a flat `RoomEntity` sweep that also catches
//!    strays such as gizmo-detached prims), because snapped transforms
//!    resp. `PlacementMarker` indices would otherwise go stale.
//! 2. **Execute** (every frame while a job is active): build queued
//!    units inside a ~5 ms wall-clock slice ([`job::SLICE_BUDGET`]),
//!    resuming mid-grid / mid-scatter via [`job::UnitCursor`] (which
//!    carries the RNG, so a sliced build is byte-identical to a
//!    monolithic one). On completion: cache GC (full-coverage jobs
//!    only), the [`WorldCompiled`](super::WorldCompiled) gate marker,
//!    and one telemetry line into the diagnostics log.
//!
//! Both halves exist for the wasm build, where every millisecond of
//! compile runs on the main thread: the diff makes editor tweaks pay
//! for only what they touched, and the slice keeps even a full build
//! from freezing input and audio.
//!
//! ## Sub-module map
//!
//! * [`job`] — [`CompiledWorld`] / [`CompileJob`] state, the unit
//!   fingerprint, the slice budget, and the resume cursors.
//! * [`spawn_ctx`] — [`SpawnCtx`] (the write-context shared with every
//!   sibling spawner module), [`GeneratorCaches`] system param,
//!   [`MAX_ROOM_ENTITIES`](spawn_ctx::MAX_ROOM_ENTITIES) cap +
//!   [`budget_exceeded`] gate, and [`transform_from_data`].
//! * [`environment`] — [`apply_environment_state`] (its own system).
//! * [`scatter`] — sampling helpers ([`sample_bounds`], [`unit_f32`])
//!   and the biome-rule evaluator ([`dominant_biome`]).
//! * [`dispatch`] — recursive [`spawn_generator`] +
//!   [`dispatch_top_level`] walker into the per-generator spawners.
//! * [`contact_recipes`] — [`apply_contact_recipes`] system.

mod contact_recipes;
mod dispatch;
mod environment;
pub(super) mod job;
mod scatter;
mod spawn_ctx;

// External callers (`super::compile::SpawnCtx` etc.) reach these names
// through this re-export. Behavioural surface is identical to the
// pre-refactor flat `compile.rs`.
pub(super) use contact_recipes::apply_contact_recipes;
pub(super) use dispatch::dispatch_top_level;
pub use dispatch::spawn_generator;
pub(super) use environment::apply_environment_state;
pub use job::{CompileJob, CompiledWorld};
pub(super) use spawn_ctx::transform_from_data;
pub use spawn_ctx::{GeneratorCaches, SpawnCtx, budget_exceeded};

use avian3d::prelude::*;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use std::collections::VecDeque;

use crate::pds::{GeneratorKind, Placement, RoomRecord, ScatterBounds};
use crate::state::{CurrentRoomDid, LiveRoomRecord};
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterPlane, WaterSurfaces};

use super::image_cache::BlobImageCache;
use super::{PlacementMarker, PlacementUnit, PropMeshAssets, RoomEntity};

use job::{
    ActiveJob, CompiledUnit, CursorKind, QueuedUnit, StepOutcome, UnitCursor, unit_fingerprint,
};
use scatter::{dominant_biome, sample_bounds, unit_f32};

#[allow(clippy::too_many_arguments)]
pub(super) fn compile_room_record(
    mut commands: Commands,
    record: Option<Res<LiveRoomRecord>>,
    existing: Query<(Entity, Option<&PlacementUnit>), With<RoomEntity>>,
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
    let record_changed = record.is_changed();
    if !record_changed && !heightmap_changed && generator_caches.job.0.is_none() {
        return;
    }
    // The change tick above is read off the `Res<LiveRoomRecord>`
    // wrapper; everything below wants the inner `RoomRecord`.
    let record = &record.0;
    let room_water_y = room_water_level(record);

    // ---- Phase 1: plan -------------------------------------------------
    if record_changed || heightmap_changed {
        plan_job(
            &mut commands,
            &existing,
            record,
            heightmap_changed,
            &mut generator_caches.world,
            &mut generator_caches.job,
            &mut water_surfaces,
        );
        if generator_caches.job.0.is_none() {
            // Nothing to (re)build — an environment / effects / metadata
            // edit. The world for this record already exists, so the
            // loading gate may release.
            commands.insert_resource(super::WorldCompiled);
            return;
        }
    }

    // ---- Phase 2: execute one slice -------------------------------------
    // The job is moved out of its resource slot for the duration of the
    // slice so `SpawnCtx` can borrow its touch-sets / budget counters
    // while the loop still mutates its queue and cursor.
    let Some(mut job) = generator_caches.job.0.take() else {
        return;
    };
    let slice_start = Instant::now();
    let deadline = slice_start + job::SLICE_BUDGET;
    let mut finished = false;

    {
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
            lsystem_cache_touched: &mut job.touched.lsystem_material,
            lsystem_mesh_cache: &mut generator_caches.lsystem_mesh,
            lsystem_mesh_touched: &mut job.touched.lsystem_mesh,
            shape_material_cache: &mut generator_caches.shape_material,
            shape_material_touched: &mut job.touched.shape_material,
            shape_mesh_cache: &mut generator_caches.shape_mesh,
            upstream_shape_mesh_cache: &mut generator_caches.upstream_shape_mesh,
            shape_mesh_touched: &mut job.touched.shape_mesh,
            current_room: current_room.as_deref(),
            entities_spawned: &mut job.entities_spawned,
            budget_warned: &mut job.budget_warned,
            blob_image_cache: &mut blob_image_cache,
            blob_audio_cache: &mut blob_audio_cache,
            baked_audio_cache: &mut generator_caches.baked_audio,
            water_surfaces: &mut water_surfaces,
            placement_index: WaterPlane::NO_OWNER,
            avatar_mode: false,
            local_avatar_mode: false,
        };

        loop {
            // The multiplicative entity cap stops the whole job, exactly
            // like the monolithic pass stopped its placement walk: the
            // in-flight unit is committed as-is and the rest is skipped
            // (their fingerprints stay unset, so a later edit retries).
            if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
                if let Some(cursor) = job.cursor.take() {
                    generator_caches.world.units[cursor.index] = CompiledUnit {
                        fingerprint: cursor.fingerprint,
                        anchor: Some(cursor.anchor),
                    };
                    job.units_built += 1;
                }
                job.queue.clear();
            }
            if Instant::now() >= deadline {
                break;
            }

            if let Some(cursor) = job.cursor.as_mut() {
                ctx.placement_index = cursor.index;
                match step_unit(&mut ctx, cursor, deadline) {
                    StepOutcome::Yielded => break,
                    StepOutcome::Done => {
                        let cursor = job.cursor.take().expect("cursor checked above");
                        generator_caches.world.units[cursor.index] = CompiledUnit {
                            fingerprint: cursor.fingerprint,
                            anchor: Some(cursor.anchor),
                        };
                        job.units_built += 1;
                    }
                }
            } else if let Some(queued) = job.queue.pop_front() {
                ctx.placement_index = queued.index;
                match start_unit(&mut ctx, queued, room_water_y) {
                    UnitStart::Committed(index, unit) => {
                        generator_caches.world.units[index] = unit;
                        job.units_built += 1;
                    }
                    UnitStart::InProgress(cursor) => {
                        job.cursor = Some(cursor);
                    }
                }
            } else {
                finished = true;
                break;
            }
        }
    }

    job.work += slice_start.elapsed();
    job.frames += 1;

    if !finished {
        generator_caches.job.0 = Some(job);
        return;
    }

    // ---- Job completion --------------------------------------------------
    // Cache GC is only sound when the job touched every placement: an
    // incremental job's touch-sets cover just the rebuilt units, and
    // evicting everything else would orphan the untouched world's
    // mesh / material handles. Stale entries from a removed generator
    // persist until the next full rebuild instead.
    if job.full {
        generator_caches
            .lsystem_material
            .entries
            .retain(|k, _| job.touched.lsystem_material.contains(k));
        generator_caches
            .lsystem_mesh
            .entries
            .retain(|k, _| job.touched.lsystem_mesh.contains(k));
        generator_caches
            .shape_material
            .entries
            .retain(|k, _| job.touched.shape_material.contains(k));
        generator_caches
            .shape_mesh
            .entries
            .retain(|k, _| job.touched.shape_mesh.contains(k));
    }

    let line = format!(
        "World compile: {} unit(s), {} entities, {:.1} ms over {} frame(s){}",
        job.units_built,
        job.entities_spawned,
        job.work.as_secs_f64() * 1000.0,
        job.frames,
        if job.full { " (full)" } else { "" },
    );
    info!("{line}");
    let now = generator_caches.time.elapsed_secs_f64();
    generator_caches.diagnostics.push(now, line);

    // Unblock the loading gate: the world this record describes exists.
    // Idempotent on later jobs; removed by `logout::cleanup_on_logout`.
    commands.insert_resource(super::WorldCompiled);
}

/// Diff the record against [`CompiledWorld`] and (re)build the job
/// queue. See the module docs for the full / incremental split.
fn plan_job(
    commands: &mut Commands,
    existing: &Query<(Entity, Option<&PlacementUnit>), With<RoomEntity>>,
    record: &RoomRecord,
    heightmap_changed: bool,
    world: &mut CompiledWorld,
    job: &mut CompileJob,
    water_surfaces: &mut WaterSurfaces,
) {
    // Indices whose spawned entities must be retired this plan. Filled
    // by the cursor abort + the diff below, then swept in one flat pass
    // over the `PlacementUnit` markers — anchor-recursive despawn alone
    // is NOT enough, because the gizmo detaches dragged prims from
    // their anchor hierarchy and the detachment outlives the drag
    // (pre-marker, rebuilding a gizmo-edited placement duplicated the
    // dragged subtree; a second water plane was the visible case).
    let mut retired: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Abort any mid-build unit first: its fingerprint was never
    // committed, so the diff below naturally re-queues it against the
    // *current* record.
    if let Some(active) = job.0.as_mut()
        && let Some(cursor) = active.cursor.take()
    {
        commands.entity(cursor.anchor).try_despawn();
        water_surfaces.planes.retain(|p| p.owner != cursor.index);
        retired.insert(cursor.index);
    }

    let len = record.placements.len();
    // Full when the heightmap was swapped (every snapped transform
    // sampled the old surface) or the placement count changed (indices
    // are unit identity; `PlacementMarker` values on surviving anchors
    // would go stale under an insert/remove shift). The first compile
    // is the count-change case with a previous length of zero.
    let full = heightmap_changed || world.units.len() != len;
    let mut queue: VecDeque<QueuedUnit> = VecDeque::new();

    if full {
        // Flat sweep of everything (marker-blind): also catches spawns
        // that never carried a unit marker, e.g. world-space particles
        // and one-shot audio voices.
        for (e, _) in existing.iter() {
            commands.entity(e).try_despawn();
        }
        water_surfaces.planes.clear();
        world.units = (0..len).map(|_| CompiledUnit::default()).collect();
        for (index, placement) in record.placements.iter().enumerate() {
            queue.push_back(QueuedUnit {
                index,
                fingerprint: unit_fingerprint(record, placement),
            });
        }
    } else {
        for (index, placement) in record.placements.iter().enumerate() {
            let fingerprint = unit_fingerprint(record, placement);
            if fingerprint.is_some() && world.units[index].fingerprint == fingerprint {
                continue;
            }
            // Stale unit: retire its spawned tree and its water planes
            // now, so a later unit in this same job (e.g. a scatter
            // sampling the water registry) never sees the old state.
            // The anchor-recursive despawn handles the (common) intact
            // hierarchy a frame earlier than the flat sweep can see
            // newly-spawned children; the marker sweep below catches
            // anything reparented out of it.
            if let Some(anchor) = world.units[index].anchor.take() {
                commands.entity(anchor).try_despawn();
            }
            water_surfaces.planes.retain(|p| p.owner != index);
            world.units[index].fingerprint = None;
            retired.insert(index);
            queue.push_back(QueuedUnit { index, fingerprint });
        }

        // One flat ownership sweep for every retired unit. `try_despawn`
        // tolerates the overlap with the recursive anchor despawns
        // above (and with double-marked descendants).
        if !retired.is_empty() {
            for (e, unit) in existing.iter() {
                if unit.is_some_and(|u| retired.contains(&u.0)) {
                    commands.entity(e).try_despawn();
                }
            }
        }
    }

    match job.0.as_mut() {
        // Replan of an in-flight job: the fresh diff already covers
        // everything the old queue still owed (uncommitted units have a
        // `None` fingerprint and always mismatch), so the queue is
        // replaced outright. Telemetry / touch-sets / spawn budget
        // accumulate across the replan, and `full` is sticky so the
        // end-of-job GC keeps full coverage.
        Some(active) => {
            active.queue = queue;
            active.full |= full;
        }
        None if queue.is_empty() => {}
        None => job.0 = Some(ActiveJob::new(queue, full)),
    }
}

/// Outcome of [`start_unit`]: simple units commit immediately, grid /
/// scatter units hand back a cursor for the slice loop to drive.
/// The cursor is boxed-by-variant-size standards large (it carries a
/// ChaCha RNG state), but the enum lives only for the duration of one
/// `start_unit` return — no arrays of it ever exist — so the size skew
/// clippy flags has no carrier to matter on.
#[allow(clippy::large_enum_variant)]
enum UnitStart {
    Committed(usize, CompiledUnit),
    InProgress(UnitCursor),
}

/// Begin one queued unit: resolve the anchor transform (snap /
/// dry-land walk), spawn the anchor, and either finish it on the spot
/// (`Absolute` / `Unknown`) or return the resume cursor for its cell
/// loop.
fn start_unit(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    queued: QueuedUnit,
    room_water_y: Option<f32>,
) -> UnitStart {
    let index = queued.index;
    // Same reference-copy trick as `step_unit`: the placement borrows
    // the record, not `ctx`.
    let record = ctx.record;
    let placement = &record.placements[index];

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
                ScatterBounds::Circle { center, .. } => Vec3::new(center.0[0], 0.0, center.0[1]),
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
        Placement::Unknown => {
            // Nothing to spawn; commit so the planner doesn't requeue.
            return UnitStart::Committed(
                index,
                CompiledUnit {
                    fingerprint: queued.fingerprint,
                    anchor: None,
                },
            );
        }
    };

    // Resolve Anchor world Y if snapped.
    let mut anchor_world_tf = anchor_tf;
    if snap {
        if let Some(hm_res) = ctx.heightmap {
            let hm = &hm_res.0;
            let extent = (hm.width() - 1) as f32 * hm.scale();
            let half = extent * 0.5;
            // Water-avoiding placements slide to dry land before the
            // height sample (may move X/Z, preserves bearing).
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
            // Absolute placements keep their authored Y as an offset
            // from the snapped terrain height (the seeded landmark
            // sinks its foundations 0.35 m); Scatter / Grid anchors
            // keep the historical replace semantics.
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
            PlacementMarker(index),
            RoomEntity,
            PlacementUnit(index),
        ))
        .id();

    match placement {
        Placement::Absolute { generator_ref, .. } => {
            // One dispatch — atomic; a single blueprint stays the
            // smallest unit of work the slicer can schedule.
            if let Some(entity) = dispatch_top_level(ctx, generator_ref, Transform::IDENTITY) {
                ctx.commands.entity(anchor).add_child(entity);
            }
            UnitStart::Committed(
                index,
                CompiledUnit {
                    fingerprint: queued.fingerprint,
                    anchor: Some(anchor),
                },
            )
        }
        Placement::Grid { random_yaw, .. } => UnitStart::InProgress(UnitCursor {
            index,
            fingerprint: queued.fingerprint,
            anchor,
            anchor_world_tf,
            snap,
            kind: CursorKind::Grid {
                next_cell: 0,
                // Per-placement RNG so yaw stays deterministic across
                // peers without a user-facing seed field on Grid.
                rng: random_yaw.then(|| ChaCha8Rng::seed_from_u64(index as u64)),
            },
        }),
        Placement::Scatter {
            bounds, local_seed, ..
        } => {
            // Resolve the biome-filter water threshold from the runtime
            // registry. One global Y per scatter, sampled at its centre
            // at unit start — placements that come before the
            // home-water spawn collapse to "no water" and the filter
            // accepts by default, exactly as in the monolithic pass.
            let scatter_center_xz = match bounds {
                ScatterBounds::Circle { center, .. } => Vec2::new(center.0[0], center.0[1]),
                ScatterBounds::Rect { center, .. } => Vec2::new(center.0[0], center.0[1]),
            };
            let water_level = ctx
                .water_surfaces
                .surface_at(scatter_center_xz)
                .map(|(_, y)| y);
            UnitStart::InProgress(UnitCursor {
                index,
                fingerprint: queued.fingerprint,
                anchor,
                anchor_world_tf,
                snap,
                kind: CursorKind::Scatter {
                    spawned: 0,
                    attempts: 0,
                    rng: ChaCha8Rng::seed_from_u64(*local_seed),
                    water_level,
                },
            })
        }
        // `Unknown` returned `Committed` before the anchor spawn; the
        // other variants are covered by the arms above.
        Placement::Unknown => unreachable!("Unknown placements commit before the anchor spawn"),
    }
}

/// Drive the current unit's cell loop until it finishes or the slice
/// deadline passes. Cell-for-cell identical to the monolithic pass —
/// the cursor carries the RNG so resuming doesn't shift the stream.
fn step_unit(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    cursor: &mut UnitCursor,
    deadline: Instant,
) -> StepOutcome {
    // `ctx.record` is a shared reference field — copying it out gives a
    // borrow of the record itself, not of `ctx`, so the placement can
    // stay live across the `&mut ctx` dispatch calls below.
    let record = ctx.record;
    let placement = &record.placements[cursor.index];
    match (placement, &mut cursor.kind) {
        (
            Placement::Grid {
                generator_ref,
                counts,
                gaps,
                ..
            },
            CursorKind::Grid { next_cell, rng },
        ) => {
            let [cx, cy, cz] = *counts;
            let total = cx as u64 * cy as u64 * cz as u64;
            let [gx, gy, gz] = gaps.0;
            let start_x = -((cx as f32 - 1.0) * gx) / 2.0;
            let start_y = -((cy as f32 - 1.0) * gy) / 2.0;
            let start_z = -((cz as f32 - 1.0) * gz) / 2.0;

            while *next_cell < total {
                if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
                    return StepOutcome::Done;
                }
                if Instant::now() >= deadline {
                    return StepOutcome::Yielded;
                }
                // Linear → (ix, iy, iz) in the monolithic loop's order.
                let cell = *next_cell;
                let ix = (cell / (cy as u64 * cz as u64)) as u32;
                let iy = ((cell / cz as u64) % cy as u64) as u32;
                let iz = (cell % cz as u64) as u32;
                *next_cell += 1;

                let local_x = start_x + (ix as f32) * gx;
                let local_y = start_y + (iy as f32) * gy;
                let local_z = start_z + (iz as f32) * gz;

                let mut final_local_y = local_y;
                if cursor.snap {
                    let world_pos = cursor
                        .anchor_world_tf
                        .transform_point(Vec3::new(local_x, 0.0, local_z));
                    let world_y = if let Some(hm_res) = ctx.heightmap {
                        let hm = &hm_res.0;
                        let extent = (hm.width() - 1) as f32 * hm.scale();
                        let half = extent * 0.5;
                        let hm_x = (world_pos.x + half).clamp(0.0, extent);
                        let hm_z = (world_pos.z + half).clamp(0.0, extent);
                        hm.get_height_at(hm_x, hm_z)
                    } else {
                        0.0
                    };
                    let local_snapped = cursor
                        .anchor_world_tf
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
                // Per-cell placement transform composes on top of the
                // generator's own root transform inside
                // `dispatch_top_level`. Yaw spins each cell around its
                // own Y axis so identical blueprints don't all face the
                // same way.
                let cell_tf =
                    Transform::from_xyz(local_x, final_local_y, local_z).with_rotation(rotation);
                if let Some(entity) = dispatch_top_level(ctx, generator_ref, cell_tf) {
                    ctx.commands.entity(cursor.anchor).add_child(entity);
                }
            }
            StepOutcome::Done
        }
        (
            Placement::Scatter {
                generator_ref,
                bounds,
                count,
                biome_filter,
                random_yaw,
                ..
            },
            CursorKind::Scatter {
                spawned,
                attempts,
                rng,
                water_level,
            },
        ) => {
            let terrain_cfg = crate::pds::find_terrain_config(ctx.record);
            let max_attempts = count.saturating_mul(10).max(*count);

            while *spawned < *count && *attempts < max_attempts {
                if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
                    return StepOutcome::Done;
                }
                if Instant::now() >= deadline {
                    return StepOutcome::Yielded;
                }
                *attempts += 1;
                let (world_x, world_z) = sample_bounds(bounds, rng);

                let (world_y, keep) = if let Some(hm_res) = ctx.heightmap {
                    let hm = &hm_res.0;
                    let extent = (hm.width() - 1) as f32 * hm.scale();
                    let half = extent * 0.5;
                    let hm_x = (world_x + half).clamp(0.0, extent);
                    let hm_z = (world_z + half).clamp(0.0, extent);
                    let y = hm.get_height_at(hm_x, hm_z);
                    let keep = if biome_filter.is_noop() {
                        true
                    } else {
                        // Without a terrain generator the biome
                        // allow-list has no channel to resolve against;
                        // treat any non-empty list as "never matches" so
                        // accidental biome filters on dry-land records
                        // don't silently pass through. The water clause
                        // still evaluates.
                        let biome = if let Some(tcfg) = terrain_cfg {
                            let normal = hm.get_normal_at(hm_x, hm_z);
                            let slope = (1.0 - normal[1]).max(0.0);
                            dominant_biome(tcfg, y, slope)
                        } else {
                            255
                        };
                        biome_filter.accepts(biome, y, *water_level)
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
                let local_pos = cursor
                    .anchor_world_tf
                    .compute_affine()
                    .inverse()
                    .transform_point3(Vec3::new(world_x, world_y, world_z));
                let yaw_sample = unit_f32(rng) * std::f32::consts::PI;
                let rotation = if *random_yaw {
                    Quat::from_rotation_y(yaw_sample)
                } else {
                    Quat::IDENTITY
                };
                let cell_tf = Transform::from_translation(local_pos).with_rotation(rotation);

                if let Some(entity) = dispatch_top_level(ctx, generator_ref, cell_tf) {
                    ctx.commands.entity(cursor.anchor).add_child(entity);
                }
                *spawned += 1;
            }

            if *spawned < *count {
                debug!(
                    "Scatter `{}` placed {}/{} points",
                    generator_ref, spawned, count
                );
            }
            StepOutcome::Done
        }
        // A cursor only exists for Grid / Scatter, and a record change
        // replans (aborting the cursor) before the placement kind could
        // differ — but stay total rather than panicking the frame loop.
        _ => StepOutcome::Done,
    }
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
