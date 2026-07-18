//! Load-time lot-based building population.
//!
//! Seeded rooms grow no road network (too heavy for a good default room on
//! wasm), so this layer only serves rooms whose author added a `RoadNetwork`
//! generator in the editor (or saved one back when roads were seeded). Once
//! the heightmap exists, this system extracts the road network's enclosed
//! building lots ([`crate::urban::extract_building_lots`]) and injects themed
//! catalogue buildings onto them — straight into the live record, so they
//! compile to entities like any authored placement and the author can save
//! them.
//!
//! Everything is deterministic in the room DID + the network's layout seed: the
//! terrain (and thus the lots) reproduce on every peer, the building picks come
//! from a seeded stream, so every peer that derives the record lands identical
//! buildings even before anyone saves. The buildings are named
//! `lot_building_{seed}_{i}`; that seed-tagged prefix is the idempotency key —
//! a re-roll (new seed) strips the stale set and repopulates, an unchanged
//! layout is left alone, and turning the layer off sweeps them.

use std::collections::HashMap;

use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::catalogue::{CatalogueEntry, StructureRole, entries_for, entries_for_room};
use crate::pds::generator::{GeneratorKind, Placement, RoadConfig};
use crate::pds::room::find_road_config;
use crate::pds::sanitize::limits;
use crate::pds::types::{Fp, Fp3, Fp4, TransformData};
use crate::pds::{RoomRecord, material_finish, ruin};
use crate::seeded_defaults::{SceneCharacter, ThemeArchetype, fnv1a_64};
use crate::state::{CurrentRoomDid, LiveRoomRecord};

use super::FinishedHeightMap;

/// Name prefix for every injected lot building (any layout seed).
const LOT_PREFIX: &str = "lot_building_";
/// Theme used when the room's own theme has no landmark-role catalogue entry
/// yet — mirrors the settlement deriver's fallback so a road-growing room of a
/// still-sparse theme is never left empty.
const FALLBACK_THEME: ThemeArchetype = ThemeArchetype::AncientClassical;
/// Upper bound on injected building *placements*. Buildings share one generator
/// per distinct catalogue entry (so generators stay few — a placement per lot,
/// not an asset per lot); the injection clamps this to the record's free
/// placement budget (`MAX_PLACEMENTS` − existing) so a packed map can't trip
/// sanitiser truncation. The enclosed-lot count is usually the real limiter;
/// tune down if spawn cost bites on wasm.
const MAX_LOT_BUILDINGS: usize = 256;
/// Distinct sub-stream salt for the building-pick RNG.
const LOT_STREAM_SALT: u64 = 0x10C5_B011_D196_5EED;
/// Sink (m) below the terrain snap so foundations bite into slopes rather than
/// leaving daylight under the downhill edge (matches the settlement deriver).
const FOUNDATION_SINK_M: f32 = 0.35;

/// Per-seed building name prefix — the record-side half of the idempotency
/// key for one layout (survives session restarts inside the saved record).
fn seed_prefix(seed: u64) -> String {
    format!("{LOT_PREFIX}{seed}_")
}

/// Session-side idempotency key (#882): the layout-relevant subset of the
/// network config. Only fields that feed `build_road_graph` — and thus move
/// the enclosed blocks — participate; ribbon-profile dims (half-widths,
/// curbs, skirt) re-mesh roads without moving lots, so editing them must
/// NOT churn the buildings. The seed prefix alone missed spacing/extent
/// edits, leaving buildings standing on the previous layout until a
/// re-roll.
fn layout_fingerprint(did: &str, c: &RoadConfig) -> String {
    format!(
        "{did}|{}|{}|{}|{}",
        c.seed, c.district_half_extent.0, c.major_spacing.0, c.minor_spacing.0
    )
}

/// What [`maybe_populate_lots`] should do for an active network, from the
/// record state (`populated` = buildings with the current seed prefix
/// exist) and the session fingerprint. Pure so the idempotency contract is
/// unit-testable.
#[derive(PartialEq, Eq, Debug)]
enum LotAction {
    /// Buildings match the current layout — leave them alone.
    Skip,
    /// Fresh session over a record that already carries this seed's
    /// buildings (a load): adopt the fingerprint without churning the
    /// record — saved buildings are trusted, exactly the pre-#882
    /// behavior on load.
    Adopt,
    /// Layout changed (re-roll, spacing/extent edit, or nothing built
    /// yet): strip stale buildings and repopulate.
    Repopulate,
}

fn lot_action(populated: bool, session_fp: Option<&str>, current_fp: &str) -> LotAction {
    if !populated {
        return LotAction::Repopulate;
    }
    match session_fp {
        Some(prev) if prev == current_fp => LotAction::Skip,
        Some(_) => LotAction::Repopulate,
        None => LotAction::Adopt,
    }
}

/// Whether a placement (any referencing variant) targets an injected lot
/// building.
fn refs_lot_building(p: &Placement) -> bool {
    match p {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => generator_ref.starts_with(LOT_PREFIX),
        Placement::Unknown => false,
    }
}

/// Remove every injected lot building (and its placement) from `record`.
/// Returns whether anything was removed, so the caller only flags the record
/// dirty when there was stale state to clear.
fn strip_lot_buildings(record: &mut RoomRecord) -> bool {
    let names: Vec<String> = record
        .generators
        .keys()
        .filter(|k| k.starts_with(LOT_PREFIX))
        .cloned()
        .collect();
    if names.is_empty() {
        return false;
    }
    for n in &names {
        record.generators.remove(n);
    }
    record.placements.retain(|p| !refs_lot_building(p));
    true
}

/// Inject lot buildings into `record`, deterministic in the room DID + the
/// network's layout `seed`. Returns the number placed.
fn inject_lot_buildings(
    record: &mut RoomRecord,
    lots: &[crate::urban::BuildingLot],
    did: &str,
    seed: u64,
) -> usize {
    let scene = SceneCharacter::for_seed(fnv1a_64(did));
    // Fall back to a guaranteed-populated theme if the room's own theme has no
    // landmark entry yet, exactly as the settlement deriver does.
    let theme = if entries_for(scene.theme, StructureRole::Landmark)
        .next()
        .is_some()
    {
        scene.theme
    } else {
        FALLBACK_THEME
    };
    let (prosperity, escalation) = (scene.prosperity_tier(), scene.escalation_tier());
    let pool = |role| -> Vec<&'static dyn CatalogueEntry> {
        entries_for_room(theme, role, prosperity, escalation).collect()
    };
    let landmark = pool(StructureRole::Landmark);
    let secondary = pool(StructureRole::Secondary);
    let prop = pool(StructureRole::Prop);
    if landmark.is_empty() && secondary.is_empty() && prop.is_empty() {
        return 0;
    }

    // Rank lots largest-first: the biggest block takes the landmark, the next
    // band fills with secondary buildings, the long tail with props.
    let mut ranked: Vec<&crate::urban::BuildingLot> = lots.iter().collect();
    ranked.sort_by(|a, b| (b.width * b.depth).total_cmp(&(a.width * a.depth)));
    // One placement per lot, capped to the free placement budget so a packed
    // map can't trip sanitiser truncation. Generators are shared by entry, so
    // the placement budget — not the generator budget — is the binding limit.
    let cap = MAX_LOT_BUILDINGS.min(limits::MAX_PLACEMENTS.saturating_sub(record.placements.len()));
    ranked.truncate(cap);

    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ LOT_STREAM_SALT);
    let prefix = seed_prefix(seed);
    // One shared generator per distinct catalogue entry: every lot that picks
    // the same building references it, so the compiler bakes that mesh once and
    // instances it across the placements (the record stays compact instead of
    // carrying a near-duplicate asset per lot). Per-lot variety comes from the
    // placement transform — street-facing yaw + lot-fit scale.
    let mut by_slug: HashMap<&'static str, String> = HashMap::new();
    let mut placed = 0usize;
    for (i, lot) in ranked.iter().enumerate() {
        // Role by rank: lot 0 = landmark; next ~20% = secondary; rest = props.
        // Each role falls back to the others so a theme missing one role still
        // populates rather than dropping lots.
        let order: [&[&'static dyn CatalogueEntry]; 3] = if i == 0 {
            [&landmark, &secondary, &prop]
        } else if i * 5 < ranked.len() {
            [&secondary, &prop, &landmark]
        } else {
            [&prop, &secondary, &landmark]
        };
        let Some(chosen) = order.into_iter().find(|p| !p.is_empty()) else {
            continue;
        };
        let entry = chosen[(rng.next_u32() as usize) % chosen.len()];
        let slug = entry.slug();

        // Get-or-build the one shared generator for this entry.
        let name = if let Some(existing) = by_slug.get(slug) {
            existing.clone()
        } else if record.generators.len() >= limits::MAX_GENERATORS {
            // No budget for a new distinct asset — never hit in practice (the
            // catalogue pool is tens of entries). Skip rather than mis-scale a
            // reuse onto a lot meant for a different building.
            continue;
        } else {
            let mut tree = entry.build(did);
            // One deterministic derivation per entry, shared by all its
            // instances (grammar draw + socio finish + escalation damage).
            let entry_seed = seed ^ fnv1a_64(slug);
            if let GeneratorKind::Shape { seed: s, .. } = &mut tree.kind {
                *s = entry_seed;
            }
            material_finish::apply_socio_finish(&mut tree, scene.prosperity, scene.escalation);
            ruin::apply_ruin(&mut tree, scene.escalation, entry_seed);
            let name = format!("{prefix}{slug}");
            record.generators.insert(name.clone(), tree);
            by_slug.insert(slug, name.clone());
            name
        };

        // Per-lot placement of the shared building: lot-fit scale, street-facing.
        let fp = entry.footprint();
        let fit = (lot.width.min(lot.depth) / (2.0 * fp.clearance.max(0.5))).clamp(0.5, 2.0);
        let half_yaw = lot.yaw * 0.5;
        record.placements.push(Placement::Absolute {
            generator_ref: name,
            transform: TransformData {
                translation: Fp3([lot.position[0], -FOUNDATION_SINK_M, lot.position[1]]),
                rotation: Fp4([0.0, half_yaw.sin(), 0.0, half_yaw.cos()]),
                scale: Fp3([fit, fit, fit]),
            },
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: Fp(fp.clearance),
        });
        placed += 1;
    }
    placed
}

/// The active lot-growing network config: enabled, opted into lot population.
fn active_config(record: &RoomRecord) -> Option<RoadConfig> {
    find_road_config(record)
        .filter(|c| c.enabled && c.populate_lots)
        .cloned()
}

/// Populate the road network's lots with buildings when the heightmap or record
/// changes, writing them into the live record (which recompiles + flags dirty).
/// Idempotent per layout seed; sweeps stale buildings on re-roll / toggle-off.
pub(super) fn maybe_populate_lots(
    mut record: ResMut<LiveRoomRecord>,
    did: Option<Res<CurrentRoomDid>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut undo_signals: ResMut<crate::ui::undo::RoomWriteSignals>,
    time: Res<Time>,
    // Session-side layout fingerprint (#882): `None` until the first
    // decision this run, cleared when the network deactivates.
    mut session_fp: Local<Option<String>>,
    // Trailing re-derive debounce (#884): lot extraction re-traces the
    // whole street graph, so a spacing-slider drag must cost one
    // re-derive on release, not one per tick — the same cadence as the
    // road re-mesh.
    mut due: Local<Option<f64>>,
) {
    let Some(heightmap) = heightmap else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let did_str = did.as_ref().map_or("", |d| d.0.as_str());

    // 1 — change detection decides + arms. Sweeps (network gone) stay
    // immediate: a toggle isn't a drag storm and leaving stale buildings
    // up for the debounce window would flash them at the old layout.
    if heightmap.is_changed() || record.is_changed() {
        match active_config(&record.0) {
            None => {
                *session_fp = None;
                *due = None;
                if record
                    .0
                    .generators
                    .keys()
                    .any(|k| k.starts_with(LOT_PREFIX))
                {
                    // Derived write (#862): fold the sweep into the edit that
                    // disabled the network, not a phantom undo entry of its own.
                    undo_signals.derived = true;
                    strip_lot_buildings(&mut record.0);
                }
                return;
            }
            Some(config) => {
                let fp = layout_fingerprint(did_str, &config);
                let prefix = seed_prefix(config.seed);
                let populated = record.0.generators.keys().any(|k| k.starts_with(&prefix));
                match lot_action(populated, session_fp.as_deref(), &fp) {
                    // Layout matches the standing buildings — also cancels a
                    // pending re-derive when an undo walked the edit back.
                    LotAction::Skip => *due = None,
                    LotAction::Adopt => {
                        *session_fp = Some(fp);
                        *due = None;
                    }
                    LotAction::Repopulate => {
                        *due = Some(now + super::roads::ROAD_EDIT_DEBOUNCE_SECS)
                    }
                }
            }
        }
    }

    // 2 — deadline reached: re-evaluate against the CURRENT record (edits
    // inside the debounce window fold in) and repopulate if still needed.
    if !due.is_some_and(|d| now >= d) {
        return;
    }
    *due = None;
    let Some(config) = active_config(&record.0) else {
        return; // the change branch above already swept
    };
    let fp = layout_fingerprint(did_str, &config);
    let prefix = seed_prefix(config.seed);
    let populated = record.0.generators.keys().any(|k| k.starts_with(&prefix));
    if lot_action(populated, session_fp.as_deref(), &fp) != LotAction::Repopulate {
        return;
    }

    // A changed layout (re-roll, spacing / extent edit) or none yet: clear
    // stale, then repopulate. Derived write (#862): the strip + inject
    // below are fallout of the road edit that changed the layout — fold
    // them into that entry so one undo reverts the edit and its buildings
    // together.
    undo_signals.derived = true;
    let stripped = strip_lot_buildings(&mut record.0);
    *session_fp = Some(fp);
    let lots = crate::urban::extract_building_lots(&heightmap.0, &config);
    if lots.is_empty() {
        // Nothing enclosed; the strip above (if any) already updated the record.
        let _ = stripped;
        return;
    }
    inject_lot_buildings(&mut record.0, &lots, did_str, config.seed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::BuildingLot;

    fn lot(x: f32, z: f32, w: f32, d: f32) -> BuildingLot {
        BuildingLot {
            position: [x, z],
            yaw: 0.3,
            width: w,
            depth: d,
        }
    }

    /// Any DID works: the injector falls back to [`FALLBACK_THEME`] when the
    /// room's own theme has no landmark entry, so the pools are never empty.
    fn urban_did() -> String {
        "did:test:0".to_string()
    }

    #[test]
    fn layout_fingerprint_tracks_layout_fields_only() {
        // #882: the graph (and thus the lots) depends on seed + extent +
        // spacings; ribbon-profile dims must NOT churn the buildings.
        let base = RoadConfig::default();
        let fp = |c: &RoadConfig| layout_fingerprint("did:test:0", c);

        let mut spacing = base.clone();
        spacing.major_spacing.0 += 10.0;
        assert_ne!(fp(&base), fp(&spacing), "spacing edits move lots");

        let mut extent = base.clone();
        extent.district_half_extent.0 += 25.0;
        assert_ne!(fp(&base), fp(&extent), "extent edits move lots");

        let mut seeded = base.clone();
        seeded.seed ^= 1;
        assert_ne!(fp(&base), fp(&seeded), "re-roll moves lots");

        let mut ribbon = base.clone();
        ribbon.major_half_width.0 += 1.0;
        ribbon.curb_height.0 += 0.1;
        ribbon.skirt_depth.0 += 3.0;
        assert_eq!(
            fp(&base),
            fp(&ribbon),
            "ribbon-profile edits must not re-derive lots"
        );

        assert_ne!(
            fp(&base),
            layout_fingerprint("did:test:1", &base),
            "fingerprint is per-room"
        );
    }

    #[test]
    fn lot_action_contract() {
        let fp = "did|1|170|95|55";
        let other = "did|1|170|105|55";
        // Nothing built yet → populate, regardless of session state.
        assert_eq!(lot_action(false, None, fp), LotAction::Repopulate);
        assert_eq!(lot_action(false, Some(fp), fp), LotAction::Repopulate);
        // Built + matching session fingerprint → leave alone.
        assert_eq!(lot_action(true, Some(fp), fp), LotAction::Skip);
        // Built + differing fingerprint (spacing edit, same seed) → rebuild.
        assert_eq!(lot_action(true, Some(other), fp), LotAction::Repopulate);
        // Built + fresh session (a load): trust the saved buildings, adopt.
        assert_eq!(lot_action(true, None, fp), LotAction::Adopt);
    }

    #[test]
    fn inject_places_buildings_and_strip_removes_them() {
        let did = urban_did();
        let mut record = RoomRecord::default_for_did(&did);
        let before_gens = record.generators.len();
        let lots: Vec<BuildingLot> = (0..20)
            .map(|i| lot(i as f32 * 8.0, 0.0, 12.0, 14.0))
            .collect();

        let n = inject_lot_buildings(&mut record, &lots, &did, 4242);
        assert!(n > 0, "expected buildings injected onto the lots");
        // One placement per lot...
        let placements = record
            .placements
            .iter()
            .filter(|p| refs_lot_building(p))
            .count();
        assert_eq!(placements, n);
        // ...but generators are SHARED by entry, so there are at most as many
        // generators as placements (fewer when lots repeat a building).
        let gens_added = record.generators.len() - before_gens;
        assert!(
            (1..=n).contains(&gens_added),
            "lot generators ({gens_added}) must be ≥1 and ≤ placements ({n})"
        );
        // Every lot placement resolves to an existing shared generator...
        for p in record.placements.iter().filter(|p| refs_lot_building(p)) {
            if let Placement::Absolute { generator_ref, .. } = p {
                assert!(
                    record.generators.contains_key(generator_ref),
                    "placement references missing generator {generator_ref}"
                );
            }
        }
        // ...and every lot generator carries the seed-tagged prefix.
        assert!(
            record
                .generators
                .keys()
                .filter(|k| k.starts_with(LOT_PREFIX))
                .all(|k| k.starts_with(&seed_prefix(4242))),
            "every lot building must carry the layout-seed prefix"
        );

        assert!(strip_lot_buildings(&mut record));
        assert_eq!(record.generators.len(), before_gens, "strip must be exact");
        assert!(!record.placements.iter().any(refs_lot_building));
        assert!(!strip_lot_buildings(&mut record), "second strip is a no-op");
    }

    #[test]
    fn injection_is_bounded_deduped_and_deterministic() {
        let did = urban_did();
        let lots: Vec<BuildingLot> = (0..400).map(|i| lot(i as f32, 0.0, 10.0, 10.0)).collect();

        let mut a = RoomRecord::default_for_did(&did);
        let mut b = RoomRecord::default_for_did(&did);
        let before = a.generators.len();
        let na = inject_lot_buildings(&mut a, &lots, &did, 7);
        let nb = inject_lot_buildings(&mut b, &lots, &did, 7);
        assert_eq!(na, nb);
        assert!(na <= MAX_LOT_BUILDINGS, "exceeded the placement cap");
        // Dedup: 400 lots collapse onto a handful of shared generators (one per
        // distinct catalogue entry), far fewer than the placement count.
        let gens_added = a.generators.len() - before;
        assert!(
            gens_added >= 1 && gens_added < na,
            "buildings must share generators by entry ({gens_added} generators for {na} placements)"
        );
        // Same DID + seed + lots ⇒ identical injected generators & placements.
        assert!(
            !crate::state::records_differ(&a, &b),
            "lot injection is non-deterministic"
        );
    }
}
