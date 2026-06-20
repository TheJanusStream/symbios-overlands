//! Load-time lot-based building population.
//!
//! A road-growing room bakes **no** concentric near-spawn settlement at seed
//! time (see [`crate::pds::room`]'s `default_for_seed`). Instead, once the
//! heightmap exists, this system extracts the road network's enclosed building
//! lots ([`crate::urban::extract_building_lots`]) and injects themed catalogue
//! buildings onto them — straight into the live record, so they compile to
//! entities like any authored placement and the author can save them.
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

/// Per-seed building name prefix — the idempotency key for one layout.
fn seed_prefix(seed: u64) -> String {
    format!("{LOT_PREFIX}{seed}_")
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
) {
    let Some(heightmap) = heightmap else {
        return;
    };
    // Only consider work on frames where the terrain or the record changed.
    if !heightmap.is_changed() && !record.is_changed() {
        return;
    }

    let Some(config) = active_config(&record.0) else {
        // No active lot-growing network (disabled, no roads, or populate off):
        // sweep any buildings a prior config left behind.
        if record
            .0
            .generators
            .keys()
            .any(|k| k.starts_with(LOT_PREFIX))
        {
            strip_lot_buildings(&mut record.0);
        }
        return;
    };

    // Already populated for this exact layout? Leave it alone.
    let prefix = seed_prefix(config.seed);
    if record.0.generators.keys().any(|k| k.starts_with(&prefix)) {
        return;
    }

    // A different layout (re-roll) or none yet: clear stale, then repopulate.
    let stripped = strip_lot_buildings(&mut record.0);
    let lots = crate::urban::extract_building_lots(&heightmap.0, &config);
    if lots.is_empty() {
        // Nothing enclosed; the strip above (if any) already updated the record.
        let _ = stripped;
        return;
    }
    let did_str = did.as_ref().map_or("", |d| d.0.as_str());
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

    /// A road-growing DID so the catalogue pools are non-empty.
    fn urban_did() -> String {
        for s in 0u64..512 {
            let did = format!("did:test:{s}");
            if crate::pds::room::theme_grows_roads(SceneCharacter::for_seed(fnv1a_64(&did)).theme) {
                return did;
            }
        }
        panic!("no road-growing test DID found");
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
        assert!(
            n > 0,
            "expected buildings injected for a road-growing theme"
        );
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
