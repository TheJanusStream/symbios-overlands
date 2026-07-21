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
//! - **Incrementality** — an edit that touches one generator rebuilds
//!   only the placements referencing it; environment-only edits queue
//!   nothing at all.
//! - **Bounded stalls** — even a full rebuild (first load, heightmap
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
    /// not yet rebuilt) — always re-queued by the next plan.
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
/// Reset by `logout::cleanup_on_logout` (the teardown despawns every
/// `RoomEntity`, so an identical record next login must compile from
/// scratch). A placements-length change resets it wholesale — indices
/// are unit identity, and `PlacementMarker` values on surviving anchors
/// would go stale under an insert/remove shift.
#[derive(Resource, Default)]
pub struct CompiledWorld {
    pub(super) units: Vec<CompiledUnit>,
}

/// The in-flight sliced compile job, if any. At most one exists; a
/// record change while a job is active re-plans the queue against the
/// units committed so far (the in-progress unit is aborted and
/// re-queued by the diff, since its fingerprint was never committed).
#[derive(Resource, Default)]
pub struct CompileJob(pub(super) Option<ActiveJob>);

/// One placement waiting to be (re)built, with the fingerprint the
/// rebuild will be committed under.
pub(super) struct QueuedUnit {
    pub(super) index: usize,
    pub(super) fingerprint: Option<String>,
}

/// Cache touch-sets accumulated across every slice of one job. Only a
/// job with full coverage may GC against them — an incremental job
/// touches only the rebuilt units' keys, and evicting everything else
/// would orphan the untouched world's mesh/material handles.
#[derive(Default)]
pub(super) struct TouchSets {
    pub(super) lsystem_material: HashSet<(String, u16)>,
    pub(super) lsystem_mesh: HashSet<String>,
    pub(super) shape_material: HashSet<(String, String)>,
    pub(super) shape_mesh: HashSet<String>,
    /// Content-hash keys of the primitive caches (#919). Unlike the sets
    /// above these are not generator refs — the prim caches are keyed by
    /// content so one entry can serve many generators — but the GC
    /// argument is identical: a full job touches every key the live world
    /// needs, so anything untouched is unreachable.
    pub(super) prim_mesh: HashSet<u64>,
    pub(super) prim_material: HashSet<u64>,
}

pub(super) struct ActiveJob {
    /// Unit indices still to build, ascending (preserves the authored
    /// placement order — water registered before the scatters that
    /// sample it, matching the monolithic pass).
    pub(super) queue: VecDeque<QueuedUnit>,
    /// Resume state for the unit currently mid-build, when its grid /
    /// scatter loop outlived the previous slice.
    pub(super) cursor: Option<UnitCursor>,
    /// `true` when this job (re)builds every placement — the only case
    /// where the end-of-job cache GC is sound, and the case the loading
    /// gate's first pass always hits.
    pub(super) full: bool,
    pub(super) touched: TouchSets,
    /// Multiplicative spawn budget across the whole job (mirrors the
    /// monolithic pass's per-pass cap).
    pub(super) entities_spawned: u32,
    pub(super) budget_warned: bool,
    /// The room's water level at plan time, for `start_unit`'s dry-land
    /// walk. Cached on the job so the execute slices don't re-scan every
    /// generator each frame (#673); a replan recomputes it, and the record
    /// cannot change between plans without triggering one.
    pub(super) room_water_y: Option<f32>,
    // --- telemetry (#351) ---
    pub(super) work: Duration,
    pub(super) frames: u32,
    pub(super) units_built: u32,
}

impl ActiveJob {
    pub(super) fn new(queue: VecDeque<QueuedUnit>, full: bool, room_water_y: Option<f32>) -> Self {
        Self {
            queue,
            cursor: None,
            full,
            touched: TouchSets::default(),
            entities_spawned: 0,
            budget_warned: false,
            room_water_y,
            work: Duration::ZERO,
            frames: 0,
            units_built: 0,
        }
    }
}

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
        /// Present when `random_yaw` — seeded once at unit start.
        rng: Option<ChaCha8Rng>,
    },
    Scatter {
        spawned: u32,
        attempts: u32,
        /// Positions only (#912). Every per-instance decoration moved to
        /// `jitter_rng`, which is what makes the naturalness filters
        /// purely subtractive — see `super::scatter`.
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
        /// Water level sampled from the registry at scatter start —
        /// once per unit, matching the monolithic pass.
        water_level: Option<f32>,
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
    match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Grid { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. } => Some(generator_ref),
        Placement::Unknown => None,
    }
}

/// Serialise everything that determines one placement's compiled
/// output into a stable string:
///
/// - the placement itself,
/// - the generator tree it references (children are embedded, so the
///   whole blueprint is covered) and that generator's `traits` entry,
/// - the room water level for `avoid_water` placements (the dry-land
///   walk samples it),
/// - the terrain config + room water level for biome-filtered scatters
///   (`dominant_biome` reads the terrain rules; the filter's water
///   relation reads the registry — the room level is a sound proxy for
///   the home-water plane those filters target in practice; a
///   scatter near a *moved scattered pond* can go stale until the next
///   full rebuild, which heightmap edits and placement-count changes
///   both force).
///
/// Once-per-planning-pass fingerprint inputs that are pure functions of
/// the whole record: the room water level and the terrain config, both
/// serialised up front. Before this existed, every `unit_fingerprint`
/// call re-scanned all generators (`room_water_level` +
/// `find_terrain_config`) — O(placements × generators) per pass in the
/// editing loop (#673). Scoped to a single pass ONLY: the fingerprint is
/// the planner's change-detection source of truth, so caching these
/// across passes would be a correctness trap.
///
/// A `None` field means its serialisation failed; the consuming
/// fingerprint arm then returns `None` ("always rebuild"), matching the
/// previous per-call behaviour.
pub(super) struct FingerprintPass {
    water_level: Option<serde_json::Value>,
    terrain: Option<serde_json::Value>,
}

impl FingerprintPass {
    pub(super) fn new(record: &RoomRecord, room_water_y: Option<f32>) -> Self {
        Self {
            water_level: serde_json::to_value(room_water_y).ok(),
            terrain: serde_json::to_value(crate::pds::find_terrain_config(record)).ok(),
        }
    }
}

/// Routed through `serde_json::to_value` so any `HashMap`-backed field
/// serialises key-sorted (`Value::Object` is BTreeMap-backed; the
/// `preserve_order` feature is off). `None` on serialisation failure —
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
        // Both the biome allow-list and the naturalness slope cutoff
        // resolve against the terrain generator, so either one makes the
        // compiled output depend on it.
        Placement::Scatter {
            biome_filter,
            naturalness,
            ..
        } if !biome_filter.is_noop() || naturalness.max_slope_deg.is_some() => {
            extras.insert("water_level".into(), pass.water_level.clone()?);
            extras.insert("terrain".into(), pass.terrain.clone()?);
        }
        _ => {}
    }

    let v = serde_json::json!({
        "placement": serde_json::to_value(placement).ok()?,
        "generator": serde_json::to_value(generator).ok()?,
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
    use super::{FingerprintPass, unit_fingerprint};
    use crate::pds::{Fp, RoomRecord};

    fn fingerprints(record: &RoomRecord) -> Vec<Option<String>> {
        // Same once-per-pass construction the planner uses.
        let pass = FingerprintPass::new(record, super::super::water::room_water_level(record));
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
