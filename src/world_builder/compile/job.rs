//! State types for the incremental, time-sliced compile engine, plus
//! the per-placement fingerprint the diff planner keys on.
//!
//! The engine itself lives in [`super::compile_room_record`]: on every
//! record change it *plans* (diff the per-placement fingerprints
//! against [`CompiledWorld`], despawn the stale units, queue the
//! changed indices) and then *executes* the queue a few milliseconds
//! per frame ([`SLICE_BUDGET`]), resuming mid-scatter / mid-grid via
//! [`UnitCursor`]. Two properties fall out of that split:
//!
//! - **Incrementality** - an edit that touches one generator rebuilds
//!   only the placements referencing it; environment-only edits queue
//!   nothing at all.
//! - **Bounded stalls** - even a full rebuild (first load, heightmap
//!   change) spreads its spawning over frames instead of freezing the
//!   wasm main thread for the whole world.
//!
//! Determinism is unaffected: units are queued in ascending placement
//! order (preserving the authored water-before-scatter convention) and
//! every cursor carries its RNG, so a sliced build is byte-identical
//! to a monolithic one.

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;

use crate::pds::{Placement, RoomRecord};

/// Per-frame wall-clock budget for the executor. ~5 ms leaves room for
/// the rest of the frame at 30 FPS even on the single-threaded wasm
/// build, while a typical seeded room still compiles in a handful of
/// frames. The budget bounds *slices*, not units: a single placement
/// whose blueprint is one enormous derivation (first uncached L-system
/// bake) remains atomic and can overshoot.
pub(super) const SLICE_BUDGET: Duration = Duration::from_millis(5);

/// What one placement compiled to, as far as the planner cares: the
/// fingerprint it was built from and the anchor entity that roots its
/// spawned tree.
#[derive(Default)]
pub(super) struct CompiledUnit {
    /// `None` when the unit has never compiled (or was invalidated and
    /// not yet rebuilt) - always re-queued by the next plan.
    pub(super) fingerprint: Option<String>,
    /// The unit's `PlacementMarker` anchor. `None` for
    /// `Placement::Unknown` (which spawns nothing) and for
    /// not-yet-rebuilt units.
    pub(super) anchor: Option<Entity>,
}

/// Per-placement compiled state for the active room. The diff planner
/// compares fresh fingerprints against this; the executor commits each
/// unit here as it completes.
///
/// Reset by `ui::logout::cleanup_on_logout` and the attract-scene teardown
/// (both despawn every `RoomEntity` outside the planner, so an identical
/// record afterwards must compile from scratch). A placements-length
/// *shrink* resets it wholesale - indices are unit identity, and
/// `PlacementMarker` values on surviving anchors would go stale under a
/// removal shift. A length *growth* only extends it (#979): live-record
/// mutations append, so surviving indices keep their placements and the
/// diff rebuilds nothing but the new tail.
#[derive(Resource, Default)]
pub struct CompiledWorld {
    pub(super) units: Vec<CompiledUnit>,
    /// The generator each placement placed when the world was last
    /// planned, by index (#1505): what a placement pointed at another
    /// generator placed before, and which generators the room placed.
    pub(super) placed: Vec<Option<String>>,
}

/// The in-flight sliced compile job, if any. At most one exists; a
/// record change while a job is active re-plans the queue against the
/// units committed so far (the in-progress unit is aborted and
/// re-queued by the diff, since its fingerprint was never committed).
#[derive(Resource, Default)]
pub struct CompileJob(pub(super) Option<ActiveJob>);

impl CompileJob {
    /// How far the in-flight compile has got: `(units built, units total)`,
    /// or `None` when no job is running (#1230 f281).
    ///
    /// The loading screen's longest and least legible phase was a bare
    /// spinner with a warning that it "may pause a few seconds", although
    /// the job has counted its own work since #351. No new bookkeeping: the
    /// numbers are `units_built`, the queue length, and the unit currently
    /// mid-build. What was actually missing is this accessor - every field
    /// of [`ActiveJob`] is `pub(super)`, so a `Res<CompileJob>` parameter on
    /// its own could read nothing.
    ///
    /// `total` is the size of THIS job, not of the world: an incremental
    /// re-plan rebuilds only what changed, and reporting "3 of 480" for a
    /// three-unit diff would be a worse lie than the spinner.
    pub fn progress(&self) -> Option<(u32, u32)> {
        let job = self.0.as_ref()?;
        let remaining = job.queue.len() as u32 + job.cursor.is_some() as u32;
        Some((job.units_built, job.units_built + remaining))
    }

    /// A job running, with nothing left to build: for a test of what waits
    /// on a compile.
    #[cfg(test)]
    pub(crate) fn running() -> Self {
        CompileJob(Some(ActiveJob::new(
            VecDeque::new(),
            true,
            Default::default(),
        )))
    }
}

/// One placement waiting to be (re)built, with the fingerprint the
/// rebuild will be committed under.
pub(super) struct QueuedUnit {
    pub(super) index: usize,
    pub(super) fingerprint: Option<String>,
}

/// Cache touch-sets accumulated across every slice of one job. Only a
/// job with full coverage may GC against them - an incremental job
/// touches only the rebuilt units' keys, and evicting everything else
/// would orphan the untouched world's mesh/material handles.
#[derive(Default)]
pub(super) struct TouchSets {
    pub(super) lsystem_material: HashSet<(String, u16)>,
    pub(super) lsystem_mesh: HashSet<String>,
    pub(super) shape_material: HashSet<(String, String)>,
    /// `(node key, seed)`: one per variant of a Shape node drawn (#1505).
    pub(super) shape_mesh: HashSet<(String, u64)>,
    /// Content-hash keys of the primitive caches (#919). Unlike the sets
    /// above these are not generator refs - the prim caches are keyed by
    /// content so one entry can serve many generators - but the GC
    /// argument is identical: a full job touches every key the live world
    /// needs, so anything untouched is unreachable.
    pub(super) prim_mesh: HashSet<u64>,
    pub(super) prim_material: HashSet<u64>,
}

pub(super) struct ActiveJob {
    /// Unit indices still to build, ascending (preserves the authored
    /// placement order - water registered before the scatters that
    /// sample it, matching the monolithic pass).
    pub(super) queue: VecDeque<QueuedUnit>,
    /// Resume state for the unit currently mid-build, when its grid /
    /// scatter loop outlived the previous slice.
    pub(super) cursor: Option<UnitCursor>,
    /// `true` when this job (re)builds every placement - the only case
    /// where the end-of-job cache GC is sound, and the case the loading
    /// gate's first pass always hits.
    pub(super) full: bool,
    pub(super) touched: TouchSets,
    /// Multiplicative spawn budget across the whole job (mirrors the
    /// monolithic pass's per-pass cap).
    pub(super) entities_spawned: u32,
    pub(super) budget_warned: bool,
    /// Units abandoned when the budget tripped (#1211): how many, and the
    /// first index - the owner's clue to which rows are missing.
    pub(super) skipped_units: u32,
    pub(super) skipped_from: Option<usize>,
    /// The room's water at plan time (`drawn_water`: its sea and lakes,
    /// #1727), for `start_unit`'s dry-land walk. Cached on the job so the
    /// execute slices don't re-scan every generator each frame (#673); a
    /// replan recomputes it, and the record cannot change between plans
    /// without triggering one.
    pub(super) room_water: crate::water::bodies::RoomWater,
    /// What every scatter keeps off (#1776,
    /// [`structure_exclusions`](super::scatter::structure_exclusions)),
    /// walked once a plan when its first scatter starts: a replan - which a
    /// new heightmap or record always brings - clears it.
    pub(super) keep_off: Option<KeepOff>,
    /// The scattered copy being spawned and each generator's size class,
    /// measured on its first copy this job (#1480) - see
    /// [`CopyRecorder`](super::super::draw_distance::CopyRecorder).
    pub(super) copy: super::super::draw_distance::CopyRecorder,
    // --- telemetry (#351) ---
    pub(super) work: Duration,
    pub(super) frames: u32,
    pub(super) units_built: u32,
}

impl ActiveJob {
    pub(super) fn new(
        queue: VecDeque<QueuedUnit>,
        full: bool,
        room_water: crate::water::bodies::RoomWater,
    ) -> Self {
        Self {
            queue,
            cursor: None,
            full,
            touched: TouchSets::default(),
            entities_spawned: 0,
            budget_warned: false,
            skipped_units: 0,
            skipped_from: None,
            room_water,
            keep_off: None,
            copy: Default::default(),
            work: Duration::ZERO,
            frames: 0,
            units_built: 0,
        }
    }
}

/// `(centre_x, centre_z, radius²)` of the ground every scatter keeps off
/// ([`structure_exclusions`](super::scatter::structure_exclusions), #1776),
/// shared by every scatter of a plan.
pub(super) type KeepOff = std::sync::Arc<[(f32, f32, f32)]>;

/// Resume point inside a multi-cell unit. The anchor and its resolved
/// world transform are computed once at unit start; the kind carries
/// the loop state (including the RNG, so the sample stream across
/// slices is byte-identical to an unsliced run).
pub(super) struct UnitCursor {
    pub(super) index: usize,
    /// Fingerprint to commit when the unit completes (from the queue).
    pub(super) fingerprint: Option<String>,
    pub(super) anchor: Entity,
    pub(super) anchor_world_tf: Transform,
    pub(super) snap: bool,
    pub(super) kind: CursorKind,
}

pub(super) enum CursorKind {
    Grid {
        /// Linearised next cell: `((ix * cy) + iy) * cz + iz`, matching
        /// the monolithic loop's iteration order.
        next_cell: u64,
        /// Present when `random_yaw` - seeded once at unit start.
        rng: Option<ChaCha8Rng>,
    },
    Scatter {
        spawned: u32,
        attempts: u32,
        /// Positions only (#912). Every per-instance decoration moved to
        /// `jitter_rng`, which is what makes the naturalness filters
        /// purely subtractive - see `super::scatter`.
        rng: ChaCha8Rng,
        /// Per-instance scale / tilt / yaw stream, seeded off the same
        /// `local_seed` but salted apart from `rng`. Boxed because a
        /// second inline `ChaCha8Rng` would make this variant twice the
        /// size of `Grid` and inflate every cursor.
        jitter_rng: Box<ChaCha8Rng>,
        /// Cluster seeds for `ScatterNaturalness::clumping`, derived once
        /// at unit start from `local_seed`. Always populated, so enabling
        /// clumping changes only the contraction.
        clusters: Vec<(f32, f32)>,
        /// The water the samples are judged against, read from the planes
        /// spawned when the scatter started, matching the monolithic pass
        /// ([`crate::water::ScatterWater`]): the level over its centre, and
        /// a lake's at each sample its reach takes in (#1727).
        water: crate::water::ScatterWater,
        /// The seeded footprints and the landing it keeps off, where the
        /// compile stands them ([`ActiveJob::keep_off`], #1776).
        keep_off: KeepOff,
    },
}

/// Outcome of one executor step on the current unit.
pub(super) enum StepOutcome {
    /// Budget expired mid-unit; cursor updated, resume next slice.
    Yielded,
    /// The unit finished; commit it.
    Done,
}

/// `generator_ref` of a placement, if it has one.
pub(super) fn placement_generator_ref(placement: &Placement) -> Option<&str> {
    placement.generator_ref()
}

/// Serialise everything that determines one placement's compiled
/// output into a stable string:
///
/// - the placement itself,
/// - the generator tree it references (children are embedded, so the
///   whole blueprint is covered) and that generator's `traits` entry,
/// - the room's water for `avoid_water` placements (the dry-land walk
///   asks it the level at each probe),
/// - the terrain config + the room's water for biome-filtered scatters
///   (`dominant_biome` reads the terrain rules; the filter's water
///   relation reads the registry at each sample - the room's water, its
///   sea and its lakes, is a sound proxy for the planes those filters
///   target in practice; a scatter near a *moved scattered pond* can go
///   stale until the next full rebuild, which heightmap edits and
///   placement-count changes both force).
///
/// Once-per-planning-pass fingerprint inputs that are pure functions of
/// the whole record: the room's water, the terrain config and what every
/// scatter keeps off (#1776), all serialised up front. The water is the one drawn - its sea's level, and
/// its bodies' levels and cells (`RoomWater::key`, #1727): a lake's, or
/// Berlin's own in a region built from real Berlin (#1724) - and the
/// bodies' cells and Berlin's land use, which a scatter's biome filter reads
/// there instead of the terrain rules, change only with the heightmap,
/// whose every swap is a full rebuild. Before this
/// existed, every `unit_fingerprint` call re-scanned all generators
/// (`room_water_level` + `find_terrain_config`) - O(placements ×
/// generators) per pass in the editing loop (#673). Scoped to a single pass ONLY: the fingerprint is
/// the planner's change-detection source of truth, so caching these
/// across passes would be a correctness trap.
///
/// A `None` field means its serialisation failed; the consuming
/// fingerprint arm then returns `None` ("always rebuild"), matching the
/// previous per-call behaviour.
pub(super) struct FingerprintPass {
    water_level: Option<serde_json::Value>,
    terrain: Option<serde_json::Value>,
    /// What every scatter keeps off (#1776): each seeded structure's place
    /// and ground radius, and the world's landing with its facing -
    /// `Value::Null` for a world with neither.
    kept_off: Option<serde_json::Value>,
}

impl FingerprintPass {
    pub(super) fn new(record: &RoomRecord, room_water: &crate::water::bodies::RoomWater) -> Self {
        // Only what the keep-outs read: where each seeded structure's record
        // stands it, how wide its ground, and whether it is the landmark a
        // seeded landing looks at (#1777) - and the gate a walked Berlin
        // landing turns to, the signature feature (#1781) and the crafts'
        // places (#1782), water-avoiding or not, snapped or not - not its
        // generator, seed or params, whose edits leave every scatter as it
        // was.
        let structures: Vec<[f32; 7]> = record
            .placements
            .iter()
            .filter_map(|placement| match placement {
                Placement::Absolute {
                    generator_ref,
                    transform,
                    snap_to_terrain,
                    avoid_water,
                    avoid_water_clearance,
                    ..
                } if *snap_to_terrain
                    && (*avoid_water || generator_ref == super::water::GATEWAY)
                    || super::scatter::KEPT_BY_NAME.contains(&generator_ref.as_str()) =>
                {
                    Some([
                        transform.translation.0[0],
                        transform.translation.0[2],
                        avoid_water_clearance.0,
                        transform.scale.0[0],
                        f32::from(u8::from(generator_ref == super::scatter::LANDMARK)),
                        f32::from(u8::from(*avoid_water)),
                        f32::from(u8::from(*snap_to_terrain)),
                    ])
                }
                _ => None,
            })
            .collect();
        // The paths every scatter keeps off (#1779): their lines and width,
        // and the tracks (#1782), each at its own.
        let paths = crate::pds::find_terrain_config(record)
            .map(|terrain| &terrain.paths)
            .filter(|paths| !paths.is_empty())
            .map(|paths| (&paths.lines, paths.width, &paths.tracks));
        Self {
            water_level: Some(room_water.key()),
            // The land a scatter samples, not its grass (#1700).
            terrain: serde_json::to_value(
                crate::pds::find_terrain_config(record).map(|terrain| terrain.land()),
            )
            .ok(),
            kept_off: if structures.is_empty()
                && record.default_landing.is_none()
                && paths.is_none()
            {
                Some(serde_json::Value::Null)
            } else {
                // The pitch reaches the camera's ground back, and a landing
                // that avoids water keeps its view clear (#1777).
                let landing = record.default_landing.as_ref().map(|landing| {
                    [
                        landing.pos.0[0],
                        landing.pos.0[1],
                        landing.yaw_deg.0,
                        landing.pitch_deg.map_or(f32::NAN, |pitch| pitch.0),
                        f32::from(u8::from(landing.avoid_water)),
                    ]
                });
                serde_json::to_value((&structures, landing, paths)).ok()
            },
        }
    }
}

/// Routed through `serde_json::to_value` so any `HashMap`-backed field
/// serialises key-sorted (`Value::Object` is BTreeMap-backed; the
/// `preserve_order` feature is off). `None` on serialisation failure -
/// the planner treats that as "always rebuild".
pub(super) fn unit_fingerprint(
    record: &RoomRecord,
    placement: &Placement,
    pass: &FingerprintPass,
) -> Option<String> {
    let generator_ref = placement_generator_ref(placement);
    let generator = generator_ref.and_then(|r| record.generators.get(r));
    let traits_entry = generator_ref.and_then(|r| record.traits.get(r));

    let mut extras = serde_json::Map::new();
    match placement {
        Placement::Absolute {
            avoid_water: true, ..
        } => {
            extras.insert("water_level".into(), pass.water_level.clone()?);
        }
        // The biome allow-list, the slope cutoff and the microbiome bands
        // all resolve against the terrain generator and/or the water line,
        // so any of them makes the compiled output depend on both.
        Placement::Scatter {
            biome_filter,
            naturalness,
            ..
        } => {
            if !biome_filter.is_noop()
                || naturalness.max_slope_deg.is_some()
                || naturalness.above_water_band.is_some()
                || naturalness.altitude_band.is_some()
            {
                extras.insert("water_level".into(), pass.water_level.clone()?);
                extras.insert("terrain".into(), pass.terrain.clone()?);
            }
            // Every scatter keeps off the seeded structures, where the
            // water walks them, and off the landing (#1776).
            let kept_off = pass.kept_off.clone()?;
            if !kept_off.is_null() {
                extras.insert("water_level".into(), pass.water_level.clone()?);
                extras.insert("kept_off".into(), kept_off);
            }
        }
        _ => {}
    }

    let mut placement_value = serde_json::to_value(placement).ok()?;
    // An instance set's copies are followed by its build in place (#1644):
    // what they are is no part of the unit the compile builds.
    if matches!(placement, Placement::Instances { .. })
        && let Some(fields) = placement_value.as_object_mut()
    {
        fields.remove("copies");
    }
    // A terrain's grass (#1700) is drawn by the grass's and the ground's
    // shaders from the live record: no unit builds it, so the terrain's own
    // unit - its anchor and everything riding on it - is keyed on its land.
    let generator_value = match generator {
        Some(
            generator @ crate::pds::Generator {
                kind: crate::pds::GeneratorKind::Terrain(terrain),
                ..
            },
        ) => {
            let mut land = generator.clone();
            land.kind = crate::pds::GeneratorKind::Terrain(terrain.land());
            serde_json::to_value(&land).ok()?
        }
        _ => serde_json::to_value(generator).ok()?,
    };
    let v = serde_json::json!({
        "placement": placement_value,
        "generator": generator_value,
        "traits": serde_json::to_value(traits_entry).ok()?,
        "extras": extras,
    });
    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    //! The fingerprint is the planner's entire decision input, so it
    //! carries the unit coverage; the executor's ECS flow is exercised
    //! by the existing integration suite plus manual smoke tests.
    use super::{ActiveJob, CompileJob, FingerprintPass, QueuedUnit, unit_fingerprint};
    use crate::pds::{Fp, RoomRecord};
    use std::collections::VecDeque;

    /// #1230 f281. The loading screen's longest and least legible phase was
    /// a numberless spinner under a warning that it "may pause a few
    /// seconds" - although the job has counted `units_built` since #351.
    /// The finding's refuter was right that the screen could not read them:
    /// every field of `ActiveJob` is `pub(super)`, so a `Res<CompileJob>`
    /// parameter alone would have read nothing. This is the accessor that
    /// was missing.
    #[test]
    fn a_running_compile_can_say_how_far_it_has_got() {
        assert_eq!(
            CompileJob::default().progress(),
            None,
            "no job is not zero progress"
        );

        let queue: VecDeque<QueuedUnit> = (0..3)
            .map(|index| QueuedUnit {
                index,
                fingerprint: None,
            })
            .collect();
        let mut job = ActiveJob::new(queue, true, Default::default());
        job.units_built = 2;
        let running = CompileJob(Some(job));
        assert_eq!(
            running.progress(),
            Some((2, 5)),
            "two built plus three queued is five units in THIS job"
        );
    }

    /// The last slice: an empty queue with everything built reports a
    /// finished ratio rather than a division by nothing, and the row's own
    /// `total > 0` filter keeps a not-yet-planned job on the bare spinner.
    #[test]
    fn a_job_with_nothing_queued_reports_a_whole_ratio() {
        let mut job = ActiveJob::new(VecDeque::new(), true, Default::default());
        job.units_built = 7;
        assert_eq!(CompileJob(Some(job)).progress(), Some((7, 7)));
        let empty = ActiveJob::new(VecDeque::new(), true, Default::default());
        assert_eq!(
            CompileJob(Some(empty)).progress(),
            Some((0, 0)),
            "a job that has planned nothing yet counts nothing"
        );
    }

    fn fingerprints(record: &RoomRecord) -> Vec<Option<String>> {
        // Same once-per-pass construction the planner uses.
        let pass = FingerprintPass::new(
            record,
            &crate::water::bodies::RoomWater::sea_alone(super::super::water::room_water_level(
                record,
            )),
        );
        record
            .placements
            .iter()
            .map(|p| unit_fingerprint(record, p, &pass))
            .collect()
    }

    #[test]
    fn fingerprints_are_stable_across_serialisations() {
        let record = RoomRecord::default_for_did("did:plc:fp_test");
        assert!(!record.placements.is_empty());
        let a = fingerprints(&record);
        let b = fingerprints(&record);
        assert_eq!(a, b);
        assert!(a.iter().all(|fp| fp.is_some()));
    }

    /// A grass edit (#1700) - every setting of the terrain's grass group -
    /// recompiles no unit: not the terrain's own, its water and props
    /// riding on its anchor, nor a scatter laid on its land. The grass's
    /// and the ground's shaders draw it from the live record.
    #[test]
    fn grass_edits_change_no_unit() {
        use crate::pds::{Fp3, GeneratorKind, SovereignGrassConfig};
        let mut record = RoomRecord::default_for_did("did:plc:fp_test");
        let before = fingerprints(&record);
        let terrain = record
            .generators
            .values_mut()
            .find_map(|generator| match &mut generator.kind {
                GeneratorKind::Terrain(terrain) => Some(terrain),
                _ => None,
            })
            .expect("the seeded world has a terrain");
        terrain.grass = SovereignGrassConfig {
            color_base: Some(Fp3([0.2, 0.15, 0.05])),
            dry_share: Some(Fp(0.7)),
            flower_share: Some(Fp(0.4)),
            layer_density: Some([Fp(1.0); 4]),
            thickness: Some(Fp(0.5)),
            sway: Some(Fp(1.5)),
            ..SovereignGrassConfig::default()
        };
        assert_eq!(fingerprints(&record), before);
    }

    /// #1777: every scatter is rebuilt when the gate a walked Berlin landing
    /// turns to moves - water-avoiding or not - and when the landmark a
    /// seeded landing looks at is another placement: both decide which way
    /// the arrival camera stands, and the ground behind it is kept off.
    #[test]
    fn the_gate_and_the_landmark_reach_every_scatters_fingerprint() {
        use crate::pds::Placement;
        // On its own ground, where a seeded world has a landmark.
        let mut record =
            crate::seeded_defaults::room::build::build_room_with_source(0, "did:plc:fp_test", None);
        let scatter = record
            .placements
            .iter()
            .position(|p| matches!(p, Placement::Scatter { .. }))
            .expect("a seeded scatter");
        let named = |name: &'static str| move |p: &Placement| matches!(p, Placement::Absolute { generator_ref, .. } if generator_ref == name);
        let gate = record
            .placements
            .iter()
            .position(named(super::super::water::GATEWAY))
            .expect("a seeded gate");
        let before = fingerprints(&record)[scatter].clone();
        // Made the owner's own - walked off nothing - the gate still counts.
        if let Placement::Absolute { avoid_water, .. } = &mut record.placements[gate] {
            *avoid_water = false;
        }
        let owned = fingerprints(&record)[scatter].clone();
        assert_ne!(owned, before);
        if let Placement::Absolute { transform, .. } = &mut record.placements[gate] {
            transform.translation.0[0] += 5.0;
        }
        assert_ne!(fingerprints(&record)[scatter], owned, "a moved gate");
        let moved = fingerprints(&record)[scatter].clone();
        if let Some(Placement::Absolute { generator_ref, .. }) = record
            .placements
            .iter_mut()
            .find(|p| named(super::super::scatter::LANDMARK)(p))
        {
            *generator_ref = "landmark_copy".into();
        }
        assert_ne!(fingerprints(&record)[scatter], moved, "another landmark");
    }

    /// #1779: every scatter is rebuilt when a world's paths change, which
    /// it keeps off.
    #[test]
    fn a_path_edit_reaches_every_scatters_fingerprint() {
        use crate::pds::{Fp2, GeneratorKind, Placement};
        let mut record =
            crate::seeded_defaults::room::build::build_room_with_source(0, "did:plc:fp_test", None);
        let scatter = record
            .placements
            .iter()
            .position(|p| matches!(p, Placement::Scatter { .. }))
            .expect("a seeded scatter");
        let before = fingerprints(&record)[scatter].clone();
        for generator in record.generators.values_mut() {
            if let GeneratorKind::Terrain(terrain) = &mut generator.kind {
                terrain
                    .paths
                    .lines
                    .push(vec![Fp2([1.0, 2.0]), Fp2([30.0, 2.0])]);
            }
        }
        assert_ne!(fingerprints(&record)[scatter], before);
    }

    #[test]
    fn environment_edits_change_no_unit() {
        let mut record = RoomRecord::default_for_did("did:plc:fp_test");
        let before = fingerprints(&record);
        // The most-dragged editor sliders: sky and fog. Neither feeds
        // any placement's compiled output.
        record.environment.sky_color.0 = [0.1, 0.2, 0.3];
        record.environment.fog_visibility = Fp(123.0);
        assert_eq!(fingerprints(&record), before);
    }

    #[test]
    fn generator_edit_changes_only_referencing_units() {
        let mut record = RoomRecord::default_for_did("did:plc:fp_test");
        let before = fingerprints(&record);

        // Pick the generator referenced by the first placement and
        // perturb it; only placements referencing it may change.
        let target_ref = super::placement_generator_ref(&record.placements[0])
            .expect("seeded placements carry a generator_ref")
            .to_string();
        let g = record
            .generators
            .get_mut(&target_ref)
            .expect("referenced generator exists");
        g.transform.translation.0[1] += 1.0;

        let after = fingerprints(&record);
        for (i, placement) in record.placements.iter().enumerate() {
            let references_target =
                super::placement_generator_ref(placement) == Some(target_ref.as_str());
            if references_target {
                assert_ne!(after[i], before[i], "unit {i} references the edit");
            } else {
                assert_eq!(after[i], before[i], "unit {i} must be untouched");
            }
        }
    }

    #[test]
    fn traits_edit_changes_only_the_keyed_units() {
        let mut record = RoomRecord::default_for_did("did:plc:fp_test");
        let target_ref = super::placement_generator_ref(&record.placements[0])
            .unwrap()
            .to_string();
        let before = fingerprints(&record);
        // Insert a fresh traits entry for the first placement's ref. An
        // empty list still differs from "no entry" (`[]` vs `null` in
        // the serialised form), which is exactly the edit shape the
        // traits editor produces when a row is first added.
        record.traits.insert(target_ref.clone(), Vec::new());
        let after = fingerprints(&record);
        for (i, placement) in record.placements.iter().enumerate() {
            let keyed = super::placement_generator_ref(placement) == Some(target_ref.as_str());
            if keyed {
                assert_ne!(after[i], before[i]);
            } else {
                assert_eq!(after[i], before[i]);
            }
        }
    }
}
