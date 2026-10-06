//! The compile executor: the [`compile_room_record`] system and its
//! plan/execute internals.
//!
//! [`compile_room_record`] runs two phases:
//!
//! 1. **Plan** (on every record / heightmap change): fingerprint each
//!    placement ([`super::job::unit_fingerprint`]) and diff against
//!    [`super::job::CompiledWorld`]. Stale units are despawned immediately
//!    (anchor-recursive, plus their water planes); changed indices are
//!    queued ascending. Heightmap swaps, placement-count *shrinks*, and
//!    the first compile force a full rebuild (a flat `RoomEntity` sweep
//!    that also catches strays such as gizmo-detached prims), because
//!    snapped transforms resp. `PlacementMarker` indices would otherwise
//!    go stale. Count *growth* is an append (#979) and stays
//!    incremental: only the new tail builds, so dropping an item into a
//!    room never blinks the rest of the room out.
//! 2. **Execute** (every frame while a job is active): build queued
//!    units inside a ~5 ms wall-clock slice ([`super::job::SLICE_BUDGET`]),
//!    resuming mid-grid / mid-scatter via [`super::job::UnitCursor`] (which
//!    carries the RNG, so a sliced build is byte-identical to a
//!    monolithic one). On completion: cache GC (full-coverage jobs
//!    only), the [`WorldCompiled`](super::super::WorldCompiled) gate marker,
//!    and one telemetry line into the diagnostics log.
//!
//! Both halves exist for the wasm build, where every millisecond of
//! compile runs on the main thread: the diff makes editor tweaks pay
//! for only what they touched, and the slice keeps even a full build
//! from freezing input and audio.

use avian3d::prelude::*;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use std::collections::VecDeque;

use crate::pds::{Placement, RoomRecord, ScatterBounds};
use crate::state::{CurrentRoomDid, LiveRoomRecord};
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterPlane, WaterSurfaces};

use super::super::image_cache::BlobImageCache;
use super::super::shape::ShapeMeshCache;
use super::super::{PlacementMarker, PlacementUnit, RoomEntity};

use super::dispatch::dispatch_top_level;
use super::job::{
    self, ActiveJob, CompileJob, CompiledUnit, CompiledWorld, CursorKind, FingerprintPass,
    QueuedUnit, StepOutcome, UnitCursor, unit_fingerprint,
};
use super::scatter::unit_f32;
use super::spawn_ctx::{GeneratorCaches, SpawnCtx, budget_exceeded, transform_from_data};
use super::water::room_water_level;

#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_room_record(
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
    mut generator_caches: GeneratorCaches,
    current_room: Option<Res<CurrentRoomDid>>,
    mut blob_image_cache: ResMut<BlobImageCache>,
    mut blob_audio_cache: ResMut<super::super::audio_resolver::BlobAudioCache>,
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
            &mut generator_caches.shape_mesh,
        );
        if generator_caches.job.0.is_none() {
            // Nothing to (re)build - an environment / effects / metadata
            // edit. The world for this record already exists, so the
            // loading gate may release.
            commands.insert_resource(super::super::WorldCompiled);
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
    // Cached at plan time (#673) so the per-frame slices don't re-scan
    // every generator; a record change replans before it could go stale.
    let room_water_y = job.room_water_y;

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
            lsystem_material_cache: &mut generator_caches.lsystem_material,
            lsystem_cache_touched: &mut job.touched.lsystem_material,
            lsystem_mesh_cache: &mut generator_caches.lsystem_mesh,
            lsystem_mesh_touched: &mut job.touched.lsystem_mesh,
            shape_material_cache: &mut generator_caches.shape_material,
            shape_material_touched: &mut job.touched.shape_material,
            shape_mesh_cache: &mut generator_caches.shape_mesh,
            prim_mesh_cache: &mut generator_caches.prim_mesh,
            prim_mesh_touched: &mut job.touched.prim_mesh,
            prim_material_cache: &mut generator_caches.prim_material,
            prim_material_touched: &mut job.touched.prim_material,
            upstream_shape_mesh_cache: &mut generator_caches.upstream_shape_mesh,
            shape_mesh_touched: &mut job.touched.shape_mesh,
            texture_cache: &mut generator_caches.texture,
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
            attachment_rkey: None,
            copy: &mut job.copy,
            draw_cuts: generator_caches
                .draw_cuts
                .as_deref()
                .copied()
                .unwrap_or_default(),
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
                // What the stop cost, for the user-facing report (#1211).
                if job.skipped_from.is_none() {
                    job.skipped_from = job.queue.front().map(|unit| unit.index);
                }
                job.skipped_units += job.queue.len() as u32;
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
        // Its builds and its remembered failures alike (#1505).
        generator_caches.shape_mesh.retain(&job.touched.shape_mesh);
        // The content-addressed primitive caches (#918). Without this they
        // survived every rebuild and were bounded only by a 4096-entry
        // wholesale clear or logout, so each region re-roll permanently
        // added that region's prim meshes, materials, and - through the
        // materials - their procedural images: ~90 image and ~100 mesh
        // handles per re-roll, ~70 MB of RSS, never released (#919).
        //
        // Evicting is safe for the same reason it is for the caches above:
        // a cache entry is only a *second* owner of the handle, and every
        // live instance holds its own. What is evicted here is re-baked on
        // the next miss. That includes avatar-spawned prims, which share
        // these caches but populate their touch-sets outside the job - a
        // full room rebuild costs them one re-bake, and full rebuilds are
        // already when avatars are being rebuilt anyway.
        super::super::prim_cache::retain_touched(
            &mut generator_caches.prim_mesh,
            &job.touched.prim_mesh,
        );
        super::super::prim_cache::retain_touched(
            &mut generator_caches.prim_material,
            &job.touched.prim_material,
        );
        // The upstream `ShapeMeshCache` is keyed by float-exact terminal
        // footprint, has no eviction, and (unlike the caches above) exposes no
        // per-key retain - so a slider drag mints a fresh `Handle<Mesh>` per
        // distinct footprint that is otherwise pinned for the whole session,
        // an unbounded leak. It is only a derivation-time dedup accelerator:
        // every mesh it holds is also kept alive by the spawned entities and
        // the local `shape_mesh` cache's instances, so clearing it on each full
        // rebuild frees the orphaned footprints without touching live geometry
        // (a re-edited generator simply re-bakes its terminals on the next miss).
        generator_caches.upstream_shape_mesh.clear();

        // Anchor for the asset-growth watch (#921): the 1 Hz diagnostics
        // scraper snapshots handle counts into the per-rebuild mark gauges
        // whenever this counter advances. Counted here rather than sampled
        // there so "a full rebuild happened" has exactly one definition -
        // a job whose touch-sets covered every placement, i.e. the ones
        // after which everything unreferenced should have been released.
        // (`None` in headless embedders without the diagnostics plugin.)
        if let Some(metrics) = generator_caches.metrics.as_deref_mut() {
            metrics.incr(crate::diagnostics::names::RUNTIME_FULL_REBUILD_COUNT);
        }
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

    // The compile part of the world digest (#1146): the placement
    // fingerprints in index order, plus the entity count this job actually
    // produced. The count is the derived half and the one that matters - a
    // scatter's slope accept/reject (#1132) changes how many instances land,
    // so two peers that read the same record and placed a different number of
    // trees disagree here even though their records are identical.
    let compile_digest = crate::world_digest::compile_digest(
        generator_caches
            .world
            .units
            .iter()
            .map(|u| u.fingerprint.as_deref()),
        job.entities_spawned,
    );
    if let Some(digest) = generator_caches.digest.as_deref_mut() {
        digest.retarget(crate::world_digest::record_fingerprint(record));
        digest.compile = Some(compile_digest);
    }

    generator_caches.session_log.info(
        now,
        crate::diagnostics::event::EventPayload::WorldCompileCompleted {
            entity_count: job.entities_spawned,
            duration_secs: job.work.as_secs_f64(),
            digest: compile_digest,
            skipped_placements: job.skipped_units,
        },
    );
    // A budget stop is a user-facing fact (#1211), not a console line: the
    // resource feeds one toast and the World Editor footer, and a later
    // compile that builds everything clears it.
    if job.skipped_units > 0 {
        commands.insert_resource(super::super::WorldCompileTruncated {
            skipped_placements: job.skipped_units,
            first_skipped_index: job.skipped_from,
            announced: false,
        });
    } else {
        commands.remove_resource::<super::super::WorldCompileTruncated>();
    }

    // Unblock the loading gate: the world this record describes exists.
    // Idempotent on later jobs; removed by `ui::logout::cleanup_on_logout`.
    commands.insert_resource(super::super::WorldCompiled);
}

/// Diff the record against [`CompiledWorld`] and (re)build the job
/// queue. See the module docs for the full / incremental split.
///
/// A Shape node's grammar status names the placement and seed of a copy
/// that draws nothing, and a copy of the node writes it as it is built
/// (#1505). So where an incremental plan finds a placement pointed at
/// another generator, the statuses of the one it placed before are written
/// anew from the placements as they are, without building a copy of it
/// ([`ShapeMeshCache::statuses_under`]); and the statuses of a generator no
/// placement places any more are forgotten, whatever the plan.
#[allow(clippy::too_many_arguments)]
fn plan_job(
    commands: &mut Commands,
    existing: &Query<(Entity, Option<&PlacementUnit>), With<RoomEntity>>,
    record: &RoomRecord,
    heightmap_changed: bool,
    world: &mut CompiledWorld,
    job: &mut CompileJob,
    water_surfaces: &mut WaterSurfaces,
    shape_mesh: &mut ShapeMeshCache,
) {
    // What each generator is drawn with is read from the placements anew.
    shape_mesh.placements_changed();
    let placed_before = std::mem::take(&mut world.placed);
    let placed_now: Vec<Option<String>> = record
        .placements
        .iter()
        .map(|placement| job::placement_generator_ref(placement).map(str::to_owned))
        .collect();
    let placed: std::collections::HashSet<&str> =
        placed_now.iter().flatten().map(String::as_str).collect();

    // Indices whose spawned entities must be retired this plan. Filled
    // by the cursor abort + the diff below, then swept in one flat pass
    // over the `PlacementUnit` markers - anchor-recursive despawn alone
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

    // One generator scan + one terrain serialisation for the whole pass
    // (#673): `room_water_y` feeds both the fingerprints below and (via
    // the job) `start_unit`'s dry-land walk during the execute slices.
    let room_water_y = room_water_level(record);
    let fp_pass = FingerprintPass::new(record, room_water_y);

    let len = record.placements.len();
    // Full when the heightmap was swapped (every snapped transform
    // sampled the old surface), when the placement count SHRANK
    // (removal shifts the indices of everything after the removed
    // entry, and indices are unit identity - `PlacementMarker` values
    // on surviving anchors would go stale), or on the first compile
    // for this world (empty `CompiledWorld`; full coverage is what
    // keeps the end-of-job cache GC and the full-rebuild metric sound
    // after a logout / attract teardown reset it).
    //
    // Count GROWTH is deliberately NOT full (#979): every live-record
    // mutation appends (`placements.push` - no call site inserts
    // mid-list), so existing indices keep their placements, their
    // fingerprints match, and the diff below no-ops them. Only the new
    // tail queues - which is what stops the whole room from blinking
    // out (flat despawn, multi-frame sliced respawn) every time an
    // item is dropped in from the catalogue or inventory, for the
    // owner and for every peer receiving the record broadcast. If a
    // mid-list insert is ever added, it must NOT ride this path
    // unaudited: a shifted unit that survives on a matching
    // fingerprint keeps its index-seeded Grid `random_yaw` stream
    // (`step_unit` seeds off the placement index), silently diverging
    // from what a from-scratch compile of the same record produces.
    let full = heightmap_changed || world.units.len() > len || world.units.is_empty();
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
                fingerprint: unit_fingerprint(record, placement, &fp_pass),
            });
        }
    } else {
        // Appended placements enter the diff as never-compiled units
        // (`None` fingerprint, no anchor), so the per-index loop below
        // treats "new" and "stale" uniformly: the new tail always
        // mismatches and queues, the untouched prefix never does.
        if world.units.len() < len {
            world.units.resize_with(len, CompiledUnit::default);
        }
        for (index, placement) in record.placements.iter().enumerate() {
            let fingerprint = unit_fingerprint(record, placement, &fp_pass);
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

        // A placement pointed at another generator (#1505) is rebuilt as
        // the new one alone, and the generator it placed before keeps the
        // statuses its copies wrote - which may name that placement's seed
        // as one that draws nothing. Where the generator is still placed,
        // each of its Shape nodes the cache remembers to draw nothing with
        // some seed is given, here, the status a copy of it that draws
        // would write with the placements as they are - without building
        // one, since its first copy left may be a scatter of thousands.
        // Queued, so a status a unit built after this plan writes comes
        // after it. (A generator placed no more loses its statuses below.)
        let mut rewritten: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut statuses = Vec::new();
        for (index, now) in placed_now.iter().enumerate() {
            let Some(Some(was)) = placed_before.get(index) else {
                continue;
            };
            if now.as_deref() != Some(was.as_str())
                && placed.contains(was.as_str())
                && rewritten.insert(was.as_str())
            {
                statuses.extend(shape_mesh.statuses_under(record, was));
            }
        }
        if !statuses.is_empty() {
            commands.queue(move |ecs: &mut World| {
                if let Some(mut diagnostics) =
                    ecs.get_resource_mut::<super::super::grammar_diag::GrammarDiagnostics>()
                {
                    for (node, error) in statuses {
                        diagnostics.record(node, error);
                    }
                }
            });
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

    // A generator no placement places any more draws nothing, and no copy
    // of it will write its statuses again: they go (#1505). Queued, so they
    // go before any status a unit built after this plan records.
    let mut unplaced: Vec<String> = placed_before
        .iter()
        .flatten()
        .filter(|was| !placed.contains(was.as_str()))
        .cloned()
        .collect();
    unplaced.sort_unstable();
    unplaced.dedup();
    if !unplaced.is_empty() {
        commands.queue(move |ecs: &mut World| {
            if let Some(mut statuses) =
                ecs.get_resource_mut::<super::super::grammar_diag::GrammarDiagnostics>()
            {
                statuses.forget_generators(&unplaced);
            }
        });
    }
    world.placed = placed_now;

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
            active.room_water_y = room_water_y;
        }
        None if queue.is_empty() => {}
        None => job.0 = Some(ActiveJob::new(queue, full, room_water_y)),
    }
}

/// Outcome of [`start_unit`]: simple units commit immediately, grid /
/// scatter units hand back a cursor for the slice loop to drive.
/// The cursor is boxed-by-variant-size standards large (it carries a
/// ChaCha RNG state), but the enum lives only for the duration of one
/// `start_unit` return - no arrays of it ever exist - so the size skew
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
            super::pad::relocation_clearance(
                *avoid_water,
                avoid_water_clearance.0,
                transform.scale.0[0],
            ),
        ),
        Placement::Scatter {
            bounds,
            snap_to_terrain,
            ..
        } => (
            super::scatter::scatter_anchor(bounds),
            *snap_to_terrain,
            None,
        ),
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
            // Water-avoiding placements slide to dry land before the
            // height sample (may move X/Z, preserves bearing), then off
            // over-steep ground (#905) - the safety net under the
            // derive-time proxy siting. Both walks are gated on the
            // seeded pipeline's `avoid_water` opt-in, so editor-authored
            // placements are never second-guessed. The editor walks an
            // anchor through the same function wherever it shows one
            // (#1399), so the two cannot disagree about where it stands.
            if let Some(clearance) = avoid_water {
                super::pad::relocate_snapped_anchor(
                    hm,
                    &mut anchor_world_tf.translation,
                    clearance,
                    room_water_y,
                );
            }
            // Absolute placements keep their authored Y as an offset
            // from the snapped terrain height (the seeded landmark
            // sinks its foundations 0.35 m); Scatter / Grid anchors
            // keep the historical replace semantics.
            let authored_y = if matches!(placement, Placement::Absolute { .. }) {
                anchor_world_tf.translation.y
            } else {
                0.0
            };
            // A seeded structure resolves against its whole footprint,
            // not the one point under its centre (#1008) - otherwise a
            // hillside tilts the building around that point and buries
            // its uphill wall. Everything else keeps the plain sample:
            // scatter instances re-sample per instance (they are point
            // objects), and an editor placement was positioned by hand
            // against exactly this height.
            //
            // Read through the shared resolver, not the heightmap: the
            // editor's preview, drag commit and snap toggle read the same
            // one, and a disagreement between them and this line lands in
            // the stored offset and compounds per drag (#1011).
            let ground = super::pad::snapped_ground_y(
                hm,
                anchor_world_tf.translation.x,
                anchor_world_tf.translation.z,
                super::pad::snap_footprint_radius(placement),
            );
            anchor_world_tf.translation.y = ground + authored_y;
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
            // One dispatch - atomic; a single blueprint stays the
            // smallest unit of work the slicer can schedule.
            dispatch_top_level(ctx, generator_ref, Transform::IDENTITY, anchor);
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
            bounds,
            local_seed,
            count,
            naturalness,
            ..
        } => {
            // Resolve the biome-filter water threshold from the runtime
            // registry. One global Y per scatter, sampled at its centre
            // at unit start - placements that come before the
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
                    jitter_rng: Box::new(ChaCha8Rng::seed_from_u64(
                        local_seed ^ super::scatter::JITTER_SEED_SALT,
                    )),
                    clusters: super::scatter::cluster_centers(
                        bounds,
                        *count,
                        *local_seed,
                        naturalness.edge_falloff.0,
                    ),
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
/// deadline passes. Cell-for-cell identical to the monolithic pass -
/// the cursor carries the RNG so resuming doesn't shift the stream.
fn step_unit(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    cursor: &mut UnitCursor,
    deadline: Instant,
) -> StepOutcome {
    // `ctx.record` is a shared reference field - copying it out gives a
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
                    let world_y = ctx
                        .heightmap
                        .map(|hm| hm.world_height_at(world_pos.x, world_pos.z))
                        .unwrap_or(0.0);
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
                // A cell is a copy, and a small one is cut by distance (#1480).
                ctx.begin_copy(generator_ref, &cell_tf);
                dispatch_top_level(ctx, generator_ref, cell_tf, cursor.anchor);
                ctx.end_copy(generator_ref);
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
                avoid_urban,
                float_on_water,
                naturalness,
                ..
            },
            CursorKind::Scatter {
                spawned,
                attempts,
                rng,
                jitter_rng,
                clusters,
                water_level,
            },
        ) => {
            let terrain_cfg = crate::pds::find_terrain_config(ctx.record);
            let max_attempts = count.saturating_mul(10).max(*count);

            let urban_exclusions = super::scatter::urban_exclusions(ctx.record, *avoid_urban);
            let filters = super::scatter::SampleFilters {
                biome_filter,
                terrain_cfg,
                water_level: *water_level,
                urban_exclusions: &urban_exclusions,
                // `1 - normal.y` cutoff resolved once per unit (#912) - the
                // trigonometry would otherwise be paid per sample.
                slope_cutoff: super::scatter::slope_cutoff(naturalness),
            };

            while *spawned < *count && *attempts < max_attempts {
                if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
                    return StepOutcome::Done;
                }
                if Instant::now() >= deadline {
                    return StepOutcome::Yielded;
                }
                *attempts += 1;
                let Some((world_x, mut world_y, world_z)) = super::scatter::try_sample(
                    bounds,
                    naturalness,
                    clusters,
                    rng,
                    ctx.heightmap,
                    &filters,
                ) else {
                    continue;
                };
                // Floating cover (#914) rides the water surface instead of
                // the submerged terrain the sample landed on. `max` so a
                // sample on dry shore keeps its bank height - floating only
                // ever lifts, it never sinks an instance into the ground.
                if *float_on_water && let Some(wl) = *water_level {
                    world_y = world_y.max(wl);
                }

                // Make scatter children of the anchor so grabbing the
                // gizmo moves the whole forest live.
                let local_pos = cursor
                    .anchor_world_tf
                    .compute_affine()
                    .inverse()
                    .transform_point3(Vec3::new(world_x, world_y, world_z));
                // One fixed group of draws from the side stream, taken
                // whether or not the knobs are on, so any of them can be
                // toggled without disturbing the others (#912).
                let jitter = super::scatter::instance_jitter(jitter_rng, naturalness);
                let cell_tf =
                    super::scatter::instance_pose(local_pos, &jitter, *random_yaw, naturalness);

                // A small scattered copy is cut by distance (#1480).
                ctx.begin_copy(generator_ref, &cell_tf);
                dispatch_top_level(ctx, generator_ref, cell_tf, cursor.anchor);
                ctx.end_copy(generator_ref);
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
        // differ - but stay total rather than panicking the frame loop.
        _ => StepOutcome::Done,
    }
}

#[cfg(test)]
mod tests {
    //! Planner-level ECS coverage (#979): what a replan tears down. The
    //! fingerprint inputs are unit-tested in [`super::super::job`]; these
    //! tests drive the real [`compile_room_record`] system in a minimal
    //! headless app and watch anchor entities across record edits - the
    //! entity-identity view of "the room must not blink when an item is
    //! appended, and must still rebuild wholesale when one is removed".

    use std::collections::HashMap;
    use std::sync::Arc;

    use super::*;
    use crate::pds::{Environment, Fp, Fp3, Fp4, Generator, GeneratorKind, TransformData};

    fn test_placement(x: f32) -> Placement {
        Placement::Absolute {
            generator_ref: "box".to_string(),
            transform: TransformData {
                translation: Fp3([x, 0.0, 0.0]),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            snap_to_terrain: false,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: None,
        }
    }

    fn test_record(placements: usize) -> RoomRecord {
        let mut generators = HashMap::new();
        generators.insert("box".to_string(), Generator::default_cuboid());
        RoomRecord {
            lex_type: "network.symbios.room".to_string(),
            environment: Environment::default(),
            generators,
            placements: (0..placements)
                .map(|i| test_placement(i as f32 * 4.0))
                .collect(),
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            opaque_refs: Default::default(),
        }
    }

    /// A headless app carrying exactly the resources
    /// [`compile_room_record`]'s signature demands - no render, no
    /// terrain, no heightmap (`heightmap_changed` stays `false`, so the
    /// only full-rebuild triggers reachable here are the ones under
    /// test: first compile and count shrink).
    fn compile_app(record: RoomRecord) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<Image>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<WaterMaterial>();
        app.init_resource::<crate::world_builder::lsystem::LSystemMaterialCache>();
        app.init_resource::<crate::world_builder::lsystem::LSystemMeshCache>();
        app.init_resource::<crate::world_builder::shape::ShapeMaterialCache>();
        app.init_resource::<crate::world_builder::shape::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMaterialCache>();
        app.init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::spatial_audio::BakedAudioCache>();
        app.insert_resource(crate::world_builder::fresh_texture_cache());
        app.init_resource::<CompiledWorld>();
        app.init_resource::<CompileJob>();
        app.init_resource::<WaterSurfaces>();
        app.init_resource::<BlobImageCache>();
        app.init_resource::<crate::world_builder::audio_resolver::BlobAudioCache>();
        app.init_resource::<crate::diagnostics::SessionLog>();
        app.insert_resource(LiveRoomRecord(record));
        app.add_systems(Update, compile_room_record);
        app
    }

    /// Update until the in-flight job drains. Panics rather than loops
    /// forever - a queue that never empties is itself a failure.
    fn settle(app: &mut App) {
        for _ in 0..64 {
            app.update();
            if app.world().resource::<CompileJob>().0.is_none() {
                return;
            }
        }
        panic!("compile job did not settle within 64 frames");
    }

    /// The Shape geometry cache the compile keeps.
    fn shape_cache(app: &App) -> &ShapeMeshCache {
        app.world().resource::<ShapeMeshCache>()
    }

    /// How many variants that draw nothing the Shape cache remembers.
    fn remembered_failures(app: &App) -> usize {
        shape_cache(app).failures.values().map(HashMap::len).sum()
    }

    fn unit_anchors(app: &App) -> Vec<Option<Entity>> {
        app.world()
            .resource::<CompiledWorld>()
            .units
            .iter()
            .map(|u| u.anchor)
            .collect()
    }

    /// Every spawned entity claimed by a placement unit, with its index.
    fn unit_members(app: &mut App) -> Vec<(Entity, usize)> {
        app.world_mut()
            .query::<(Entity, &PlacementUnit)>()
            .iter(app.world())
            .map(|(e, u)| (e, u.0))
            .collect()
    }

    /// #1512: a turned rect scatter's anchor - the frame its copies hang
    /// from, and face by when they keep no yaw of their own - turns as the
    /// sampler lays the rect out. Its X ran mirrored from the rect's.
    #[test]
    fn a_turned_rect_scatters_anchor_turns_as_the_rect_is_laid_out() {
        let rotation = 0.6;
        let mut record = test_record(0);
        record.placements = vec![Placement::Scatter {
            generator_ref: "box".to_string(),
            bounds: ScatterBounds::Rect {
                center: crate::pds::Fp2([10.0, -4.0]),
                extents: crate::pds::Fp2([40.0, 5.0]),
                rotation: Fp(rotation),
            },
            count: 4,
            local_seed: 7,
            biome_filter: Default::default(),
            snap_to_terrain: false,
            random_yaw: false,
            avoid_urban: false,
            float_on_water: false,
            naturalness: Default::default(),
        }];
        let mut app = compile_app(record);
        settle(&mut app);
        let anchor = unit_anchors(&app)[0].expect("the scatter spawned its anchor");
        let turn = app
            .world()
            .get::<Transform>(anchor)
            .expect("the anchor has a transform")
            .rotation;
        let (x, z) = super::super::scatter::rect_point([0.0, 0.0], rotation, 1.0, 0.0);
        let along = turn * Vec3::X;
        assert!(
            (along - Vec3::new(x, 0.0, z)).length() < 1e-5,
            "the anchor's X runs along {along}, the rect's along ({x}, 0, {z})"
        );
    }

    #[test]
    fn append_keeps_existing_units_alive() {
        let mut app = compile_app(test_record(3));
        settle(&mut app);
        let anchors_before = unit_anchors(&app);
        assert_eq!(anchors_before.len(), 3);
        assert!(anchors_before.iter().all(|a| a.is_some()));
        let members_before = unit_members(&mut app);
        assert!(!members_before.is_empty());

        // The catalogue / inventory drop shape: one appended placement.
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements
            .push(test_placement(99.0));
        settle(&mut app);

        let anchors_after = unit_anchors(&app);
        assert_eq!(anchors_after.len(), 4);
        // The prefix anchors are the SAME entities - never despawned,
        // never respawned. `Entity` equality includes the generation,
        // so a despawn recycled into the same slot would still fail.
        assert_eq!(
            &anchors_after[..3],
            &anchors_before[..],
            "append must not touch existing units"
        );
        // Their whole spawned trees survive too, not just the anchors.
        for (entity, index) in &members_before {
            assert!(
                app.world().get_entity(*entity).is_ok(),
                "unit {index} member despawned by an append"
            );
        }
        assert!(anchors_after[3].is_some(), "appended unit must compile");
    }

    #[test]
    fn append_to_an_empty_room_compiles() {
        // 0 → 1 rides the first-compile (empty `CompiledWorld`) full
        // path; there is nothing to preserve, but the drop must build.
        let mut app = compile_app(test_record(0));
        settle(&mut app);
        assert!(unit_anchors(&app).is_empty());

        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements
            .push(test_placement(0.0));
        settle(&mut app);

        let anchors = unit_anchors(&app);
        assert_eq!(anchors.len(), 1);
        assert!(anchors[0].is_some());
    }

    #[test]
    fn removal_still_rebuilds_wholesale() {
        // Removal shifts every later index, so index identity is void:
        // the shrink must tear the whole compiled world down and rebuild
        // it under the new indices.
        let mut app = compile_app(test_record(3));
        settle(&mut app);
        let anchors_before = unit_anchors(&app);

        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements
            .remove(0);
        settle(&mut app);

        let anchors_after = unit_anchors(&app);
        assert_eq!(anchors_after.len(), 2);
        assert!(anchors_after.iter().all(|a| a.is_some()));
        for anchor in anchors_before.iter().flatten() {
            assert!(
                app.world().get_entity(*anchor).is_err(),
                "stale anchor survived a shrink"
            );
        }
    }

    #[test]
    fn content_edit_after_append_stays_scoped() {
        // Composition check: an append followed by a transform edit of
        // one OLD unit rebuilds that unit alone - the append must not
        // have wedged the diff's index bookkeeping.
        let mut app = compile_app(test_record(2));
        settle(&mut app);
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements
            .push(test_placement(99.0));
        settle(&mut app);
        let anchors_before = unit_anchors(&app);
        assert_eq!(anchors_before.len(), 3);

        {
            let mut record = app.world_mut().resource_mut::<LiveRoomRecord>();
            let Placement::Absolute { transform, .. } = &mut record.0.placements[1] else {
                panic!("test record uses Absolute placements");
            };
            transform.translation.0[2] += 5.0;
        }
        settle(&mut app);

        let anchors_after = unit_anchors(&app);
        assert_eq!(anchors_after[0], anchors_before[0], "unit 0 untouched");
        assert_eq!(anchors_after[2], anchors_before[2], "unit 2 untouched");
        assert_ne!(anchors_after[1], anchors_before[1], "unit 1 rebuilt");
    }

    /// #1399: the editor reads a snapped Absolute anchor exactly where this
    /// compile draws it, walk and all, and a record carrying that pose with
    /// snap off draws in the same place - the snap toggle's "turning it OFF
    /// keeps it where it is". Compiled for real: the editor's reading is
    /// only worth its agreement with the executor.
    #[test]
    fn the_editor_reads_a_snapped_anchor_where_the_compile_draws_it() {
        let snapped = |x: f32, avoid_water: bool| Placement::Absolute {
            generator_ref: "box".to_string(),
            transform: TransformData {
                translation: Fp3([x, -0.35, 0.0]),
                ..Default::default()
            },
            snap_to_terrain: true,
            avoid_water,
            avoid_water_clearance: Fp(3.0),
            seed: None,
        };
        let mut record = test_record(0);
        // The seeded layout: the room's water is a child of its terrain,
        // here at 0 - the line `wet_ramp` is drawn against.
        let mut terrain = Generator::from_kind(GeneratorKind::Terrain(Default::default()));
        terrain
            .children
            .push(Generator::from_kind(GeneratorKind::Water {
                surface: Default::default(),
            }));
        record
            .generators
            .insert("base_terrain".to_string(), terrain);
        record.placements = vec![
            // Controls first. Seeded, recorded on dry ground: stays.
            snapped(44.0, true),
            // Hand-placed in the water: never second-guessed.
            snapped(25.0, false),
            // Seeded, recorded in the water: walked out along its bearing.
            snapped(25.0, true),
        ];
        let room_water_y = room_water_level(&record);
        assert_eq!(room_water_y, Some(0.0));

        let mut app = compile_app(record);
        app.insert_resource(super::super::pad::wet_ramp());
        settle(&mut app);
        let drawn = |app: &App| -> Vec<Vec3> {
            unit_anchors(app)
                .into_iter()
                .map(|anchor| {
                    let anchor = anchor.expect("every unit spawns an anchor");
                    app.world()
                        .get::<Transform>(anchor)
                        .expect("an anchor")
                        .translation
                })
                .collect()
        };
        let bits = |v: Vec3| v.to_array().map(f32::to_bits);
        let snapped_poses = drawn(&app);

        // The fixture does what it says: the walk moved the seeded anchor
        // recorded in the water, and nothing else.
        let xz = |v: Vec3| [v.x, v.z];
        assert_eq!(xz(snapped_poses[0]), [44.0, 0.0]);
        assert_eq!(xz(snapped_poses[1]), [25.0, 0.0]);
        assert_eq!(xz(snapped_poses[2]), [37.0, 0.0], "walked 12 m out");

        let hm = super::super::pad::wet_ramp();
        let mut record = app.world().resource::<LiveRoomRecord>().0.clone();
        for (i, placement) in record.placements.iter_mut().enumerate() {
            let Placement::Absolute {
                transform,
                snap_to_terrain,
                avoid_water,
                avoid_water_clearance,
                ..
            } = placement
            else {
                panic!("test record uses Absolute placements");
            };
            let read = crate::world_builder::snapped_absolute_anchor(
                &hm.0,
                transform,
                *avoid_water,
                avoid_water_clearance.0,
                room_water_y,
            );
            assert_eq!(
                bits(read),
                bits(snapped_poses[i]),
                "placement {i}: the editor reads {read}, the compile draws {}",
                snapped_poses[i]
            );
            // Un-snapped the way the editor's toggle does it: the pose it
            // was drawn at becomes its absolute translation.
            transform.translation = Fp3(read.to_array());
            *snap_to_terrain = false;
        }
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = record;
        settle(&mut app);
        for (i, (after, before)) in drawn(&app).into_iter().zip(&snapped_poses).enumerate() {
            assert_eq!(
                bits(after),
                bits(*before),
                "placement {i} moved from {before} to {after} when snap was turned off"
            );
        }
    }

    /// [`compile_app`] with avian's physics running beside the compile, one
    /// fixed step per update, as `tests/freeze_rigid_body.rs` stands it up.
    fn compile_app_with_physics(record: RoomRecord) -> App {
        let mut app = compile_app(record);
        app.add_plugins((TransformPlugin, bevy::scene::ScenePlugin));
        app.add_plugins(PhysicsPlugins::default());
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 64.0),
        ));
        app.insert_resource(Time::<Fixed>::from_hz(64.0));
        app.finish();
        app.cleanup();
        app
    }

    /// #1453, found live: the admin said the agent's gateway did nothing.
    /// Under a NON-solid root, every collider kept the offset from the
    /// placement's static body that avian gave it before any transform had
    /// propagated, because the tree was hung together bottom-up and avian
    /// never learnt that the anchor had colliders below it: a walk-in zone
    /// drawn at (13, 24.6, -30) collided at (0, 2.25, 0). A solid part, a
    /// gateway's zone and a solid part two levels down must each collide
    /// where they are drawn - in a placement turned and moved off the origin.
    #[test]
    fn colliders_under_a_non_solid_root_collide_where_they_are_drawn() {
        let tree: Generator = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.cuboid", "size": [1000, 1000, 1000], "solid": false,
            "children": [
                {"$type": "network.symbios.gen.cuboid", "size": [10000, 10000, 10000],
                 "solid": true, "transform": {"translation": [0, 20000, 0]}},
                {"$type": "network.symbios.gen.gateway", "size": [30000, 40000, 10000],
                 "transform": {"translation": [30000, 20000, 0]}},
                {"$type": "network.symbios.gen.cuboid", "size": [1000, 1000, 1000],
                 "solid": false, "transform": {"translation": [0, 0, 20000]},
                 "children": [
                    {"$type": "network.symbios.gen.cuboid", "size": [10000, 10000, 10000],
                     "solid": true, "transform": {"translation": [0, 10000, 0]}}
                 ]}
            ]
        }))
        .expect("wire JSON");
        let mut record = test_record(0);
        record.generators.insert("tree".into(), tree);
        let turn = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        record.placements.push(Placement::Absolute {
            generator_ref: "tree".into(),
            transform: TransformData {
                translation: Fp3([10.0, 0.0, -5.0]),
                rotation: Fp4(turn.to_array()),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            snap_to_terrain: false,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: None,
        });
        let mut app = compile_app_with_physics(record);
        settle(&mut app);
        for _ in 0..4 {
            app.update();
        }

        let colliders: Vec<(Vec3, Vec3)> = app
            .world_mut()
            .query_filtered::<(&Position, &GlobalTransform), (With<ColliderMarker>, Without<RigidBody>)>()
            .iter(app.world())
            .map(|(position, global)| (position.0, global.translation()))
            .collect();
        assert_eq!(
            colliders.len(),
            3,
            "two solid parts and a zone: {colliders:?}"
        );
        for (collides_at, drawn_at) in colliders {
            assert!(
                collides_at.distance(drawn_at) < 1e-3,
                "collides at {collides_at}, drawn at {drawn_at}"
            );
        }
    }

    /// #1506: a Shape node's terminals listed solid collide where they are
    /// drawn, each as the box of its scope - in a placement turned and moved
    /// off the origin, under a non-solid root - and the rest do not: the
    /// wall holds a point inside it, the doorway beside it holds none.
    /// Listing the door instead, an edit, swaps them, so a changed list is
    /// not answered from the cached spawn list.
    #[test]
    fn a_grammar_s_solid_terminals_collide_where_they_are_drawn() {
        use bevy::ecs::system::RunSystemOnce;
        let house = |solid: &[&str]| -> Generator {
            serde_json::from_value(serde_json::json!({
                "$type": "network.symbios.gen.cuboid", "size": [1000, 1000, 1000], "solid": false,
                "children": [{
                    "$type": "network.symbios.gen.shape",
                    "grammar_source": "Lot --> Extrude(2) Split(X) { ~1: Wall | ~1: Door }\n\
                                       Wall --> I(\"Wall\")\nDoor --> I(\"Door\")",
                    "root_rule": "Lot",
                    "footprint": [40_000, 0, 10_000],
                    "seed": "1",
                    "solid_meshes": solid,
                    "transform": {"translation": [0, 5_000, 0]},
                }]
            }))
            .expect("wire JSON")
        };
        let mut record = test_record(0);
        record.generators.insert("house".into(), house(&["Wall"]));
        let turn = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        record.placements.push(Placement::Absolute {
            generator_ref: "house".into(),
            transform: TransformData {
                translation: Fp3([10.0, 0.0, -5.0]),
                rotation: Fp4(turn.to_array()),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            snap_to_terrain: false,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: None,
        });
        // The middle of each 2 x 2 x 1 m terminal, from the node's frame
        // (wall at x 0..2, door at x 2..4, 0.5 m up the root) to the world.
        let world = |x: f32| Vec3::new(10.0, 0.0, -5.0) + turn * Vec3::new(x, 1.5, 0.5);
        let (wall, door) = (world(1.0), world(3.0));
        let mut app = compile_app_with_physics(record);
        let held = |app: &mut App| {
            settle(app);
            for _ in 0..4 {
                app.update();
            }
            let drawn: Vec<Vec3> = app
                .world_mut()
                .query_filtered::<&GlobalTransform, With<Mesh3d>>()
                .iter(app.world())
                .map(GlobalTransform::translation)
                .collect();
            for at in [wall, door] {
                assert!(
                    drawn.iter().any(|d| d.distance(at) < 1e-3),
                    "a terminal is drawn at {at}: {drawn:?}"
                );
            }
            app.world_mut()
                .run_system_once(move |query: SpatialQuery| {
                    [wall, door].map(|at| {
                        !query
                            .point_intersections(at, &SpatialQueryFilter::default())
                            .is_empty()
                    })
                })
                .expect("the spatial query runs")
        };
        assert_eq!(held(&mut app), [true, false], "the wall is solid");

        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .generators
            .insert("house".into(), house(&["Door"]));
        assert_eq!(held(&mut app), [false, true], "the door is solid now");
    }

    /// #1506, the critic's case: the flat walls `Comp(Faces)` hands out, on
    /// a node scaled 2 x 1 x 1, collide where they are drawn - a ray at
    /// mid-height meets the far side wall 1 mm thick where it stands, and
    /// one a metre past the building's end meets nothing. avian scales a
    /// compound's pieces along each piece's own axes, and a side wall is
    /// turned in the node: given as a turned box, its collider ran double
    /// its length past both ends of the building.
    #[test]
    fn flat_solid_walls_collide_where_drawn_on_a_scaled_node() {
        use bevy::ecs::system::RunSystemOnce;
        let house: Generator = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.shape",
            "grammar_source": "Lot --> Extrude(3) Comp(Faces) { Side: Wall }\nWall --> I(\"Wall\")",
            "root_rule": "Lot",
            "footprint": [40_000, 0, 100_000],
            "seed": "1",
            "solid_meshes": ["Wall"],
            "transform": {"scale": [20_000, 10_000, 10_000]},
        }))
        .expect("wire JSON");
        let mut record = test_record(0);
        record.generators.insert("house".into(), house);
        record.placements.push(placed("house", 0, None));
        let mut app = compile_app_with_physics(record);
        settle(&mut app);
        for _ in 0..4 {
            app.update();
        }
        // The block stands over x 0..8 (4 m scaled 2) and z 0..10.
        let hits = app
            .world_mut()
            .run_system_once(|query: SpatialQuery| {
                [5.0_f32, 11.0, -1.0].map(|z| {
                    query
                        .cast_ray(
                            Vec3::new(20.0, 1.5, z),
                            Dir3::NEG_X,
                            40.0,
                            true,
                            &SpatialQueryFilter::default(),
                        )
                        .map(|hit| hit.distance)
                })
            })
            .expect("the spatial query runs");
        let [inside, past_the_end, before_the_start] = hits;
        let met = inside.expect("the side wall at x = 8 stops the ray");
        assert!((met - 12.0).abs() < 0.01, "met at {met}");
        assert_eq!(past_the_end, None, "an invisible wall past z = 10");
        assert_eq!(before_the_start, None, "an invisible wall before z = 0");
    }

    /// The node seed of [`tower`]'s grammar.
    const OWN_SEED: u64 = 1;

    /// A grammar whose height its seed draws (#1505): `Extrude(rand(2, 9))`
    /// on a 2 m square, so two seeds draw two towers. `seed` is the node's
    /// own.
    fn tower(seed: u64) -> Generator {
        serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.shape",
            "grammar_source": "Lot --> Extrude(rand(2, 9)) I(\"Block\")",
            "root_rule": "Lot",
            "footprint": [20_000, 0, 20_000],
            "seed": seed.to_string(),
        }))
        .expect("a Shape node")
    }

    /// An unsnapped absolute placement of `name` 10 m along per `slot`, with
    /// `seed` or none.
    fn placed(name: &str, slot: usize, seed: Option<u64>) -> Placement {
        Placement::Absolute {
            generator_ref: name.to_string(),
            transform: TransformData {
                translation: Fp3([slot as f32 * 10.0, 0.0, 0.0]),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            snap_to_terrain: false,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed,
        }
    }

    /// A room of `tower(OWN_SEED)` placed once per entry of `seeds`.
    fn towers(seeds: &[Option<u64>]) -> RoomRecord {
        let mut record = test_record(0);
        record
            .generators
            .insert("tower".to_string(), tower(OWN_SEED));
        record.placements = seeds
            .iter()
            .enumerate()
            .map(|(slot, seed)| placed("tower", slot, *seed))
            .collect();
        record
    }

    /// What each unit draws: every entity under its anchor with a mesh, as
    /// the bits of its transform under its parent and its mesh, in spawn
    /// order.
    fn drawn(app: &App) -> Vec<Vec<([u32; 10], AssetId<Mesh>)>> {
        let world = app.world();
        unit_anchors(app)
            .into_iter()
            .map(|anchor| {
                let mut parts = Vec::new();
                let mut todo = vec![anchor.expect("every unit spawns an anchor")];
                while let Some(entity) = todo.pop() {
                    if let (Some(tf), Some(mesh)) =
                        (world.get::<Transform>(entity), world.get::<Mesh3d>(entity))
                    {
                        let bits = [
                            tf.translation.to_array(),
                            tf.scale.to_array(),
                            [tf.rotation.x, tf.rotation.y, tf.rotation.z],
                        ]
                        .concat()
                        .into_iter()
                        .chain([tf.rotation.w])
                        .map(f32::to_bits)
                        .collect::<Vec<u32>>();
                        parts.push((bits.try_into().expect("ten floats"), mesh.id()));
                    }
                    if let Some(children) = world.get::<Children>(entity) {
                        todo.extend(children.iter().rev());
                    }
                }
                parts
            })
            .collect()
    }

    /// #1505: a placement's seed replaces its Shape node's own - not mixed
    /// with it. Two placements of one generator with two seeds draw two
    /// towers, a placement whose seed is the node's own draws, to the bit,
    /// what an unseeded one draws, and a seeded placement draws what the
    /// same tree with that seed written into it draws unseeded - the tree
    /// the agent's z-fighting check and the render tool read
    /// (`Generator::with_shape_seed`).
    #[test]
    fn a_placement_s_seed_replaces_its_shape_nodes_seed() {
        let mut record = towers(&[None, Some(OWN_SEED), Some(7), Some(8)]);
        record
            .generators
            .insert("tower_7".to_string(), tower(OWN_SEED).with_shape_seed(7));
        record.placements.push(placed("tower_7", 4, None));
        let mut app = compile_app(record);
        settle(&mut app);
        let drawn = drawn(&app);

        assert_eq!(drawn[0].len(), 1, "one block a tower: {:?}", drawn[0]);
        assert_eq!(drawn[1], drawn[0], "its own seed draws what no seed draws");
        assert_ne!(drawn[2], drawn[0], "seed 7 draws another tower");
        assert_ne!(drawn[3], drawn[2], "and seed 8 a third");
        assert_eq!(
            drawn[4], drawn[2],
            "the tree with 7 written into it draws what the placement's 7 draws"
        );
    }

    /// #1505: the geometry cache keeps one entry per node and seed. Three
    /// seeds and the node's own each keep theirs across a second compile
    /// (a full one: a placement removed) - the same derivation, not a
    /// re-derived copy - and the variant no placement draws any more is
    /// dropped by that compile's cache GC.
    #[test]
    fn each_seed_keeps_its_own_geometry_and_a_dropped_one_goes() {
        let mut app = compile_app(towers(&[Some(3), Some(4), Some(5), None]));
        settle(&mut app);
        let cached = |app: &App| -> Vec<((String, u64), usize)> {
            let mut entries: Vec<((String, u64), usize)> = shape_cache(app)
                .builds
                .iter()
                .flat_map(|(node, seeds)| {
                    seeds.iter().map(|(seed, built)| {
                        (
                            (node.clone(), *seed),
                            Arc::as_ptr(&built.value).cast::<()>() as usize,
                        )
                    })
                })
                .collect();
            entries.sort();
            entries
        };
        let first = cached(&app);
        let key = |seed: u64| ("tower".to_string(), seed);
        assert_eq!(
            first.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
            [key(OWN_SEED), key(3), key(4), key(5)],
            "one entry per seed drawn"
        );

        // Placement 2 (seed 5) removed: a shrink, so a full rebuild and GC.
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements
            .remove(2);
        settle(&mut app);
        let kept: Vec<((String, u64), usize)> =
            first.into_iter().filter(|(k, _)| *k != key(5)).collect();
        assert_eq!(
            cached(&app),
            kept,
            "the live variants kept their derivations, and seed 5's went"
        );
    }

    /// #1505: a seed change rebuilds the placement it is on, and no other -
    /// the per-placement fingerprint hashes the placement's JSON, the seed
    /// among it - and the rebuilt one draws the new seed; taking the seed
    /// off draws the node's own again.
    #[test]
    fn a_seed_change_rebuilds_its_placement_alone() {
        // The last tower is unseeded: what the node's own seed draws.
        let mut app = compile_settled(towers(&[Some(3), Some(4), Some(5), None]));
        let (anchors, before) = (unit_anchors(&app), drawn(&app));

        let reseed = |app: &mut App, seed: Option<u64>| {
            let mut record = app.world_mut().resource_mut::<LiveRoomRecord>();
            let Placement::Absolute { seed: at, .. } = &mut record.0.placements[1] else {
                panic!("towers are placed absolutely");
            };
            *at = seed;
        };
        reseed(&mut app, Some(9));
        settle(&mut app);
        let (after, now) = (unit_anchors(&app), drawn(&app));
        for unit in [0, 2, 3] {
            assert_eq!(after[unit], anchors[unit], "unit {unit} untouched");
            assert_eq!(now[unit], before[unit], "unit {unit} draws as it did");
        }
        assert_ne!(after[1], anchors[1], "unit 1 rebuilt");
        assert_ne!(now[1], before[1], "unit 1 draws seed 9");

        reseed(&mut app, None);
        settle(&mut app);
        assert_ne!(unit_anchors(&app)[1], after[1], "unit 1 rebuilt again");
        assert_eq!(
            drawn(&app)[1],
            before[3],
            "unseeded, it draws the node's own seed"
        );
    }

    /// [`compile_app`] for `record`, settled.
    fn compile_settled(record: RoomRecord) -> App {
        let mut app = compile_app(record);
        settle(&mut app);
        app
    }

    /// #1505: the seed is for shape grammars and nothing else. A tree of a
    /// stochastic L-system under a box, placed with a seed and without,
    /// draws the same meshes in the same places - though the L-system with
    /// that seed written into its own draws another shrub, so a seed that
    /// reached it would show.
    #[test]
    fn a_seed_on_a_tree_with_no_grammar_draws_it_as_before() {
        let mut shrub = crate::catalogue::by_slug("lsys_dead_shrub")
            .expect("a stochastic L-system in the catalogue")
            .build("");
        // Plain materials: the meshes are what is compared.
        if let GeneratorKind::LSystem { materials, .. } = &mut shrub.kind {
            materials.clear();
        }
        let mut tree = Generator::default_cuboid();
        tree.children.push(shrub.clone());
        let mut reseeded = tree.clone();
        if let GeneratorKind::LSystem { seed, .. } = &mut reseeded.children[0].kind {
            *seed = 99;
        }
        let mut record = test_record(0);
        record.generators.insert("shrub".to_string(), tree);
        record.generators.insert("shrub_99".to_string(), reseeded);
        record.placements = vec![
            placed("shrub", 0, None),
            placed("shrub", 1, Some(99)),
            placed("shrub_99", 2, None),
        ];
        let app = compile_settled(record);
        let drawn = drawn(&app);
        assert!(drawn[0].len() >= 2, "a box and a shrub: {:?}", drawn[0]);
        assert_ne!(
            drawn[2], drawn[0],
            "fixture: the L-system's own seed changes what it draws"
        );
        assert_eq!(drawn[1], drawn[0], "a placement's seed leaves it alone");
    }

    /// #1505: a placement's seed reaches every Shape node of the tree it
    /// plants, however deep - not the root alone. Every Ashmere house is a
    /// grammar under a primitive; here a box holds one tower and a second
    /// box, which holds another. Placed with the towers' own seed it draws
    /// what it draws with none; placed with 7, what the tree with 7 written
    /// into both towers draws.
    #[test]
    fn a_placement_s_seed_reaches_every_grammar_under_a_primitive() {
        let mut upper = Generator::default_cuboid();
        upper.transform.translation = Fp3([0.0, 3.0, 0.0]);
        upper.children.push(tower(OWN_SEED));
        let mut house = Generator::default_cuboid();
        house.children = vec![tower(OWN_SEED), upper];
        let mut record = test_record(0);
        record
            .generators
            .insert("house_7".to_string(), house.with_shape_seed(7));
        record.generators.insert("house".to_string(), house);
        record.placements = vec![
            placed("house", 0, None),
            placed("house", 1, Some(OWN_SEED)),
            placed("house", 2, Some(7)),
            placed("house_7", 3, None),
        ];
        let drawn = drawn(&compile_settled(record));

        assert_eq!(drawn[0].len(), 4, "two boxes, two towers: {:?}", drawn[0]);
        assert_eq!(drawn[1], drawn[0], "their own seed draws what none draws");
        assert_ne!(drawn[2], drawn[0], "fixture: 7 draws other towers");
        assert_eq!(
            drawn[2], drawn[3],
            "7 reaches both towers, the one in the second box too"
        );
    }

    /// #1505: a placement's seed is for shape grammars alone. A particle
    /// system in the tree keeps its own seed, so every copy of one
    /// generator emits the same stream - houses moved onto one shared
    /// generator keep their houses but share one chimney's smoke, which
    /// docs/building.md says. A box with a grammar and an emitter of seed
    /// 42, placed with no seed, with 7 and with 42: each emitter's RNG
    /// starts from 42.
    #[test]
    fn a_placement_s_seed_leaves_a_particle_system_its_own() {
        let mut smoke = Generator::from_kind(GeneratorKind::default_particles());
        let GeneratorKind::ParticleSystem(params) = &mut smoke.kind else {
            panic!("default_particles is a particle system");
        };
        params.seed = 42;
        let mut house = Generator::default_cuboid();
        house.children = vec![tower(OWN_SEED), smoke];
        let mut record = test_record(0);
        record.generators.insert("house".to_string(), house);
        record.placements = vec![
            placed("house", 0, None),
            placed("house", 1, Some(7)),
            placed("house", 2, Some(42)),
        ];
        let app = compile_settled(record);
        let world = app.world();
        let seeds: Vec<Vec<[u8; 32]>> = unit_anchors(&app)
            .into_iter()
            .map(|anchor| {
                let mut seeds = Vec::new();
                let mut todo = vec![anchor.expect("every unit spawns an anchor")];
                while let Some(entity) = todo.pop() {
                    if let Some(state) =
                        world.get::<crate::world_builder::particles::EmitterState>(entity)
                    {
                        seeds.push(state.rng.get_seed());
                    }
                    if let Some(children) = world.get::<Children>(entity) {
                        todo.extend(children.iter());
                    }
                }
                seeds
            })
            .collect();
        let own = ChaCha8Rng::seed_from_u64(42).get_seed();
        assert_eq!(seeds, vec![vec![own]; 3]);
        assert_ne!(
            drawn(&app)[1],
            drawn(&app)[0],
            "fixture: the placement's 7 reached the grammar"
        );
    }

    /// A grammar that tosses a coin by its seed (#1505): a 2 m block, or
    /// nothing - and a derivation that draws nothing is a grammar error.
    /// `seed` is the node's own.
    fn coin_hut(seed: u64) -> Generator {
        serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.shape",
            "grammar_source": "Lot --> 50% Solid | 50% Void\n\
                               Solid --> Extrude(2) I(\"Block\")\n\
                               Void --> NIL",
            "root_rule": "Lot",
            "footprint": [20_000, 0, 20_000],
            "seed": seed.to_string(),
        }))
        .expect("a Shape node")
    }

    /// Seeds for [`coin_hut`]: two that draw its block, and one that draws
    /// nothing - found by deriving, so that a change in how the grammar
    /// engine tosses its coins moves the seeds, not the test.
    fn hut_seeds() -> ([u64; 2], u64) {
        let draws = |seed: u64| {
            coin_hut(seed)
                .kind
                .shape_def()
                .expect("a Shape node")
                .derive()
                .is_ok()
        };
        let mut good = (1..64).filter(|&seed| draws(seed));
        let good = [
            good.next().expect("a seed that draws"),
            good.next().expect("a second"),
        ];
        let bad = (1..64)
            .find(|&seed| !draws(seed))
            .expect("a seed that draws nothing");
        (good, bad)
    }

    /// #1505: a Shape node drawn with several seeds shows an error as long
    /// as the world draws it with a seed that fails - whichever copy
    /// compiled last, and after a compile that rebuilt only a copy that
    /// draws. A hut whose own seed draws its block, placed with no seed and
    /// with a seed that draws nothing: the node's grammar status names the
    /// failing placement and seed, in either order and after the unseeded
    /// copy alone is moved (a cache hit); it clears once that placement's
    /// seed draws, and once the grammar is fixed for the failing seed.
    #[test]
    fn a_failing_seed_keeps_its_grammar_error_showing() {
        use crate::world_builder::grammar_diag::{GrammarDiagnostics, GrammarStatus};
        let ([own, other], bad) = hut_seeds();
        let compiled = |seeds: [Option<u64>; 2]| {
            let mut record = test_record(0);
            record.generators.insert("hut".to_string(), coin_hut(own));
            record.placements = seeds
                .iter()
                .enumerate()
                .map(|(slot, seed)| placed("hut", slot, *seed))
                .collect();
            let mut app = compile_app(record);
            app.init_resource::<GrammarDiagnostics>();
            settle(&mut app);
            app
        };
        let status = |app: &App| {
            app.world()
                .resource::<GrammarDiagnostics>()
                .get("hut")
                .cloned()
        };
        let failing = |at: usize| {
            Some(GrammarStatus::Error {
                message: format!(
                    "with placement #{at}'s seed {bad}: \
                     grammar produced no geometry (no terminal shapes)"
                ),
            })
        };
        let edit = |app: &mut App, change: &dyn Fn(&mut RoomRecord)| {
            change(&mut app.world_mut().resource_mut::<LiveRoomRecord>().0);
            settle(app);
        };

        for (seeds, at) in [([Some(bad), None], 0), ([None, Some(bad)], 1)] {
            let app = compiled(seeds);
            let drawn = drawn(&app);
            assert_eq!(
                (drawn[at].len(), drawn[1 - at].len()),
                (0, 1),
                "fixture: {seeds:?} draw nothing and a block"
            );
            assert_eq!(status(&app), failing(at), "{seeds:?}");
        }

        let mut app = compiled([Some(bad), None]);
        let anchors = unit_anchors(&app);
        edit(&mut app, &|record| {
            let Placement::Absolute { transform, .. } = &mut record.placements[1] else {
                panic!("huts are placed absolutely");
            };
            transform.translation.0[2] += 5.0;
        });
        let moved = unit_anchors(&app);
        assert_eq!(moved[0], anchors[0], "fixture: the failing copy is kept");
        assert_ne!(moved[1], anchors[1], "fixture: the unseeded one rebuilt");
        assert_eq!(status(&app), failing(0), "a rebuilt copy that draws");

        edit(&mut app, &|record| {
            let Placement::Absolute { seed, .. } = &mut record.placements[0] else {
                panic!("huts are placed absolutely");
            };
            *seed = Some(other);
        });
        assert_eq!(status(&app), Some(GrammarStatus::Ok), "reseeded to draw");
        // What no placement draws any more is forgotten when the node is
        // next derived - here with the placement's new seed - a dead build
        // too, not kept until a full compile.
        let failed = |app: &App| {
            shape_cache(app)
                .failures
                .get("hut")
                .is_some_and(|seeds| seeds.contains_key(&bad))
        };
        assert!(!failed(&app), "forgotten at the reseed");
        // And a full compile's GC drops one that no copy derived since: the
        // failing placement removed, the copy left hits the cache.
        let mut app = compiled([Some(bad), None]);
        assert!(failed(&app), "fixture: remembered while drawn");
        edit(&mut app, &|record| {
            record.placements.remove(0);
        });
        assert!(!failed(&app), "dropped by a full compile's GC");
        assert_eq!(status(&app), Some(GrammarStatus::Ok), "the copy left draws");

        let mut app = compiled([Some(bad), None]);
        edit(&mut app, &|record| {
            let GeneratorKind::Shape { grammar_source, .. } =
                &mut record.generators.get_mut("hut").expect("the hut").kind
            else {
                panic!("the hut is a Shape node");
            };
            *grammar_source = "Lot --> Extrude(2) I(\"Block\")".to_string();
        });
        assert_eq!(drawn(&app)[0].len(), 1, "fixed, the failing seed draws");
        assert_eq!(status(&app), Some(GrammarStatus::Ok), "fixed");
    }

    /// Point placement `at` of the live record at the test record's box,
    /// as the Placements tab's Item combo does, and settle.
    fn retarget(app: &mut App, at: usize) {
        {
            let mut live = app.world_mut().resource_mut::<LiveRoomRecord>();
            let Placement::Absolute { generator_ref, .. } = &mut live.0.placements[at] else {
                panic!("placed absolutely");
            };
            *generator_ref = "box".to_string();
        }
        settle(app);
    }

    /// Give placement `at` of the live record `seed`, and settle.
    fn reseed(app: &mut App, at: usize, seed: Option<u64>) {
        {
            let mut live = app.world_mut().resource_mut::<LiveRoomRecord>();
            let Placement::Absolute { seed: given, .. } = &mut live.0.placements[at] else {
                panic!("placed absolutely");
            };
            *given = seed;
        }
        settle(app);
    }

    /// A room of [`coin_hut`] with its own seed `own` as "hut", placed
    /// once per entry of `seeds`, beside the test record's "box"; compiled,
    /// with the grammar statuses kept.
    fn huts(own: u64, seeds: &[Option<u64>]) -> App {
        let mut record = test_record(0);
        record.generators.insert("hut".to_string(), coin_hut(own));
        record.placements = seeds
            .iter()
            .enumerate()
            .map(|(slot, seed)| placed("hut", slot, *seed))
            .collect();
        let mut app = compile_app(record);
        app.init_resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>();
        settle(&mut app);
        app
    }

    /// The grammar status of the node filed under `key`.
    fn status_of(
        app: &App,
        key: &str,
    ) -> Option<crate::world_builder::grammar_diag::GrammarStatus> {
        app.world()
            .resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>()
            .get(key)
            .cloned()
    }

    /// The status of a hut drawn with `seed` by placement `at`, which draws
    /// nothing.
    fn drew_nothing(
        at: usize,
        seed: u64,
    ) -> Option<crate::world_builder::grammar_diag::GrammarStatus> {
        Some(crate::world_builder::grammar_diag::GrammarStatus::Error {
            message: format!(
                "with placement #{at}'s seed {seed}: \
                 grammar produced no geometry (no terminal shapes)"
            ),
        })
    }

    /// A seed with which [`coin_hut`] draws nothing, other than `not`.
    fn another_seed_drawing_nothing(not: u64) -> u64 {
        (1..256)
            .find(|&seed| {
                seed != not
                    && coin_hut(seed)
                        .kind
                        .shape_def()
                        .expect("a Shape node")
                        .derive()
                        .is_err()
            })
            .expect("a second seed that draws nothing")
    }

    /// #1505: a placement that drew the hut with a seed that draws nothing,
    /// pointed at another item, takes its error with it. The edit rebuilds
    /// that unit, as the box, and writes the hut's status anew from the
    /// placements as they are - where they are planned, not by building the
    /// hut's copy left again: Ok, since no placement draws the hut with a
    /// failing seed any more. The hut left was moved first, so its copy,
    /// built again by a compile that does not end in a GC, read the failing
    /// seed last: what it read must not outlive the placements it read.
    #[test]
    fn a_retargeted_failing_placement_leaves_no_error_behind() {
        let ([own, _], bad) = hut_seeds();
        let mut app = huts(own, &[Some(bad), None]);
        {
            let mut live = app.world_mut().resource_mut::<LiveRoomRecord>();
            let Placement::Absolute { transform, .. } = &mut live.0.placements[1] else {
                panic!("placed absolutely");
            };
            transform.translation.0[2] += 5.0;
        }
        settle(&mut app);
        assert_eq!(status_of(&app, "hut"), drew_nothing(0, bad), "fixture");
        let anchors = unit_anchors(&app);
        retarget(&mut app, 0);
        let now = unit_anchors(&app);
        assert_ne!(now[0], anchors[0], "unit 0 rebuilt as the box");
        assert_eq!(now[1], anchors[1], "the hut left is not built again");
        assert_eq!(
            status_of(&app, "hut"),
            Some(crate::world_builder::grammar_diag::GrammarStatus::Ok)
        );
    }

    /// #1505: of two placements drawing the hut with seeds that draw
    /// nothing, the one the status names - the copy compiled last - is
    /// pointed at the box: the status names the other, which still draws
    /// the hut so.
    #[test]
    fn a_retargeted_failing_placement_leaves_the_error_still_drawn() {
        let ([own, _], bad) = hut_seeds();
        let worse = another_seed_drawing_nothing(bad);
        let mut app = huts(own, &[Some(bad), Some(worse)]);
        assert_eq!(status_of(&app, "hut"), drew_nothing(1, worse), "fixture");
        retarget(&mut app, 1);
        assert_eq!(status_of(&app, "hut"), drew_nothing(0, bad));
    }

    /// #1505: a generator no placement places draws nothing, and has no
    /// status to show - not the error naming the seed of the placement that
    /// left it. The hut's only placement pointed at the box, and a yard's
    /// only placement removed: neither the hut's own status nor the one of
    /// the grammar under the yard's box is left.
    #[test]
    fn a_generator_no_placement_places_keeps_no_status() {
        let ([own, _], bad) = hut_seeds();
        let mut app = huts(own, &[Some(bad)]);
        assert_eq!(status_of(&app, "hut"), drew_nothing(0, bad), "fixture");
        retarget(&mut app, 0);
        assert_eq!(status_of(&app, "hut"), None, "pointed at the box");

        let mut yard = Generator::default_cuboid();
        yard.children.push(coin_hut(own));
        let mut record = test_record(1);
        record.generators.insert("yard".to_string(), yard);
        record.placements.push(placed("yard", 1, Some(bad)));
        let mut app = compile_app(record);
        app.init_resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>();
        settle(&mut app);
        assert_eq!(status_of(&app, "yard/0"), drew_nothing(1, bad), "fixture");
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements
            .remove(1);
        settle(&mut app);
        assert_eq!(status_of(&app, "yard/0"), None, "removed");
    }

    /// #1505: pointing a placement at another item rebuilds only that unit
    /// where the generator it placed before draws with every seed: the
    /// other copy of it is not built again, so it does not blink.
    #[test]
    fn a_retarget_off_a_generator_that_draws_rebuilds_nothing_else() {
        let mut app = compile_settled(towers(&[Some(3), None]));
        let anchors = unit_anchors(&app);
        retarget(&mut app, 0);
        let now = unit_anchors(&app);
        assert_ne!(now[0], anchors[0], "unit 0 rebuilt as the box");
        assert_eq!(now[1], anchors[1], "the tower left is not built again");
    }

    /// #1505: nor where the generator it placed before draws nothing with
    /// the seed that placement gave it. Its statuses are written anew where
    /// the placements are planned, from what the cache remembers, so no
    /// copy of it is built again to write them - here a scatter of the hut,
    /// its first copy left, which would blink out and fill back in over
    /// many frames to rewrite one line.
    #[test]
    fn a_retarget_off_a_failing_generator_builds_no_copy_left_again() {
        use crate::world_builder::grammar_diag::GrammarStatus;
        let ([own, _], bad) = hut_seeds();
        let mut record = test_record(0);
        record.generators.insert("hut".to_string(), coin_hut(own));
        record.placements = vec![scattered("hut", 500), placed("hut", 1, Some(bad))];
        let mut app = compile_app(record);
        app.init_resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>();
        settle(&mut app);
        assert_eq!(status_of(&app, "hut"), drew_nothing(1, bad), "fixture");
        let anchors = unit_anchors(&app);
        retarget(&mut app, 1);
        let now = unit_anchors(&app);
        assert_ne!(now[1], anchors[1], "fixture: unit 1 rebuilt as the box");
        assert_eq!(now[0], anchors[0], "the scatter left is not built again");
        assert_eq!(status_of(&app, "hut"), Some(GrammarStatus::Ok));
    }

    /// #1505: a placement's seed changed through many values - a person
    /// trying one variant after another - leaves no trail of them in the
    /// cache. Each change is an incremental compile, which runs no GC; the
    /// variant no placement draws any more is forgotten when the node is
    /// next derived.
    #[test]
    fn a_seed_changed_through_many_values_keeps_no_trail() {
        let mut app = compile_settled(towers(&[Some(3), None]));
        let unseeded = unit_anchors(&app)[1];
        for seed in 10..20 {
            reseed(&mut app, 0, Some(seed));
        }
        assert_eq!(
            unit_anchors(&app)[1],
            unseeded,
            "fixture: incremental compiles alone"
        );
        let mut held: Vec<(String, u64)> = shape_cache(&app)
            .builds
            .iter()
            .flat_map(|(node, seeds)| seeds.keys().map(move |seed| (node.clone(), *seed)))
            .collect();
        held.sort();
        let key = |seed: u64| ("tower".to_string(), seed);
        assert_eq!(held, [key(OWN_SEED), key(19)]);
    }

    /// A box holding `grammars` Shape nodes whose grammar draws nothing
    /// whatever its seed (#1505).
    fn barren(grammars: usize) -> Generator {
        let nothing: Generator = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.shape",
            "grammar_source": "Lot --> NIL",
            "root_rule": "Lot",
            "footprint": [20_000, 0, 20_000],
            "seed": "1",
        }))
        .expect("a Shape node");
        let mut root = Generator::default_cuboid();
        root.children = vec![nothing; grammars];
        root
    }

    /// #1505: a copy that draws nothing spawns nothing, so the room's
    /// entity budget does not bound what the cache remembers of such
    /// copies: 33 grammars that draw nothing, placed 32 times with 32
    /// seeds, are 1056 variants, of which the cache remembers
    /// `MAX_REMEMBERED_FAILURES` - and the status still shows the error.
    #[test]
    fn a_compile_remembers_so_many_failures_and_no_more() {
        use crate::world_builder::shape::MAX_REMEMBERED_FAILURES;
        let mut record = test_record(0);
        record.generators.insert("g".to_string(), barren(33));
        record.placements = (1..=32)
            .map(|seed| placed("g", seed, Some(seed as u64)))
            .collect();
        let mut app = compile_app(record);
        app.init_resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>();
        settle(&mut app);
        assert_eq!(remembered_failures(&app), MAX_REMEMBERED_FAILURES);
        assert!(
            matches!(
                status_of(&app, "g/0"),
                Some(crate::world_builder::grammar_diag::GrammarStatus::Error { .. })
            ),
            "{:?}",
            status_of(&app, "g/0")
        );
    }

    /// #1505: the compile's touch-set holds only the variants the cache
    /// holds, so it is bounded as the cache is, and not by the number of
    /// copies that draw nothing: one unit of more grammars that draw
    /// nothing than the cache remembers, with a scatter after it that keeps
    /// the job running past the first frame, leaves no more touched than
    /// remembered.
    #[test]
    fn the_touch_set_holds_only_what_the_cache_holds() {
        use crate::world_builder::shape::MAX_REMEMBERED_FAILURES;
        let mut record = test_record(0);
        record
            .generators
            .insert("g".to_string(), barren(MAX_REMEMBERED_FAILURES + 76));
        record.placements = vec![
            placed("g", 0, Some(7)),
            Placement::Scatter {
                generator_ref: "box".to_string(),
                bounds: ScatterBounds::Circle {
                    center: crate::pds::Fp2([0.0, 0.0]),
                    radius: Fp(50.0),
                },
                count: 100_000,
                local_seed: 7,
                biome_filter: Default::default(),
                snap_to_terrain: false,
                random_yaw: false,
                avoid_urban: false,
                float_on_water: false,
                naturalness: Default::default(),
            },
        ];
        let mut app = compile_app(record);
        app.update();
        assert_eq!(
            remembered_failures(&app),
            MAX_REMEMBERED_FAILURES,
            "fixture: more drew nothing than are remembered"
        );
        let job = app.world().resource::<CompileJob>();
        let active = job
            .0
            .as_ref()
            .expect("fixture: the scatter keeps the job running");
        assert!(
            active.touched.shape_mesh.len() <= MAX_REMEMBERED_FAILURES,
            "{} touched",
            active.touched.shape_mesh.len()
        );
    }

    /// #1505: a Shape node that never drew nothing writes its status without
    /// a look at any other node's failures - which a world of seeded copies
    /// can hold a thousand of - where it used to go through them all for
    /// every copy it drew. A hut whose seed draws nothing is its own to
    /// look at.
    #[test]
    fn a_node_that_never_failed_looks_at_no_failure() {
        let ([own, _], bad) = hut_seeds();
        let mut record = towers(&[Some(3), None, Some(4)]);
        record.generators.insert("hut".to_string(), coin_hut(own));
        record.placements.push(placed("hut", 3, Some(bad)));
        record.placements.push(placed("hut", 4, None));
        let mut app = compile_app(record);
        {
            let mut cache = app.world_mut().resource_mut::<ShapeMeshCache>();
            for node in 0..1000 {
                let failed = Err("grammar produced no geometry (no terminal shapes)".to_owned());
                assert!(cache.remember((format!("dead/{node}"), 1), 1, &failed));
            }
        }
        settle(&mut app);
        // The hut's copy that draws has its own one failure in view; the
        // towers, none.
        assert_eq!(shape_cache(&app).failures_in_view, 1);
    }

    /// Seeds with which [`coin_hut`] draws its block, `n` of them.
    fn seeds_that_draw(n: usize) -> Vec<u64> {
        let seeds: Vec<u64> = (1..1024)
            .filter(|&seed| {
                coin_hut(seed)
                    .kind
                    .shape_def()
                    .expect("a Shape node")
                    .derive()
                    .is_ok()
            })
            .take(n)
            .collect();
        assert_eq!(seeds.len(), n, "fixture: {n} seeds that draw");
        seeds
    }

    /// A scatter of `count` copies of `name` round the origin.
    fn scattered(name: &str, count: u32) -> Placement {
        Placement::Scatter {
            generator_ref: name.to_string(),
            bounds: ScatterBounds::Circle {
                center: crate::pds::Fp2([0.0, 0.0]),
                radius: Fp(50.0),
            },
            count,
            local_seed: 7,
            biome_filter: Default::default(),
            snap_to_terrain: false,
            random_yaw: false,
            avoid_urban: false,
            float_on_water: false,
            naturalness: Default::default(),
        }
    }

    /// #1505: nor does a node that did draw nothing with some seed go
    /// through the placements for every copy of it that draws. A scatter of
    /// 500 huts after ten copies placed with seeds that draw and one with a
    /// seed that draws nothing reads the placements to find that seed once,
    /// not once for each of its copies - at a thousand placements, that
    /// made a scatter of 100 000 take six times as long to build as with no
    /// seed failing. Every copy still writes the error.
    #[test]
    fn a_scatter_of_a_node_that_fails_reads_the_placements_once() {
        let ([own, _], bad) = hut_seeds();
        let mut record = test_record(0);
        record.generators.insert("hut".to_string(), coin_hut(own));
        record.placements = seeds_that_draw(10)
            .into_iter()
            .enumerate()
            .map(|(slot, seed)| placed("hut", slot, Some(seed)))
            .collect();
        record.placements.push(placed("hut", 10, Some(bad)));
        record.placements.push(scattered("hut", 500));
        let placements = record.placements.len();
        let mut app = compile_app(record);
        app.init_resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>();
        settle(&mut app);
        assert_eq!(status_of(&app, "hut"), drew_nothing(10, bad));
        let read = shape_cache(&app).placements_in_view;
        assert!(
            read <= placements,
            "{read} placements read, of {placements}"
        );
    }

    /// #1505: what a node's copies read of its failing seeds is read again
    /// once a failure is remembered. An edit that leaves the hut's grammar
    /// drawing as it did - a comment added - builds its copies again in
    /// order: the first reads no failure a seed of it still draws (the one
    /// remembered is of the grammar before the edit), and the one after the
    /// copy that fails must read the failure that copy has just left, and
    /// name it.
    #[test]
    fn a_failure_remembered_is_read_by_the_copies_after_it() {
        let ([own, other], bad) = hut_seeds();
        let mut app = huts(own, &[Some(own), Some(bad), Some(other)]);
        assert_eq!(status_of(&app, "hut"), drew_nothing(1, bad), "fixture");
        {
            let mut live = app.world_mut().resource_mut::<LiveRoomRecord>();
            let GeneratorKind::Shape { grammar_source, .. } =
                &mut live.0.generators.get_mut("hut").expect("the hut").kind
            else {
                panic!("the hut is a Shape node");
            };
            grammar_source.insert_str(0, "// the same grammar, edited\n");
        }
        let anchors = unit_anchors(&app);
        settle(&mut app);
        let now = unit_anchors(&app);
        assert!(
            (0..3).all(|at| now[at] != anchors[at]),
            "fixture: every copy built again"
        );
        assert_eq!(status_of(&app, "hut"), drew_nothing(1, bad));
    }
}
