//! Load-time lot-based building population.
//!
//! Seeded rooms grow no road network (too heavy for a good default room on
//! wasm), so this layer only serves rooms whose author added a `RoadNetwork`
//! generator in the editor (or saved one back when roads were seeded). Once
//! the heightmap exists, this system extracts the road network's enclosed
//! building lots ([`crate::urban::extract_building_lots`]) and injects themed
//! catalogue buildings onto them - straight into the live record, so they
//! compile to entities like any authored placement and the author can save
//! them.
//!
//! Everything is deterministic in the room DID + the network's layout seed: the
//! terrain reproduces on every peer, the building picks come from a seeded
//! stream, and at layout revision 2 the streets and lots are derived with
//! portable maths (#1563), so every peer that derives the record lands
//! identical buildings even before anyone saves. At revisions 0 and 1 the
//! platform's maths derive them, and a native client and the web one can
//! keep different lots where one sits on a threshold - which matters only
//! until the district is saved: a saved district is adopted, never derived
//! again. The buildings share one generator per
//! catalogue entry and drawn scale, named `lot_building_{seed}_{slug}` - with
//! an `@{scale}` suffix off scale 1.0, see below - and the seed-tagged prefix
//! is the idempotency key: a re-roll (new seed) strips the stale set and
//! repopulates, an unchanged layout is left alone, and turning the layer off
//! sweeps them.
//!
//! # Fitting a building to its lot (#1553)
//!
//! The world compile draws an absolute placement at unit scale whatever its
//! record says - an item is scaled by its root prim (#1454) - so a lot
//! building is drawn at its catalogue size unless the network asks for the
//! fit ([`LotSettings::fit`]). Then the fit is baked into the generator, as
//! the seeded settlements' members are, and the placement keeps a unit
//! scale. The fit is rounded DOWN to a quarter-octave bucket (0.5, 0.59,
//! 0.71, 0.84, 1.0, 1.19, ...), so above the clamp's floor a building never
//! outgrows its lot (at the floor it still can: a 32 m tower on an 8 m lot -
//! except at layout revision 1, #1558, where a lot grows only what fits it),
//! and one entry grows at most a handful of generators rather than one per
//! lot. Props keep street scale, at most 1.0 ([`fit_clamp`]).
//!
//! Without the fit the injection writes what it wrote before the switch
//! existed: one generator per entry, the clamped fit on each placement's
//! own (undrawn) scale. A saved district grown again - a portal into the
//! room, a sibling network that grows nothing - therefore comes back byte
//! for byte, but for one change since: every Shape node of an entry's
//! generator now draws with the entry's seed (#1514), which used to reach
//! none of them, so a district grown before that comes back with its
//! grammar buildings drawn anew.
//!
//! # What the district is made of (#1555)
//!
//! The catalogue pools, the material finish and the ruin all read the room's
//! seeded prosperity and escalation unless the network's [`LotSettings`]
//! overrides them, and [`LotTierBias::Downtown`] grows a building on every
//! lot instead of the props the other mixes give the long tail.
//!
//! [`LotTierBias::Downtown`]: crate::pds::generator::LotTierBias::Downtown

use std::collections::HashMap;

use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::catalogue::{CatalogueEntry, StructureRole, entries_for, entries_for_room};
use crate::pds::generator::{Generator, LotSettings, Placement, RoadConfig};
use crate::pds::sanitize::limits;
use crate::pds::types::{FP_SCALE, Fp, Fp3, Fp4, TransformData};
use crate::pds::{RoomRecord, material_finish, ruin};
use crate::seeded_defaults::{
    EscalationTier, ProsperityTier, SceneCharacter, ThemeArchetype, fnv1a_64,
};
use crate::state::{CurrentRoomDid, LiveRoomRecord};

use super::{FinishedHeightMap, HeightMapSource};

/// Name prefix for every injected lot building (any layout seed).
const LOT_PREFIX: &str = "lot_building_";
/// Name prefix for every injected street-furniture prop (#893).
const FURNITURE_PREFIX: &str = "street_prop_";
/// Distinct sub-stream salt for the furniture-pick RNG (#893).
const FURNITURE_STREAM_SALT: u64 = 0x57F0_0F57_F00F_57F0;
/// Cap on injected furniture placements - beyond ~160 lamps the wasm spawn
/// cost outruns the ambience.
pub(crate) const MAX_FURNITURE_PROPS: usize = 160;
/// Shallow terrain sink for furniture (m) - props stand on the verge, they
/// don't need a building's foundation bite.
const FURNITURE_SINK_M: f32 = 0.1;
/// Theme used when the room's own theme has no landmark-role catalogue entry
/// yet - mirrors the settlement deriver's fallback so a road-growing room of a
/// still-sparse theme is never left empty.
pub(crate) const FALLBACK_THEME: ThemeArchetype = ThemeArchetype::AncientClassical;
/// Upper bound on injected building *placements*. Buildings share one generator
/// per distinct catalogue entry (so generators stay few - a placement per lot,
/// not an asset per lot); the injection clamps this to the record's free
/// placement budget (`MAX_PLACEMENTS` − existing) so a packed map can't trip
/// sanitiser truncation. The enclosed-lot count is usually the real limiter;
/// tune down if spawn cost bites on wasm.
pub(crate) const MAX_LOT_BUILDINGS: usize = 256;
/// Distinct sub-stream salt for the building-pick RNG.
const LOT_STREAM_SALT: u64 = 0x10C5_B011_D196_5EED;
/// Sink (m) below the terrain snap so foundations bite into slopes rather than
/// leaving daylight under the downhill edge (matches the settlement deriver).
pub(crate) const FOUNDATION_SINK_M: f32 = 0.35;
/// The quarter-octave mantissas 2^(j/4), j = 0..3: every fit bucket is one
/// of these times a power of two (#1553). A table rather than `exp2`, because
/// the bucket is written INTO the record - the generator root's scale, the
/// placement's clearance and the generator's name - and the platform libm is
/// not bit-identical across peers (#1132), while a power of two times an
/// `f32` is an exact exponent shift.
const QUARTER_OCTAVES: [f32; 4] = [1.0, 1.189_207_1, std::f32::consts::SQRT_2, 1.681_792_8];
/// The widest bucket index the fit search walks to, either way. Far outside
/// anything the sanitised clamp (0.1 to 5.0, buckets -14 to 9) reaches - it
/// only bounds the loop.
const MAX_BUCKET_INDEX: i32 = 64;
/// The largest scale a prop is fitted to (#1553): street scale. See
/// [`fit_clamp`].
const PROP_MAX_FIT: f32 = 1.0;

/// The authored lot theme override (#892) resolved against the roster, or
/// `None` when the label is empty or names nothing this build knows.
///
/// Shared with the editor's Theme combo (#1251 f390). The lenient
/// case-insensitive match used to live only inside `inject_lot_buildings`,
/// so the panel printed the raw stored string as its selected text and
/// asserted a theme that was never growing - the combo said "Steampunk"
/// while this fell through to the room theme, with no row in its own
/// dropdown highlighted. One predicate is what stops the two answering
/// differently.
pub fn resolve_lot_theme(override_label: &str) -> Option<ThemeArchetype> {
    let wanted = override_label.trim();
    if wanted.is_empty() {
        return None;
    }
    ThemeArchetype::ALL
        .into_iter()
        .find(|t| t.label().eq_ignore_ascii_case(wanted))
}

/// Per-network, per-seed generator name prefix - the record-side half of
/// the idempotency key for one layout (survives session restarts inside the
/// saved record). Network 0 keeps the legacy shape so pre-#895 records
/// adopt cleanly; later networks are namespaced by child index.
fn net_prefix(base: &str, net: usize, seed: u64) -> String {
    if net == 0 {
        format!("{base}{seed}_")
    } else {
        format!("{base}n{net}_{seed}_")
    }
}

#[cfg(test)]
fn seed_prefix(seed: u64) -> String {
    net_prefix(LOT_PREFIX, 0, seed)
}

/// Session-side idempotency key (#882): the layout-relevant subset of the
/// network config. Only fields that feed `build_road_graph` - and thus move
/// the enclosed blocks - participate; ribbon-profile dims (half-widths,
/// curbs, skirt) re-mesh roads without moving lots, so editing them must
/// NOT churn the buildings. The seed prefix alone missed spacing/extent
/// edits, leaving buildings standing on the previous layout until a
/// re-roll.
/// Combined fingerprint across every active network (#895).
fn combined_fingerprint(did: &str, configs: &[RoadConfig], water_level: Option<f32>) -> String {
    configs
        .iter()
        .enumerate()
        .map(|(i, c)| format!("[{i}]{}", layout_fingerprint(did, c, water_level)))
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

/// `water_level` is the room's water line: it moves the streets and lots
/// of a network that avoids water (#1552), so it is part of that network's
/// key and of no other's - a dry-traced network must not churn its
/// buildings when the lake is raised.
///
/// The street field (#1556) is part of every network's key: its smoothing
/// and basis fields move the streets, and its keep-out discs drop lots. So is
/// the layout revision from 1 on (#1558): it re-derives the graph and lots.
fn layout_fingerprint(did: &str, c: &RoadConfig, water_level: Option<f32>) -> String {
    let water = if c.avoid_water {
        format!("|water={water_level:?}")
    } else {
        String::new()
    };
    // The layout revision (#1558) re-derives the graph and the lots; at 0,
    // every network saved before it, the key is what it always was.
    let layout = if c.layout_revision == 0 {
        String::new()
    } else {
        format!("|layout={}", c.layout_revision)
    };
    format!(
        "{did}|{}|{}|{}|{}|{}|{}|{:?}",
        c.seed,
        c.district_half_extent.0,
        c.major_spacing.0,
        c.minor_spacing.0,
        c.center.0[0],
        c.center.0[1],
        c.style,
    ) + &format!(
        "|{:?}|{:?}|{:?}{water}{layout}",
        c.lots, c.furniture, c.field
    )
}

/// What [`maybe_populate_lots`] should do for an active network, from the
/// record state (`populated` = buildings with the current seed prefix
/// exist) and the session fingerprint. Pure so the idempotency contract is
/// unit-testable.
#[derive(PartialEq, Eq, Debug)]
enum LotAction {
    /// Buildings match the current layout - leave them alone.
    Skip,
    /// Fresh session over a record that already carries this seed's
    /// buildings (a load): adopt the fingerprint without churning the
    /// record - saved buildings are trusted, exactly the pre-#882
    /// behavior on load.
    Adopt,
    /// Layout changed (re-roll, spacing/extent edit, or nothing built
    /// yet): strip stale buildings and repopulate.
    Repopulate,
}

fn lot_action(populated: bool, session_fp: Option<&str>, current_fp: &str) -> LotAction {
    // "We derived this layout and it produced nothing" is a terminal state
    // (#1245 f376). The fingerprint check used to sit BELOW the `populated`
    // test, so a layout that grows nothing - density 0, a spacing wider than
    // its own extent, an empty theme pool, a spent placement budget - was
    // never `populated`, and every unrelated edit to the record therefore
    // armed another Repopulate whose strip removed nothing and still
    // dirtied the record: one extra placement-fingerprint pass and one extra
    // whole-room broadcast to every guest, a third of a second after each
    // edit, forever.
    if session_fp.is_some_and(|prev| prev == current_fp) {
        return LotAction::Skip;
    }
    if !populated {
        return LotAction::Repopulate;
    }
    match session_fp {
        Some(_) => LotAction::Repopulate,
        None => LotAction::Adopt,
    }
}

/// Whether a placement was planted by either road-derived layer (#1211).
pub(super) fn is_road_grown(p: &Placement) -> bool {
    refs_lot_building(p) || refs_street_prop(p)
}

/// Whether a generator key belongs to the road layer's derived namespace
/// (#1245 f382).
///
/// The prefix IS the idempotency key: `strip_lot_buildings` matches on it,
/// and so does `layer_content`. Rename a grown generator and the strip stops
/// finding it, `layer_content` reports the layer missing, and a complete fresh district
/// grows through the renamed survivor's placements - interpenetrating the
/// first. The editor asks this before offering Rename on the very rows it
/// would corrupt.
pub fn is_derived_generator_key(key: &str) -> bool {
    key.starts_with(LOT_PREFIX) || key.starts_with(FURNITURE_PREFIX)
}

/// What one injection pass did and did not do (#1211). `placed` is the
/// old return value; the rest is the arithmetic behind it, which used to
/// be invisible: density thinning, the cap (per-district constant or the
/// room's free placement budget, whichever bit) and the generator ceiling.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct InjectReport {
    found: usize,
    kept: usize,
    placed: usize,
    dropped_to_cap: usize,
    capped_by_budget: bool,
    generator_cap_skips: usize,
    /// Lots no building of their pools fits (#1558, layout revision 1).
    too_small: usize,
}

/// The cap one injection may fill: the layer's own constant, or the
/// room's free placement budget when that is smaller (#1211 names which).
fn placement_cap(layer_max: usize, record: &RoomRecord) -> (usize, bool) {
    let budget = limits::MAX_PLACEMENTS.saturating_sub(record.placements.len());
    (layer_max.min(budget), budget < layer_max)
}

/// Whether a placement (any referencing variant) targets an injected lot
/// building.
fn refs_lot_building(p: &Placement) -> bool {
    placement_ref(p).is_some_and(|r| r.starts_with(LOT_PREFIX))
}

/// Whether a placement targets an injected street-furniture prop (#893).
fn refs_street_prop(p: &Placement) -> bool {
    placement_ref(p).is_some_and(|r| r.starts_with(FURNITURE_PREFIX))
}

fn placement_ref(p: &Placement) -> Option<&str> {
    match p {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => Some(generator_ref),
        Placement::Unknown => None,
    }
}

/// Remove every injected lot building (and its placement) from `record`.
///
/// Returns how many PLACEMENTS were removed - the number of objects that
/// visibly disappear - so the caller can both skip dirtying the record when
/// there was nothing stale (#1245 f376) and say what it replaced (#1245
/// f378). It used to return a bare `bool`, which answered the first question
/// and not the second.
fn strip_lot_buildings(record: &mut RoomRecord) -> usize {
    let names: Vec<String> = record
        .generators
        .keys()
        .filter(|k| k.starts_with(LOT_PREFIX) || k.starts_with(FURNITURE_PREFIX))
        .cloned()
        .collect();
    if names.is_empty() {
        return 0;
    }
    for n in &names {
        record.generators.remove(n);
    }
    let before = record.placements.len();
    record
        .placements
        .retain(|p| !refs_lot_building(p) && !refs_street_prop(p));
    before - record.placements.len()
}

/// Fit bucket `k` (#1553): 2^(k/4), the same bits on every platform - a
/// table mantissa times an exact power of two.
fn bucket_scale(k: i32) -> f32 {
    let mut scale = QUARTER_OCTAVES[k.rem_euclid(4) as usize];
    let octave = k.div_euclid(4);
    for _ in 0..octave {
        scale *= 2.0;
    }
    for _ in octave..0 {
        scale *= 0.5;
    }
    scale
}

/// The scale a lot's building is drawn at (#1553): the lot's own `fit` (how
/// many times the entry's footprint fits across the lot's narrower side)
/// rounded DOWN to a quarter-octave bucket, so a building never outgrows its
/// lot above the floor, and kept inside the clamp `[lo, hi]` (`lo <= hi`).
///
/// A fit at or past a bound takes the bound itself, bucket or not: an
/// author's 1.8 ceiling draws the lots that ask for more at 1.8, not at the
/// 1.68 bucket below it, and a fit that rounds down past a floor that sits
/// between buckets stops at the floor. The floor is the one place a building
/// can still outgrow its lot - as it always could, when the clamp was the
/// whole of the fit.
pub(crate) fn fitted_scale(fit: f32, lo: f32, hi: f32) -> f32 {
    if fit.is_nan() || fit <= lo {
        return lo;
    }
    if fit >= hi {
        return hi;
    }
    // Plain comparisons against exact bucket values, so the answer is
    // bucket(k) <= fit < bucket(k + 1) to the last bit - no `log2`, whose
    // rounding would land a fit sitting on a bucket either side of it.
    let mut k = 0;
    while k > -MAX_BUCKET_INDEX && bucket_scale(k) > fit {
        k -= 1;
    }
    while k < MAX_BUCKET_INDEX && bucket_scale(k + 1) <= fit {
        k += 1;
    }
    bucket_scale(k).max(lo)
}

/// The fit clamp one catalogue entry is held to (#1553), as `(lo, hi)`.
///
/// Landmarks and secondary buildings use the whole authored
/// `[scale_min, scale_max]`. A prop keeps street scale: its fit is capped at
/// `min(scale_max, 1.0)`, because a planter or a barricade drawn twice its
/// size is a giant, not a building fitted to its lot - and a `scale_min`
/// above the cap comes down to meet it rather than lifting the prop past it.
/// The cap reads the ENTRY's role, not the rank of the lot it lands on, so a
/// prop that fills a landmark lot (a theme with no landmark) stays a prop.
fn fit_clamp(settings: &LotSettings, role: StructureRole) -> (f32, f32) {
    let lo = settings.scale_min.0.min(settings.scale_max.0);
    let hi = settings.scale_max.0.max(settings.scale_min.0);
    if role == StructureRole::Prop {
        let hi = hi.min(PROP_MAX_FIT);
        (lo.min(hi), hi)
    } else {
        (lo, hi)
    }
}

/// A drawn scale in the record's own fixed point (#1553): ten-thousandths,
/// as the wire writes an [`Fp`]. The key one shared generator is grown and
/// named under.
pub(crate) fn scale_e4(scale: f32) -> i64 {
    (scale * FP_SCALE).round() as i64
}

/// The shared generator one catalogue entry at one drawn scale grows as
/// (#1553). Scale 1.0 keeps the name every lot building had before the fit
/// was baked, `{prefix}{slug}`; any other appends `@` and the scale to the
/// record's 1e-4, trailing zeros dropped - `@0.7071`, `@2`.
///
/// Every character is legal in a generator key (visible, no `/`, which
/// separates a node's cache key from its root's), and a slug is
/// `[a-z0-9_]`, so no suffixed name is another entry's plain one. The name
/// keeps `prefix`, which is what `strip_lot_buildings`, `layer_content` and
/// [`is_derived_generator_key`] look for.
fn lot_generator_name(prefix: &str, slug: &str, scale_e4: i64) -> String {
    if scale_e4 == FP_SCALE as i64 {
        return format!("{prefix}{slug}");
    }
    let whole = scale_e4 / FP_SCALE as i64;
    let frac = scale_e4 % FP_SCALE as i64;
    if frac == 0 {
        format!("{prefix}{slug}@{whole}")
    } else {
        let digits = format!("{frac:04}");
        format!("{prefix}{slug}@{whole}.{}", digits.trim_end_matches('0'))
    }
}

/// The prosperity and escalation one network's lots and street props grow
/// with (#1555): the authored overrides where set, the room's own seeded
/// scene where not - for the catalogue pools (by tier), the socio finish and
/// the ruin alike.
fn lot_character(scene: &SceneCharacter, settings: &LotSettings) -> (f32, f32) {
    (
        settings.prosperity_or(scene.prosperity),
        settings.escalation_or(scene.escalation),
    )
}

/// The catalogue entries of `role` a network grows at `prosperity` and
/// `escalation`, by their tiers.
pub(crate) fn pool_for(
    theme: ThemeArchetype,
    role: StructureRole,
    prosperity: f32,
    escalation: f32,
) -> Vec<&'static dyn CatalogueEntry> {
    entries_for_room(
        theme,
        role,
        ProsperityTier::from_unit(prosperity),
        EscalationTier::from_unit(escalation),
    )
    .collect()
}

/// One shared road-layer generator: the catalogue entry built, its grammars
/// reseeded, finished and ruined by the layer's prosperity and escalation -
/// one derivation per entry, shared by all its instances - and drawn at
/// `scale` by its root (#1553), about the ground point its placement stands
/// it on, as the seeded settlements' members are. Every Shape node takes the
/// entry's seed (#1514): it was stamped on the root only, and every grammar
/// entry roots on a footing box, so the seed reached none of them.
///
/// The middle ring of a geodata region grows its buildings with it too
/// (#1587), so they are the lots' own.
pub(crate) fn grow_generator(
    entry: &dyn CatalogueEntry,
    did: &str,
    entry_seed: u64,
    (prosperity, escalation): (f32, f32),
    scale: f32,
) -> Generator {
    let mut tree = entry.build(did).with_shape_seed(entry_seed);
    material_finish::apply_socio_finish(&mut tree, prosperity, escalation);
    ruin::apply_ruin_bounded(&mut tree, escalation, entry_seed, entry.ruin_max_lean());
    crate::seeded_defaults::room::build::scale_about_ground(&mut tree, scale);
    tree
}

/// [`inject_lots`] as layout revision 0 grows lots - every network saved
/// before #1558 - for the tests of the rules that predate it.
#[cfg(test)]
fn inject_lot_buildings(
    record: &mut RoomRecord,
    lots: &[crate::urban::BuildingLot],
    did: &str,
    seed: u64,
    prefix: &str,
    settings: &LotSettings,
) -> InjectReport {
    inject_lots(record, lots, did, seed, prefix, settings, false)
}

/// Whether `entry`, drawn on `lot` as the network's settings draw it - its
/// fit inside the clamp, or its catalogue size without the fit - fits the
/// lot (#1558): drawn no larger than the lot's narrower side holds.
fn fits_lot(
    entry: &dyn CatalogueEntry,
    lot: &crate::urban::BuildingLot,
    settings: &LotSettings,
) -> bool {
    let fit = lot.width.min(lot.depth) / (2.0 * entry.lot_half_width().max(0.5));
    let drawn = if settings.fit {
        let (lo, hi) = fit_clamp(settings, entry.role());
        fitted_scale(fit, lo, hi)
    } else {
        1.0
    };
    drawn <= fit
}

/// Inject lot buildings into `record`, deterministic in the room DID + the
/// network's layout `seed`. Returns the number placed.
///
/// With `fit_only` - a network at layout revision 1 or later (#1558) - no
/// building or prop is drawn larger than its lot: a lot draws only from the
/// catalogue entries that fit it at the size they would be drawn, falling
/// through the pools as an empty pool does, and grows nothing when none
/// fits. Without it, as every network grew before: a building at its
/// clamp's floor stands on a lot however small.
fn inject_lots(
    record: &mut RoomRecord,
    lots: &[crate::urban::BuildingLot],
    did: &str,
    seed: u64,
    prefix: &str,
    settings: &LotSettings,
    fit_only: bool,
) -> InjectReport {
    let mut report = InjectReport {
        found: lots.len(),
        ..InjectReport::default()
    };
    let scene = SceneCharacter::for_seed(fnv1a_64(did));
    // Authored theme override (#892): a case-insensitive label match against
    // the theme roster; empty / unrecognised falls through to the room theme.
    let base_theme = resolve_lot_theme(&settings.theme_override).unwrap_or(scene.theme);
    // Fall back to a guaranteed-populated theme if the chosen theme has no
    // landmark entry yet, exactly as the settlement deriver does.
    let theme = if entries_for(base_theme, StructureRole::Landmark)
        .next()
        .is_some()
    {
        base_theme
    } else {
        FALLBACK_THEME
    };
    let character = lot_character(&scene, settings);
    let pool = |role| pool_for(theme, role, character.0, character.1);
    let landmark = pool(StructureRole::Landmark);
    let secondary = pool(StructureRole::Secondary);
    let prop = pool(StructureRole::Prop);
    if landmark.is_empty() && secondary.is_empty() && prop.is_empty() {
        return report;
    }

    // Rank lots largest-first: the biggest block takes the landmark, the next
    // band fills with secondary buildings, the long tail with props. A
    // district with a core (#1555) ranks them nearest-first instead, so its
    // landmarks stand round the core; ties keep the size order.
    let mut ranked: Vec<&crate::urban::BuildingLot> = lots.iter().collect();
    ranked.sort_by(|a, b| (b.width * b.depth).total_cmp(&(a.width * a.depth)));
    if let Some(core) = settings.focus {
        let away = |l: &crate::urban::BuildingLot| {
            let (dx, dz) = (l.position[0] - core.0[0], l.position[1] - core.0[1]);
            dx * dx + dz * dz
        };
        ranked.sort_by(|a, b| away(a).total_cmp(&away(b)));
    }
    // Density thinning (#892) BEFORE the budget cap: keep the biggest
    // `density` fraction so a sparse district reads as a town core, not
    // random gaps. Ceil so any nonzero density keeps at least one lot.
    let keep = ((ranked.len() as f32 * settings.density.0.clamp(0.0, 1.0)).ceil() as usize)
        .min(ranked.len());
    ranked.truncate(keep);
    report.kept = keep;
    // One placement per lot, capped to the free placement budget so a packed
    // map can't trip sanitiser truncation. Generators are shared by entry, so
    // the placement budget - not the generator budget - is the binding limit.
    // Counted, not merely applied (#1211): a record already carrying 900
    // authored placements grew 124 buildings out of 400 lots with no notice.
    let (cap, capped_by_budget) = placement_cap(MAX_LOT_BUILDINGS, record);
    report.dropped_to_cap = ranked.len().saturating_sub(cap);
    report.capped_by_budget = capped_by_budget && report.dropped_to_cap > 0;
    ranked.truncate(cap);

    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ LOT_STREAM_SALT);
    // One shared generator per distinct catalogue entry AND drawn scale:
    // every lot that picks the same building at the same fit bucket
    // references it, so the compiler bakes that mesh once and instances it
    // across the placements (the record stays compact instead of carrying a
    // near-duplicate asset per lot). The scale is the generator's, because
    // the compile leaves an absolute placement's unapplied (#1454, #1553);
    // the placement carries the street-facing yaw.
    let mut by_key: HashMap<(&'static str, i64), (String, f32)> = HashMap::new();
    let mut placed = 0usize;
    let pools = [landmark.as_slice(), secondary.as_slice(), prop.as_slice()];
    for (i, lot) in ranked.iter().enumerate() {
        let entry = if fit_only {
            let fitting: [Vec<&'static dyn CatalogueEntry>; 3] = pools.map(|pool| {
                pool.iter()
                    .copied()
                    .filter(|e| fits_lot(*e, lot, settings))
                    .collect()
            });
            let order = role_order(
                settings.tier_bias,
                i,
                ranked.len(),
                [&fitting[0][..], &fitting[1][..], &fitting[2][..]],
            );
            let Some(chosen) = order.into_iter().find(|p| !p.is_empty()) else {
                report.too_small += 1;
                continue;
            };
            chosen[(rng.next_u32() as usize) % chosen.len()]
        } else {
            let Some(chosen) = role_order(settings.tier_bias, i, ranked.len(), pools)
                .into_iter()
                .find(|p| !p.is_empty())
            else {
                continue;
            };
            chosen[(rng.next_u32() as usize) % chosen.len()]
        };
        let slug = entry.slug();

        // The scale this lot draws its building at (#1553): with the fit
        // switched on, the lot's fit rounded down to a quarter-octave bucket
        // inside the entry's clamp; with it off, the catalogue size - what
        // every lot building was drawn at before, so a saved district grown
        // again comes back at the size it was saved at.
        let fp = entry.footprint();
        let fit = lot.width.min(lot.depth) / (2.0 * fp.clearance.max(0.5));
        // The fit a building is drawn at reads its own half side (#1559),
        // which for every entry that declares none is its clearance - the
        // `fit` above, which the placement keeps without the fit.
        let wanted = if settings.fit {
            let (lo, hi) = fit_clamp(settings, entry.role());
            let fill = lot.width.min(lot.depth) / (2.0 * entry.lot_half_width().max(0.5));
            fitted_scale(fill, lo, hi)
        } else {
            1.0
        };
        let key = (slug, scale_e4(wanted));

        // Get-or-build the one shared generator for this entry at this scale.
        let (name, drawn) = if let Some(existing) = by_key.get(&key) {
            existing.clone()
        } else if record.generators.len() >= limits::MAX_GENERATORS {
            // No budget for a new distinct asset. Skip rather than reuse a
            // generator drawn for a different building or a bigger lot.
            report.generator_cap_skips += 1;
            continue;
        } else {
            let tree = grow_generator(entry, did, seed ^ fnv1a_64(slug), character, wanted);
            let name = lot_generator_name(prefix, slug, key.1);
            record.generators.insert(name.clone(), tree);
            by_key.insert(key, (name.clone(), wanted));
            (name, wanted)
        };

        // The placement's scale: unit when the generator carries the fit
        // (#1553). Without the fit it keeps the clamped fit it always wrote
        // - which the compile has never drawn (#1454) - so the record stays
        // what it was.
        let placement_scale = if settings.fit {
            1.0
        } else {
            let (smin, smax) = (
                settings.scale_min.0.min(settings.scale_max.0),
                settings.scale_max.0.max(settings.scale_min.0),
            );
            fit.clamp(smin, smax)
        };
        let half_yaw = lot.yaw * 0.5;
        record.placements.push(Placement::Absolute {
            generator_ref: name,
            transform: TransformData {
                translation: Fp3([lot.position[0], -FOUNDATION_SINK_M, lot.position[1]]),
                // libm (#1132): this rotation is written INTO the record, so
                // it is not merely derived, it is the derivation's output - and
                // #882 was a lots-and-roads desync.
                rotation: Fp4([0.0, libm::sinf(half_yaw), 0.0, libm::cosf(half_yaw)]),
                scale: Fp3([placement_scale, placement_scale, placement_scale]),
            },
            snap_to_terrain: true,
            avoid_water: true,
            // The footprint at the size the building is drawn by its
            // generator; the compile multiplies it by the placement's scale
            // (`world_builder::compile::pad::relocation_clearance`) - unit
            // with the fit, the old clamped fit without it, as it always was.
            // An entry that declares the ground it stands on carries that
            // instead (#1559); every other carries its clearance.
            avoid_water_clearance: Fp(entry.ground_radius().unwrap_or(fp.clearance) * drawn),
            seed: None,
        });
        placed += 1;
    }
    report.placed = placed;
    report
}

/// The pools a ranked lot draws from, first non-empty wins - role by rank
/// and the authored bias (#892), the pools being `[landmark, secondary,
/// prop]`. Each role falls back to the others so a theme missing one role
/// still populates rather than dropping lots. `i` is the lot's rank (0 =
/// the biggest) among the `n` kept.
fn role_order<T>(
    bias: crate::pds::generator::LotTierBias,
    i: usize,
    n: usize,
    [landmark, secondary, prop]: [&[T]; 3],
) -> [&[T]; 3] {
    use crate::pds::generator::LotTierBias;
    match bias {
        // Historical mix: lot 0 landmark, next ~20% secondary. An unknown
        // (newer) bias grows this, so an older client still builds a town.
        LotTierBias::Balanced | LotTierBias::Unknown => {
            if i == 0 {
                [landmark, secondary, prop]
            } else if i * 5 < n {
                [secondary, prop, landmark]
            } else {
                [prop, secondary, landmark]
            }
        }
        // Top ~10% landmarks, next ~30% secondary.
        LotTierBias::Monumental => {
            if i * 10 < n.max(1) || i == 0 {
                [landmark, secondary, prop]
            } else if i * 10 < n * 4 {
                [secondary, prop, landmark]
            } else {
                [prop, secondary, landmark]
            }
        }
        // No landmarks: dwellings on the top third, props below.
        LotTierBias::Residential => {
            if i * 3 < n {
                [secondary, prop, prop]
            } else {
                [prop, secondary, secondary]
            }
        }
        LotTierBias::PropsOnly => [prop, prop, prop],
        // A city (#1555): top ~15% (at least one) landmarks, the rest
        // secondary. Props come only when the theme has no building to
        // offer for a rank: a theme with no secondary at its tier fills the
        // tail with props rather than standing one landmark on every lot
        // (a Pirate district at a low prosperity put the same landmark on
        // all 68 of its lots).
        LotTierBias::Downtown => {
            if i == 0 || i * 100 < n * 15 {
                [landmark, secondary, prop]
            } else {
                [secondary, prop, landmark]
            }
        }
    }
}

/// Inject street-furniture props (#893) at the extracted spots,
/// deterministic in DID + seed: theme Prop-role entries, one shared
/// generator per slug (#454 dedup), a placement per spot facing the road.
/// Returns the number planted.
fn inject_street_furniture(
    record: &mut RoomRecord,
    spots: &[crate::urban::FurnitureSpot],
    did: &str,
    seed: u64,
    prefix: &str,
    settings: &LotSettings,
) -> InjectReport {
    let mut report = InjectReport {
        found: spots.len(),
        kept: spots.len(),
        ..InjectReport::default()
    };
    let scene = SceneCharacter::for_seed(fnv1a_64(did));
    // Same theme resolution as the buildings (#892 override honoured), with
    // the prop pool falling back to the guaranteed-populated theme.
    let base_theme = ThemeArchetype::ALL
        .into_iter()
        .find(|t| {
            t.label()
                .eq_ignore_ascii_case(settings.theme_override.trim())
        })
        .unwrap_or(scene.theme);
    // The buildings' prosperity and escalation (#1555): a network told to
    // be peaceful does not line its streets with barricades.
    let character = lot_character(&scene, settings);
    let mut pool = pool_for(base_theme, StructureRole::Prop, character.0, character.1);
    if pool.is_empty() {
        pool = pool_for(
            FALLBACK_THEME,
            StructureRole::Prop,
            character.0,
            character.1,
        );
    }
    if pool.is_empty() {
        return report;
    }

    let (cap, capped_by_budget) = placement_cap(MAX_FURNITURE_PROPS, record);
    report.dropped_to_cap = spots.len().saturating_sub(cap);
    report.capped_by_budget = capped_by_budget && report.dropped_to_cap > 0;
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ FURNITURE_STREAM_SALT);
    let mut by_slug: HashMap<&'static str, String> = HashMap::new();
    let mut placed = 0usize;
    for spot in spots.iter().take(cap) {
        let entry = pool[(rng.next_u32() as usize) % pool.len()];
        let slug = entry.slug();
        let name = if let Some(existing) = by_slug.get(slug) {
            existing.clone()
        } else if record.generators.len() >= limits::MAX_GENERATORS {
            report.generator_cap_skips += 1;
            continue;
        } else {
            let entry_seed = seed ^ fnv1a_64(slug) ^ FURNITURE_STREAM_SALT;
            // Street props stand at their authored size: scale 1.0.
            let tree = grow_generator(entry, did, entry_seed, character, 1.0);
            let name = format!("{prefix}{slug}");
            record.generators.insert(name.clone(), tree);
            by_slug.insert(slug, name.clone());
            name
        };
        let half_yaw = spot.yaw * 0.5;
        record.placements.push(Placement::Absolute {
            generator_ref: name,
            transform: TransformData {
                translation: Fp3([spot.position[0], -FURNITURE_SINK_M, spot.position[1]]),
                // libm (#1132), same reason as the lot rotation above.
                rotation: Fp4([0.0, libm::sinf(half_yaw), 0.0, libm::cosf(half_yaw)]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: Fp(entry.footprint().clearance),
            seed: None,
        });
        placed += 1;
    }
    report.placed = placed;
    report
}

/// Every active road-derived-content config (#895): enabled, and at least
/// one layer (buildings or furniture) opted in. Paired with its network
/// (child) index so generator names stay per-district.
fn active_configs(record: &RoomRecord) -> Vec<(usize, RoadConfig)> {
    crate::pds::find_road_configs(record)
        .into_iter()
        .enumerate()
        .filter(|(_, c)| c.enabled && (c.populate_lots || c.furniture.enabled))
        .map(|(i, c)| (i, c.clone()))
        .collect()
}

/// Which of network `net`'s layers the record carries content for:
/// `(lot buildings, street props)`, by their per-network seed prefixes.
fn layer_content(record: &RoomRecord, net: usize, c: &RoadConfig) -> (bool, bool) {
    let has = |base: &str| {
        let prefix = net_prefix(base, net, c.seed);
        record.generators.keys().any(|k| k.starts_with(&prefix))
    };
    (has(LOT_PREFIX), has(FURNITURE_PREFIX))
}

/// What growing every active network afresh would plant, network by network,
/// as `(lot buildings, street props)`: the repopulate below run into a
/// stripped scratch copy - every network in order, against the ONE shared
/// placement and generator budget, as the real one grows them.
fn simulate_districts(
    record: &RoomRecord,
    heightmap: &bevy_symbios_ground::HeightMap,
    did: &str,
    configs: &[(usize, RoadConfig)],
    water: Option<f32>,
) -> Vec<(usize, usize)> {
    let mut scratch = record.clone();
    strip_lot_buildings(&mut scratch);
    configs
        .iter()
        .map(|(i, c)| {
            let mut lots_planted = 0;
            if c.populate_lots {
                let lots = crate::urban::extract_building_lots(heightmap, c, water);
                if !lots.is_empty() {
                    let prefix = net_prefix(LOT_PREFIX, *i, c.seed);
                    lots_planted = inject_lots(
                        &mut scratch,
                        &lots,
                        did,
                        c.seed,
                        &prefix,
                        &c.lots,
                        c.tidies_layout(),
                    )
                    .placed;
                }
            }
            let mut props_planted = 0;
            if c.furniture.enabled {
                let spots = crate::urban::extract_furniture_spots(heightmap, c, water);
                let prefix = net_prefix(FURNITURE_PREFIX, *i, c.seed);
                props_planted =
                    inject_street_furniture(&mut scratch, &spots, did, c.seed, &prefix, &c.lots)
                        .placed;
            }
            (lots_planted, props_planted)
        })
        .collect()
}

/// Whether the record carries every active network's district, for a load
/// to adopt (#1553: the critic's second finding and the end review's first
/// two). Each layer a network grows - its lot buildings, its street props -
/// is complete when the record holds content for it OR growing it afresh
/// would plant nothing there: density 0, a layout that encloses no block,
/// an empty pool, a budget the networks before it spend. Counting such a
/// layer missing stripped and regrew EVERY network on every login, over the
/// record just saved. The growth is simulated only when some layer carries
/// nothing, and then in the real order against the real budget.
fn districts_complete(
    record: &RoomRecord,
    heightmap: &bevy_symbios_ground::HeightMap,
    did: &str,
    configs: &[(usize, RoadConfig)],
    water: Option<f32>,
) -> bool {
    // Per network, which layers it grows and the record lacks.
    let missing: Vec<(bool, bool)> = configs
        .iter()
        .map(|(i, c)| {
            let (lots, props) = layer_content(record, *i, c);
            (c.populate_lots && !lots, c.furniture.enabled && !props)
        })
        .collect();
    if missing.iter().all(|m| *m == (false, false)) {
        return true;
    }
    simulate_districts(record, heightmap, did, configs, water)
        .into_iter()
        .zip(missing)
        .all(|((lots, props), (lots_missing, props_missing))| {
            (!lots_missing || lots == 0) && (!props_missing || props == 0)
        })
}

/// Grow into `record` every road layer it carries no content for, as a
/// visitor's client grows it on load (#1554): each active network's lot
/// buildings when it holds none of them, and its street props when it holds
/// none of those, from the room's own scene and the record's own heightmap.
/// The triangle report counts what a client DRAWS, and a network not yet
/// saved with its buildings draws them all the same; a layer the record
/// carries is never grown twice. Returns how many it planted.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn grow_missing_districts(
    record: &mut RoomRecord,
    terrain: &FinishedHeightMap,
    did: &str,
) -> usize {
    // The water line as drawn: Berlin's in a geodata region (#1586).
    let water = crate::world_builder::compile::drawn_water_level(record, Some(terrain));
    let heightmap = &terrain.0;
    let mut planted = 0;
    for (i, c) in active_configs(record) {
        let (has_lots, has_props) = layer_content(record, i, &c);
        if c.populate_lots && !has_lots {
            let lots = crate::urban::extract_building_lots(heightmap, &c, water);
            let prefix = net_prefix(LOT_PREFIX, i, c.seed);
            planted += inject_lots(
                record,
                &lots,
                did,
                c.seed,
                &prefix,
                &c.lots,
                c.tidies_layout(),
            )
            .placed;
        }
        if c.furniture.enabled && !has_props {
            let spots = crate::urban::extract_furniture_spots(heightmap, &c, water);
            let prefix = net_prefix(FURNITURE_PREFIX, i, c.seed);
            planted +=
                inject_street_furniture(record, &spots, did, c.seed, &prefix, &c.lots).placed;
        }
    }
    planted
}

/// Whether the heightmap present is the one `record`'s terrain builds: its
/// [`HeightMapSource`] names the same terrain, or there is none to read (a
/// heightmap inserted directly, by the render tool or a test).
fn heightmap_is_current(record: &RoomRecord, source: Option<&HeightMapSource>) -> bool {
    source.is_none_or(|s| s.0 == super::terrain_source_key(record))
}

/// Populate the road network's lots with buildings when the heightmap or record
/// changes, writing them into the live record (which recompiles + flags dirty).
/// Idempotent per layout seed; sweeps stale buildings on re-roll / toggle-off.
#[allow(clippy::too_many_arguments)] // Bevy system: each arg is a distinct resource.
pub(super) fn maybe_populate_lots(
    mut record: ResMut<LiveRoomRecord>,
    did: Option<Res<CurrentRoomDid>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut undo_signals: ResMut<crate::state::RoomWriteSignals>,
    mut stats: ResMut<super::RoadPanelStats>,
    time: Res<Time>,
    // Session-side layout fingerprint (#882): `None` until the first
    // decision this run, cleared when the network deactivates.
    mut session_fp: Local<Option<String>>,
    // Trailing re-derive debounce (#884): lot extraction re-traces the
    // whole street graph, so a spacing-slider drag must cost one
    // re-derive on release, not one per tick - the same cadence as the
    // road re-mesh.
    mut due: Local<Option<f64>>,
    // Whether the armed re-derive was driven by the ground moving rather
    // than by a layout edit (#1245 f381). The deadline re-asks the pure
    // question against the current record, and a terrain-driven repopulate
    // has an unchanged fingerprint by construction, so it would answer
    // "Skip" and the buildings would stay on the old street plan.
    mut armed_by_terrain: Local<bool>,
    // The room the session state above belongs to (#1553, the critic's
    // first finding). A portal swaps the record in place, so without it the
    // last room's fingerprint met the new room's record, called it a layout
    // edit and regrew the saved district.
    mut session_did: Local<Option<String>>,
    // The terrain the session's last decision was made on (#1553, the end
    // review's third finding): a heightmap ARRIVING is a terrain edit only
    // when it builds another terrain. Logging out and back into the same
    // room brought the same ground back, and the old rule - any heightmap
    // change once a decision stood - regrew the saved district.
    mut session_terrain: Local<Option<String>>,
    source: Option<Res<HeightMapSource>>,
    mut toasts: ResMut<crate::notify::Toasts>,
) {
    let Some(heightmap) = heightmap else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let did_str = did.as_ref().map_or("", |d| d.0.as_str());

    // Another room is a fresh session: its saved buildings are adopted, as
    // on a login, never taken for a layout edit of the room left behind.
    if session_did.as_deref() != Some(did_str) {
        *session_did = Some(did_str.to_owned());
        *session_fp = None;
        *due = None;
        *armed_by_terrain = false;
        *session_terrain = None;
        stats.scene = None;
        // A re-derive armed in the room left behind is not this room's.
        stats.pending = false;
    }
    // The room's own character, for the editor to arm an override at
    // (#1555). Written only when missing, so an unchanged value never
    // stamps the readout changed.
    if stats.scene.is_none() {
        let own = SceneCharacter::for_seed(fnv1a_64(did_str));
        stats.scene = Some((own.prosperity, own.escalation));
    }

    // 1 - change detection decides + arms. Sweeps (network gone) stay
    // immediate: a toggle isn't a drag storm and leaving stale buildings
    // up for the debounce window would flash them at the old layout.
    // A terrain edit moves the ground the lots were extracted FROM (#1245
    // f381). `layout_fingerprint` carries no terrain term - deliberately,
    // since ribbon dims must not churn buildings - so a heightmap change
    // left the fingerprint identical, took the Skip arm, and left every
    // grown building standing on the previous street plan while
    // `maybe_rebuild_roads` re-traced the streets onto the new one. The
    // placements carry `snap_to_terrain`, so their Y re-seats and the
    // failure is XZ-only, which is what makes it read as a content bug
    // rather than a staleness one.
    //
    // Gated on the session having already decided something: on the FIRST
    // heightmap of a session there is nothing stale to replace, and forcing
    // a repopulate there would destroy the buildings a loaded record
    // carries - which is the whole of the `Adopt` arm.
    let terrain = super::terrain_source_key(&record.0);
    let terrain_moved = heightmap.is_changed()
        && session_terrain.is_some()
        && *session_terrain != terrain
        && heightmap_is_current(&record.0, source.as_deref());
    if heightmap.is_changed() || record.is_changed() {
        let configs = active_configs(&record.0);
        if configs.is_empty() {
            *session_fp = None;
            *due = None;
            if record
                .0
                .generators
                .keys()
                .any(|k| k.starts_with(LOT_PREFIX) || k.starts_with(FURNITURE_PREFIX))
            {
                // Derived write (#862): fold the sweep into the edit that
                // disabled the network, not a phantom undo entry of its own.
                undo_signals.derived = true;
                stats.last_replaced = strip_lot_buildings(&mut record.0);
                stats.buildings = 0;
                stats.props = 0;
                stats.pending = false;
            }
            return;
        }
        // Decide nothing on another terrain's ground: right after a portal
        // or a terrain edit the old heightmap is still here. The new one's
        // arrival is a heightmap change, which brings this branch back.
        if !heightmap_is_current(&record.0, source.as_deref()) {
            return;
        }
        let water = crate::world_builder::compile::drawn_water_level(&record.0, Some(&heightmap));
        let fp = combined_fingerprint(
            did_str,
            &configs.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
            water,
        );
        // Only a fresh session asks whether the record is complete (the
        // pure `lot_action` reads it for nothing else); a layer that grows
        // nothing is as complete as one whose buildings are saved.
        let populated = session_fp.is_some()
            || districts_complete(&record.0, &heightmap.0, did_str, &configs, water);
        let action = if terrain_moved {
            LotAction::Repopulate
        } else {
            lot_action(populated, session_fp.as_deref(), &fp)
        };
        match action {
            // Layout matches the standing buildings - also cancels a
            // pending re-derive when an undo walked the edit back.
            LotAction::Skip => {
                *due = None;
                *session_terrain = terrain.clone();
            }
            LotAction::Adopt => {
                *session_fp = Some(fp);
                *session_terrain = terrain.clone();
                *due = None;
                // Readout (#888): count the adopted (saved) content
                // so a loaded room doesn't report zero.
                stats.buildings = record
                    .0
                    .placements
                    .iter()
                    .filter(|p| refs_lot_building(p))
                    .count();
                stats.props = record
                    .0
                    .placements
                    .iter()
                    .filter(|p| refs_street_prop(p))
                    .count();
            }
            LotAction::Repopulate => {
                *due = Some(now + super::roads::ROAD_EDIT_DEBOUNCE_SECS);
                *armed_by_terrain = terrain_moved;
                // The readout stops asserting the previous layout's numbers
                // as settled fact from the moment the re-derive is armed
                // (#1245 f385).
                stats.pending = true;
            }
        }
    }

    // 2 - deadline reached: re-evaluate against the CURRENT record (edits
    // inside the debounce window fold in) and repopulate if still needed.
    if !due.is_some_and(|d| now >= d) {
        return;
    }
    // Never grow a district on another terrain's ground: keep the deadline
    // armed until the record's own heightmap is here.
    if !heightmap_is_current(&record.0, source.as_deref()) {
        return;
    }
    *due = None;
    let armed_by_terrain_now = std::mem::take(&mut *armed_by_terrain);
    let configs = active_configs(&record.0);
    if configs.is_empty() {
        stats.pending = false;
        return; // the change branch above already swept
    }
    let water = crate::world_builder::compile::drawn_water_level(&record.0, Some(&heightmap));
    let fp = combined_fingerprint(
        did_str,
        &configs.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        water,
    );
    let populated = session_fp.is_some()
        || districts_complete(&record.0, &heightmap.0, did_str, &configs, water);
    // The deadline re-evaluates against the CURRENT record, so it asks the
    // pure question again rather than trusting the arming decision - but a
    // terrain-driven repopulate has to survive that re-ask, and its
    // fingerprint is by construction unchanged.
    if !armed_by_terrain_now
        && lot_action(populated, session_fp.as_deref(), &fp) != LotAction::Repopulate
    {
        stats.pending = false;
        return;
    }

    // A changed layout (re-roll, spacing / extent edit, terrain edit) or
    // none yet: clear stale, then repopulate every active network (#895).
    //
    // Everything below writes through `bypass_change_detection` and the
    // record is marked changed ONCE, at the end, and only if this pass
    // actually removed or planted something (#1245 f376). Taking the
    // `ResMut` DerefMut unconditionally is what made a layout that grows
    // nothing cost a whole-room broadcast per unrelated edit;
    // `strip_lot_buildings` already returned the bool that answers it, and
    // the room editor's own flush (`ui/room/mod.rs`) is the pattern.
    let record_mut = record.bypass_change_detection();
    let stripped = strip_lot_buildings(&mut record_mut.0);
    stats.last_replaced = stripped;
    *session_fp = Some(fp);
    stats.buildings = 0;
    stats.props = 0;
    stats.clamps = super::LotClamps::default();
    for (i, config) in &configs {
        if config.populate_lots {
            let lots = crate::urban::extract_building_lots(&heightmap.0, config, water);
            if !lots.is_empty() {
                let report = inject_lots(
                    &mut record_mut.0,
                    &lots,
                    did_str,
                    config.seed,
                    &net_prefix(LOT_PREFIX, *i, config.seed),
                    &config.lots,
                    config.tidies_layout(),
                );
                stats.buildings += report.placed;
                // The arithmetic behind the number (#1211).
                stats.clamps.lots_found += report.found;
                stats.clamps.lots_kept += report.kept;
                stats.clamps.lots_too_small += report.too_small;
                stats.clamps.buildings_dropped += report.dropped_to_cap;
                stats.clamps.buildings_capped_by_budget |= report.capped_by_budget;
                stats.clamps.generator_cap_skips += report.generator_cap_skips;
            }
        }
        // Street furniture (#893) - independent of the building layer.
        if config.furniture.enabled {
            let spots = crate::urban::extract_furniture_spots(&heightmap.0, config, water);
            let report = inject_street_furniture(
                &mut record_mut.0,
                &spots,
                did_str,
                config.seed,
                &net_prefix(FURNITURE_PREFIX, *i, config.seed),
                &config.lots,
            );
            stats.props += report.placed;
            stats.clamps.props_dropped += report.dropped_to_cap;
            stats.clamps.props_capped_by_budget |= report.capped_by_budget;
            stats.clamps.generator_cap_skips += report.generator_cap_skips;
        }
    }
    stats.pending = false;

    // ONE `set_changed`, and only if this pass actually moved something
    // (#1245 f376). A layout that grows nothing now costs nothing: no
    // placement-fingerprint pass, no whole-room broadcast.
    let planted = stats.buildings + stats.props;
    if stripped > 0 || planted > 0 {
        record.set_changed();
    }

    // Say what was replaced (#1245 f378). The strip runs a third of a
    // second after the drag ends, when the owner's attention has already
    // moved on, and it removes every grown building INCLUDING ones they
    // dragged into place with the gizmo - from a control that reads as
    // cosmetic. Undo covers it (the derived write folds into the slider's
    // own entry), but only if they realise in time, and nothing told them.
    if stripped > 0 {
        toasts.info(
            format!(
                "Re-grew the district: {stripped} grown objects replaced by {planted}. \
                 Undo restores them with the edit that caused it."
            ),
            now,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::generator::GeneratorKind;
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
        let fp = |c: &RoadConfig| layout_fingerprint("did:test:0", c, Some(7.0));

        let mut spacing = base.clone();
        spacing.major_spacing.0 += 10.0;
        assert_ne!(fp(&base), fp(&spacing), "spacing edits move lots");

        let mut extent = base.clone();
        extent.district_half_extent.0 += 25.0;
        assert_ne!(fp(&base), fp(&extent), "extent edits move lots");

        let mut seeded = base.clone();
        seeded.seed ^= 1;
        assert_ne!(fp(&base), fp(&seeded), "re-roll moves lots");

        let mut centered = base.clone();
        centered.center.0 = [40.0, -25.0];
        assert_ne!(fp(&base), fp(&centered), "centre offset moves lots (#889)");

        let mut styled = base.clone();
        styled.style = crate::pds::generator::RoadStyle::Grid;
        assert_ne!(fp(&base), fp(&styled), "style change moves lots (#890)");

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
            layout_fingerprint("did:test:1", &base, Some(7.0)),
            "fingerprint is per-room"
        );

        let mut shore = base.clone();
        shore.avoid_water = true;
        assert_ne!(
            fp(&base),
            fp(&shore),
            "stopping streets at the water moves lots (#1552)"
        );

        // The street field (#1556): its smoothing and basis fields move the
        // streets, and its keep-out discs drop lots - each edit its own key.
        use crate::pds::generator::{RoadBasis, RoadKeepOut};
        const AT: crate::pds::types::Fp2 = crate::pds::types::Fp2([10.0, -20.0]);
        let field_edits: [fn(&mut RoadConfig); 6] = [
            |c| c.field.smoothing.0 = 12.0,
            |c| c.field.terrain_weight.0 = 0.5,
            |c| c.field.basis.push(RoadBasis::ring_at(AT)),
            |c| c.field.basis.push(RoadBasis::grid_at(AT)),
            |c| {
                c.field.basis.push(RoadBasis::Grid {
                    center: AT,
                    bearing: Fp(30.0),
                    radius: Fp(RoadBasis::DEFAULT_RADIUS),
                    strength: Fp(RoadBasis::DEFAULT_STRENGTH),
                })
            },
            |c| c.field.keep_out.push(RoadKeepOut::at(AT)),
        ];
        let mut seen = vec![fp(&base)];
        for edit in field_edits {
            let mut edited = base.clone();
            edit(&mut edited);
            let key = fp(&edited);
            assert!(
                !seen.contains(&key),
                "a street field edit must move the lots (#1556): {:?}",
                edited.field
            );
            seen.push(key);
        }
    }

    /// #1552: the water line moves the streets of a network that avoids
    /// water, so raising the lake must re-derive its lots - and must leave
    /// a network that ignores the water alone, since its streets did not
    /// move and churning them would replace buildings the owner placed.
    #[test]
    fn the_water_line_keys_only_the_lots_of_a_network_that_avoids_water() {
        let dry_traced = RoadConfig::default();
        assert_eq!(
            layout_fingerprint("did:test:0", &dry_traced, Some(7.0)),
            layout_fingerprint("did:test:0", &dry_traced, Some(9.5)),
            "a network that ignores the water keeps its lots when the lake rises"
        );
        assert_eq!(
            layout_fingerprint("did:test:0", &dry_traced, Some(7.0)),
            layout_fingerprint("did:test:0", &dry_traced, None),
            "or when the lake goes"
        );

        let shore = RoadConfig {
            avoid_water: true,
            ..RoadConfig::default()
        };
        assert_ne!(
            layout_fingerprint("did:test:0", &shore, Some(7.0)),
            layout_fingerprint("did:test:0", &shore, Some(9.5)),
            "a network that stops at the shore re-derives when the lake rises"
        );
        assert_ne!(
            layout_fingerprint("did:test:0", &shore, Some(7.0)),
            layout_fingerprint("did:test:0", &shore, None),
            "and when the lake goes"
        );
    }

    /// #1251 f390: the editor's Theme combo and the injector must agree on
    /// what a stored label means, or the panel asserts a setting that has no
    /// effect. This is the predicate both now ask.
    #[test]
    fn a_lot_theme_override_resolves_the_same_way_for_the_panel_and_the_injector() {
        let known = ThemeArchetype::ALL[0].label();
        assert_eq!(resolve_lot_theme(known), Some(ThemeArchetype::ALL[0]));
        // Lenient by design (#892): case and surrounding space do not
        // matter, and the panel must be lenient in exactly the same places.
        assert_eq!(
            resolve_lot_theme(&format!("  {}  ", known.to_ascii_uppercase())),
            Some(ThemeArchetype::ALL[0])
        );
        // Empty is "room theme", not a failure - the panel prints no
        // warning for it.
        assert_eq!(resolve_lot_theme(""), None);
        assert_eq!(resolve_lot_theme("   "), None);
        // A label from a newer build resolves to nothing, which is what the
        // panel now says out loud instead of showing it as the selection.
        assert_eq!(resolve_lot_theme("Steampunk Deluxe"), None);
    }

    #[test]
    fn lot_action_contract() {
        let fp = "did|1|170|95|55";
        let other = "did|1|170|105|55";
        // Nothing built yet, and nothing derived yet → populate.
        assert_eq!(lot_action(false, None, fp), LotAction::Repopulate);
        // Nothing built AND this exact layout already derived → SKIP
        // (#1245 f376). "We ran this layout and it produced nothing" is a
        // terminal state: density 0, a spacing wider than its own extent,
        // an empty theme pool, a spent placement budget. Before this it
        // returned Repopulate forever, so every unrelated edit to the
        // record armed another strip-that-removes-nothing and still
        // dirtied it - one placement-fingerprint pass and one whole-room
        // broadcast to every guest, a third of a second after each edit.
        assert_eq!(lot_action(false, Some(fp), fp), LotAction::Skip);
        // But a DIFFERENT layout that has grown nothing yet still runs.
        assert_eq!(lot_action(false, Some(other), fp), LotAction::Repopulate);
        // Built + matching session fingerprint → leave alone.
        assert_eq!(lot_action(true, Some(fp), fp), LotAction::Skip);
        // Built + differing fingerprint (spacing edit, same seed) → rebuild.
        assert_eq!(lot_action(true, Some(other), fp), LotAction::Repopulate);
        // Built + fresh session (a load): trust the saved buildings, adopt.
        assert_eq!(lot_action(true, None, fp), LotAction::Adopt);
    }

    /// #1245 f382. The prefix IS the idempotency key, and the editor now
    /// asks this before offering Rename - and before letting anyone NAME a
    /// generator into the namespace, where the next layout edit would
    /// delete it.
    #[test]
    fn the_derived_namespace_is_recognisable_from_the_key_alone() {
        assert!(is_derived_generator_key(&format!(
            "{LOT_PREFIX}0_4242_hall"
        )));
        assert!(is_derived_generator_key(&format!(
            "{FURNITURE_PREFIX}0_4242_lamp"
        )));
        assert!(!is_derived_generator_key("oak"));
        assert!(!is_derived_generator_key("my_lot_building"));
        // And it agrees with the strip, which is the thing it is
        // protecting: anything the predicate calls derived is exactly what
        // `strip_lot_buildings` removes.
        let mut record = RoomRecord::default_for_did(&urban_did());
        record.generators.clear();
        record.placements.clear();
        record.generators.insert(
            format!("{LOT_PREFIX}0_1_hall"),
            crate::pds::Generator::default(),
        );
        record
            .generators
            .insert("oak".to_string(), crate::pds::Generator::default());
        strip_lot_buildings(&mut record);
        assert_eq!(
            record.generators.keys().collect::<Vec<_>>(),
            vec!["oak"],
            "the strip removes exactly what the predicate names"
        );
    }

    /// #1211, finding 384. Sequence: a room already carrying most of its
    /// placement budget grows a district of 20 lots; the injector filled
    /// what was left and reported only the smaller number. The report now
    /// carries the arithmetic - found, kept, dropped, and WHICH cap bit -
    /// so the Lots readout can say "only N placements were left".
    #[test]
    fn the_inject_report_names_the_cap_that_emptied_the_lots() {
        let did = urban_did();
        let mut record = RoomRecord::default_for_did(&did);
        let free = 3;
        while record.placements.len() < limits::MAX_PLACEMENTS - free {
            record.placements.push(Placement::Absolute {
                generator_ref: String::from("filler"),
                transform: TransformData::default(),
                snap_to_terrain: false,
                avoid_water: false,
                avoid_water_clearance: Fp(0.0),
                seed: None,
            });
        }
        let lots: Vec<BuildingLot> = (0..20)
            .map(|i| lot(i as f32 * 8.0, 0.0, 12.0, 14.0))
            .collect();
        let report = inject_lot_buildings(
            &mut record,
            &lots,
            &did,
            4242,
            &seed_prefix(4242),
            &Default::default(),
        );
        assert_eq!(report.found, 20);
        assert_eq!(report.kept, 20, "density 1.0 keeps every lot");
        assert_eq!(report.placed, free);
        assert_eq!(report.dropped_to_cap, 20 - free);
        assert!(
            report.capped_by_budget,
            "the placement budget, not the per-district constant, is what bit"
        );
        assert_eq!(record.placements.len(), limits::MAX_PLACEMENTS);
        assert!(
            record.placements.iter().any(is_road_grown),
            "the grown placements are attributable (#1211, finding 394)"
        );
    }

    #[test]
    fn inject_places_buildings_and_strip_removes_them() {
        let did = urban_did();
        let mut record = RoomRecord::default_for_did(&did);
        let before_gens = record.generators.len();
        let lots: Vec<BuildingLot> = (0..20)
            .map(|i| lot(i as f32 * 8.0, 0.0, 12.0, 14.0))
            .collect();

        let n = inject_lot_buildings(
            &mut record,
            &lots,
            &did,
            4242,
            &seed_prefix(4242),
            &Default::default(),
        )
        .placed;
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

        // The count is what the toast and the panel print (#1245 f378):
        // how many objects visibly disappear.
        let placements_before = record.placements.len();
        let removed = strip_lot_buildings(&mut record);
        assert!(removed > 0, "the strip removed nothing");
        assert_eq!(
            removed,
            placements_before - record.placements.len(),
            "the reported count must be the placements that actually went"
        );
        assert_eq!(record.generators.len(), before_gens, "strip must be exact");
        assert!(!record.placements.iter().any(refs_lot_building));
        assert_eq!(
            strip_lot_buildings(&mut record),
            0,
            "second strip is a no-op"
        );
    }

    #[test]
    fn injection_is_bounded_deduped_and_deterministic() {
        let did = urban_did();
        let lots: Vec<BuildingLot> = (0..400).map(|i| lot(i as f32, 0.0, 10.0, 10.0)).collect();

        let mut a = RoomRecord::default_for_did(&did);
        let mut b = RoomRecord::default_for_did(&did);
        let before = a.generators.len();
        let na = inject_lot_buildings(&mut a, &lots, &did, 7, &seed_prefix(7), &Default::default())
            .placed;
        let nb = inject_lot_buildings(&mut b, &lots, &did, 7, &seed_prefix(7), &Default::default())
            .placed;
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

    /// The catalogue entry a lot generator key grows.
    fn entry_of(key: &str, prefix: &str) -> &'static dyn CatalogueEntry {
        let slug = key
            .strip_prefix(prefix)
            .and_then(|rest| rest.split('@').next())
            .unwrap_or_else(|| panic!("{key} is not under {prefix}"));
        crate::catalogue::by_slug(slug).unwrap_or_else(|| panic!("{slug} is no entry"))
    }

    /// Every lot placement under `prefix`: its generator key, the lot it
    /// stands on (matched by position) and the clearance it carries.
    fn grown<'r>(
        record: &'r RoomRecord,
        lots: &'r [BuildingLot],
        prefix: &str,
    ) -> Vec<(&'r str, &'r BuildingLot, &'r TransformData, f32)> {
        record
            .placements
            .iter()
            .filter_map(|p| match p {
                Placement::Absolute {
                    generator_ref,
                    transform,
                    avoid_water_clearance,
                    ..
                } if generator_ref.starts_with(prefix) => {
                    let at = transform.translation.0;
                    let lot = lots
                        .iter()
                        .find(|l| l.position == [at[0], at[2]])
                        .expect("every placement stands on a lot");
                    Some((
                        generator_ref.as_str(),
                        lot,
                        transform,
                        avoid_water_clearance.0,
                    ))
                }
                _ => None,
            })
            .collect()
    }

    /// The scale a lot draws `entry` at, worked from the lot alone.
    fn drawn_scale(entry: &dyn CatalogueEntry, lot: &BuildingLot, settings: &LotSettings) -> f32 {
        if !settings.fit {
            return 1.0;
        }
        let (lo, hi) = fit_clamp(settings, entry.role());
        let fit = lot.width.min(lot.depth) / (2.0 * entry.lot_half_width().max(0.5));
        fitted_scale(fit, lo, hi)
    }

    /// A DID whose seeded scene `wanted` accepts, found by search so a test
    /// does not hang on one hash.
    fn did_where(wanted: impl Fn(&SceneCharacter) -> bool) -> String {
        (0..10_000)
            .map(|i| format!("did:test:scene:{i}"))
            .find(|did| wanted(&SceneCharacter::for_did(did)))
            .expect("some DID rolls it")
    }

    /// #1553: the fit buckets are quarter octaves, built from a table and
    /// exact powers of two so every peer derives the same bits - the scale
    /// is written into the record and names the generator.
    #[test]
    fn fit_buckets_are_quarter_octaves_with_the_same_bits_on_every_peer() {
        assert_eq!(bucket_scale(0), 1.0);
        assert_eq!(bucket_scale(4), 2.0);
        assert_eq!(bucket_scale(-4), 0.5);
        assert_eq!(bucket_scale(-8), 0.25);
        assert_eq!(bucket_scale(2), std::f32::consts::SQRT_2);
        assert_eq!(bucket_scale(6), 2.0 * std::f32::consts::SQRT_2);
        for k in -16..16 {
            // Every bucket is the f32 nearest 2^(k/4): the table holds the
            // correctly rounded mantissas, and an exact power of two keeps
            // them that way.
            let exact = 2f64.powf(f64::from(k) / 4.0) as f32;
            assert_eq!(
                bucket_scale(k).to_bits(),
                exact.to_bits(),
                "bucket {k}: {} is not the f32 nearest 2^({k}/4), {exact}",
                bucket_scale(k)
            );
            assert!(bucket_scale(k + 1) > bucket_scale(k), "bucket {k}");
        }
    }

    /// #1553: a lot's fit rounds DOWN to a bucket, so a building never
    /// outgrows its lot, and stays inside the clamp - a bound that sits
    /// between buckets is drawn at itself.
    #[test]
    fn a_lot_fit_rounds_down_to_a_quarter_octave_inside_the_clamp() {
        let (lo, hi) = (0.5, 2.0);
        for k in -4..=4 {
            let bucket = bucket_scale(k);
            assert_eq!(fitted_scale(bucket, lo, hi), bucket, "a fit on bucket {k}");
        }
        assert_eq!(fitted_scale(1.18, lo, hi), 1.0, "just under 1.19");
        assert_eq!(fitted_scale(0.999, lo, hi), bucket_scale(-1));
        assert_eq!(fitted_scale(1.95, lo, hi), bucket_scale(3));
        assert_eq!(fitted_scale(7.0, lo, hi), 2.0, "past the ceiling");
        assert_eq!(fitted_scale(0.2, lo, hi), 0.5, "under the floor");
        assert_eq!(
            fitted_scale(1.9, 0.5, 1.8),
            1.8,
            "a ceiling between buckets draws the lots that ask for more at itself"
        );
        assert_eq!(
            fitted_scale(1.75, 0.5, 1.8),
            bucket_scale(3),
            "under that ceiling a fit is still a bucket"
        );
        assert_eq!(
            fitted_scale(0.57, 0.55, 2.0),
            0.55,
            "rounding down past a floor between buckets stops at the floor"
        );
        assert_eq!(fitted_scale(0.62, 0.55, 2.0), bucket_scale(-3));
        for fit in [0.1, 1.0, 1.3, 9.0] {
            assert_eq!(
                fitted_scale(fit, 1.3, 1.3),
                1.3,
                "a pinned clamp, fit {fit}"
            );
        }
        assert_eq!(fitted_scale(f32::NAN, lo, hi), lo, "NaN cannot outgrow");
        assert_eq!(fitted_scale(f32::INFINITY, lo, hi), hi);

        let mut seen: Vec<f32> = Vec::new();
        for i in 0..=4000 {
            let fit = i as f32 * 0.001;
            let scale = fitted_scale(fit, lo, hi);
            assert!((lo..=hi).contains(&scale), "{fit} drew at {scale}");
            assert!(
                scale <= fit || scale == lo,
                "{fit} drew at {scale}, outgrowing its lot"
            );
            if !seen.contains(&scale) {
                seen.push(scale);
            }
        }
        assert_eq!(
            seen.len(),
            9,
            "0.5 to 2.0 is nine quarter-octave buckets: {seen:?}"
        );
    }

    /// #1553: the fit lives in the generator. The world compile draws an
    /// absolute placement at unit scale whatever its record says (#1454),
    /// so the lot buildings drew at 1.0 whatever their lots asked for and
    /// the editor's scale sliders moved nothing. Each lot building's
    /// generator is its entry's own derivation with the root scaled by the
    /// lot's bucket, its placement unit, and its clearance the footprint at
    /// the size it is drawn.
    #[test]
    fn a_lot_buildings_fit_is_baked_into_its_generator_and_placed_at_unit_scale() {
        let did = urban_did();
        let settings = LotSettings {
            theme_override: String::from("Cyberpunk"),
            tier_bias: crate::pds::generator::LotTierBias::Monumental,
            fit: true,
            ..LotSettings::default()
        };
        // Narrow sides from 0.6 m to 59.6 m: fits from under the floor (a
        // footprint is at least 0.5 m in radius) to past the ceiling for
        // every entry the theme grows.
        let lots: Vec<BuildingLot> = (0..60)
            .map(|i| lot(i as f32 * 100.0, 0.0, 0.6 + i as f32, 70.0))
            .collect();
        let mut record = RoomRecord::default_for_did(&did);
        let prefix = seed_prefix(4242);
        let placed = inject_lot_buildings(&mut record, &lots, &did, 4242, &prefix, &settings);
        assert_eq!(placed.placed, lots.len());
        let scene = SceneCharacter::for_did(&did);
        let mut scales_seen: Vec<f32> = Vec::new();
        for (key, lot, transform, clearance) in grown(&record, &lots, &prefix) {
            assert_eq!(
                transform.scale.0, [1.0; 3],
                "{key}: the placement is not what scales it"
            );
            let entry = entry_of(key, &prefix);
            let drawn = drawn_scale(entry, lot, &settings);
            let entry_seed = 4242 ^ fnv1a_64(entry.slug());
            let mut base = entry.build(&did).with_shape_seed(entry_seed);
            material_finish::apply_socio_finish(&mut base, scene.prosperity, scene.escalation);
            ruin::apply_ruin_bounded(
                &mut base,
                scene.escalation,
                entry_seed,
                entry.ruin_max_lean(),
            );
            let generator = &record.generators[key];
            for axis in 0..3 {
                assert!(
                    (generator.transform.scale.0[axis] - base.transform.scale.0[axis] * drawn)
                        .abs()
                        < 1e-5,
                    "{key}: its root is not drawn at {drawn}"
                );
                assert!(
                    (generator.transform.translation.0[axis]
                        - base.transform.translation.0[axis] * drawn)
                        .abs()
                        < 1e-5,
                    "{key}: the root does not scale about the ground"
                );
            }
            assert_eq!(generator.transform.rotation, base.transform.rotation);
            assert_eq!(
                generator.children, base.children,
                "{key}: only the root carries the fit"
            );
            // The ground it stands on at the size it is drawn: its clearance,
            // or the ground radius an entry declares (#1559).
            let ground = entry.ground_radius().unwrap_or(entry.footprint().clearance);
            assert!(
                (clearance - ground * drawn).abs() < 1e-4,
                "{key}: the clearance is not the ground it stands on at its drawn size"
            );
            if !scales_seen.contains(&drawn) {
                scales_seen.push(drawn);
            }
        }
        assert!(
            scales_seen.iter().any(|s| *s < 1.0) && scales_seen.iter().any(|s| *s > 1.0),
            "the fixture must draw buildings both under and over unit scale: {scales_seen:?}"
        );
    }

    /// #1558: at layout revision 1 no building or prop is grown larger
    /// than its lot. The control is revision 0 over the same lots, which
    /// stands buildings at their clamp's floor on lots too small for them -
    /// as every network grew before. With the fit off a building is drawn
    /// at its catalogue size, so a lot grows only what fits it at that size.
    #[test]
    fn at_layout_revision_1_no_building_outgrows_its_lot() {
        let did = urban_did();
        for fit in [true, false] {
            let settings = LotSettings {
                theme_override: String::from("Cyberpunk"),
                tier_bias: crate::pds::generator::LotTierBias::Downtown,
                fit,
                scale_min: Fp(1.0),
                scale_max: Fp(1.8),
                ..LotSettings::default()
            };
            let lots: Vec<BuildingLot> = (0..60)
                .map(|i| lot(i as f32 * 100.0, 0.0, 0.6 + i as f32, 70.0))
                .collect();
            let prefix = seed_prefix(4242);
            let overflow = |record: &RoomRecord| {
                grown(record, &lots, &prefix)
                    .into_iter()
                    .filter(|(key, lot, _, _)| {
                        let entry = entry_of(key, &prefix);
                        let drawn = drawn_scale(entry, lot, &settings);
                        2.0 * entry.lot_half_width() * drawn > lot.width.min(lot.depth) + 1.0e-4
                    })
                    .count()
            };
            let mut before = RoomRecord::default_for_did(&did);
            inject_lots(&mut before, &lots, &did, 4242, &prefix, &settings, false);
            assert!(
                overflow(&before) > 0,
                "the control (fit {fit}): revision 0 outgrows a small lot"
            );
            let mut after = RoomRecord::default_for_did(&did);
            let report = inject_lots(&mut after, &lots, &did, 4242, &prefix, &settings, true);
            assert_eq!(
                overflow(&after),
                0,
                "fit {fit}: a building outgrows its lot"
            );
            assert!(
                report.too_small > 0,
                "fit {fit}: the smallest lots grow nothing"
            );
            assert_eq!(
                report.placed + report.too_small,
                lots.len(),
                "fit {fit}: every other lot still grows a building"
            );
        }
    }

    /// #1558 and #1559, the other way round: at layout revision 1 a lot
    /// that holds a downtown building by its own half side grows it. Read
    /// by the clearance, a spacing circle about twice the building's reach,
    /// the fit asked the megatower for a 46 m lot where its plinth is
    /// 18.4 m, and a fit that strict passes every overflow test. Drawn at
    /// its catalogue size, each of the five fits a lot 30 cm wider than its
    /// widest part above the ground; and a lone lot that size on a rich,
    /// monumental Cyberpunk network grows the megatower.
    #[test]
    fn at_layout_revision_1_a_downtown_building_that_fits_its_lot_is_grown() {
        let settings = LotSettings {
            theme_override: String::from("Cyberpunk"),
            tier_bias: crate::pds::generator::LotTierBias::Monumental,
            fit: false,
            prosperity: Some(Fp(0.9)),
            escalation: Some(Fp(0.0)),
            ..LotSettings::default()
        };
        // A lot just wider than the entry's widest part above the ground,
        // measured from its geometry rather than from what it declares.
        let lot_for = |entry: &dyn CatalogueEntry| {
            let widest = crate::catalogue::items::measure::solids(&entry.build(""))
                .iter()
                .filter(|s| s.bounds.max.y > 0.05)
                .map(|s| {
                    let (a, b) = (s.bounds.min, s.bounds.max);
                    a.x.abs().max(b.x.abs()).max(a.z.abs()).max(b.z.abs())
                })
                .fold(0.0_f32, f32::max);
            lot(0.0, 0.0, 2.0 * widest + 0.3, 70.0)
        };
        for slug in [
            "neon_megatower",
            "data_spire",
            "arcade_block",
            "parking_stack",
            "holo_billboard",
        ] {
            let entry = crate::catalogue::by_slug(slug).expect("a downtown entry");
            let lot = lot_for(entry);
            assert!(
                fits_lot(entry, &lot, &settings),
                "{slug}: a {} m lot does not hold it",
                lot.width
            );
        }
        let did = urban_did();
        let megatower = crate::catalogue::by_slug("neon_megatower").expect("the megatower");
        let lots = vec![lot_for(megatower)];
        let prefix = seed_prefix(4242);
        let mut record = RoomRecord::default_for_did(&did);
        let report = inject_lots(&mut record, &lots, &did, 4242, &prefix, &settings, true);
        assert_eq!(report.placed, 1, "the lot grows a building");
        assert!(
            record
                .generators
                .keys()
                .any(|k| k.starts_with(&prefix) && entry_of(k, &prefix).slug() == "neon_megatower"),
            "a lot that holds the megatower grows something else: {:?}",
            record
                .generators
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .collect::<Vec<_>>()
        );
    }

    /// #1558: the layout revision re-derives the graph and the lots, so it
    /// is part of the lot fingerprint - and a network on the original plan
    /// keys exactly as before the field.
    #[test]
    fn the_layout_revision_re_derives_the_lots() {
        let fp = |layout_revision: u32| {
            layout_fingerprint(
                "did:test:0",
                &RoadConfig {
                    layout_revision,
                    ..RoadConfig::default()
                },
                None,
            )
        };
        assert_ne!(fp(0), fp(1), "an upgrade must regrow the district");
        assert!(
            !fp(0).contains("layout"),
            "the original plan keys as it always did"
        );
    }

    /// #1553: a prop keeps street scale. Its fit is capped at
    /// `min(scale_max, 1.0)` - a planter fitted to a 40 m lot was a giant -
    /// while landmarks and secondaries use the whole authored clamp.
    #[test]
    fn a_prop_keeps_street_scale() {
        let plain = LotSettings::default();
        assert_eq!(fit_clamp(&plain, StructureRole::Landmark), (0.5, 2.0));
        assert_eq!(fit_clamp(&plain, StructureRole::Secondary), (0.5, 2.0));
        assert_eq!(fit_clamp(&plain, StructureRole::Prop), (0.5, 1.0));
        let tight = LotSettings {
            scale_max: Fp(0.8),
            ..LotSettings::default()
        };
        assert_eq!(fit_clamp(&tight, StructureRole::Prop), (0.5, 0.8));
        let big = LotSettings {
            scale_min: Fp(1.5),
            scale_max: Fp(3.0),
            ..LotSettings::default()
        };
        assert_eq!(
            fit_clamp(&big, StructureRole::Prop),
            (1.0, 1.0),
            "a floor above the cap comes down to meet it"
        );
        assert_eq!(fit_clamp(&big, StructureRole::Landmark), (1.5, 3.0));
        let swapped = LotSettings {
            scale_min: Fp(2.0),
            scale_max: Fp(0.5),
            ..LotSettings::default()
        };
        assert_eq!(fit_clamp(&swapped, StructureRole::Secondary), (0.5, 2.0));

        let did = urban_did();
        let settings = LotSettings {
            tier_bias: crate::pds::generator::LotTierBias::PropsOnly,
            ..LotSettings::default()
        };
        let lots: Vec<BuildingLot> = (0..30)
            .map(|i| lot(i as f32 * 50.0, 0.0, 40.0, 40.0))
            .collect();
        let mut record = RoomRecord::default_for_did(&did);
        let prefix = seed_prefix(7);
        let report = inject_lot_buildings(&mut record, &lots, &did, 7, &prefix, &settings);
        assert_eq!(report.placed, lots.len());
        for (key, _, _, clearance) in grown(&record, &lots, &prefix) {
            let entry = entry_of(key, &prefix);
            assert_eq!(entry.role(), StructureRole::Prop, "{key}");
            assert_eq!(
                key,
                format!("{prefix}{}", entry.slug()),
                "a prop on a 40 m lot draws at 1.0, under its plain name"
            );
            assert!((clearance - entry.footprint().clearance).abs() < 1e-5);
        }
    }

    /// #1553: every fitted generator is named under the layer's prefix -
    /// bucket 1.0 exactly as before - so the strip, the populated check and
    /// the editor's reserved-namespace test all still find it, and every
    /// name is a key the record keeps as written.
    #[test]
    fn a_fitted_generators_name_stays_in_the_derived_namespace() {
        let prefix = seed_prefix(4242);
        assert_eq!(
            lot_generator_name(&prefix, "hall", 10_000),
            format!("{prefix}hall"),
            "scale 1.0 keeps the name lot buildings always had"
        );
        for (e4, suffix) in [
            (5_000, "@0.5"),
            (5_946, "@0.5946"),
            (7_071, "@0.7071"),
            (8_409, "@0.8409"),
            (11_892, "@1.1892"),
            (18_000, "@1.8"),
            (20_000, "@2"),
        ] {
            let name = lot_generator_name(&prefix, "hall", e4);
            assert_eq!(name, format!("{prefix}hall{suffix}"));
            assert!(is_derived_generator_key(&name), "{name}");
            assert!(
                !name.contains('/'),
                "{name}: '/' joins a node's cache key to its root's"
            );
            assert_eq!(
                crate::pds::sanitize::names::clean_name(&name, limits::MAX_GENERATOR_NAME_CHARS)
                    .as_deref(),
                Some(name.as_str()),
                "the key sanitiser must keep {name} as written"
            );
        }
        let names: std::collections::HashSet<String> = (-4..=4)
            .map(|k| lot_generator_name(&prefix, "hall", scale_e4(bucket_scale(k))))
            .collect();
        assert_eq!(names.len(), 9, "one name per bucket: {names:?}");

        let mut record = RoomRecord::default_for_did(&urban_did());
        record.generators.clear();
        record.placements.clear();
        let fitted = lot_generator_name(&prefix, "hall", 7_071);
        record
            .generators
            .insert(fitted.clone(), crate::pds::Generator::default());
        record.placements.push(Placement::Absolute {
            generator_ref: fitted,
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: Fp(1.0),
            seed: None,
        });
        let config = RoadConfig {
            seed: 4242,
            ..RoadConfig::default()
        };
        assert_eq!(layer_content(&record, 0, &config), (true, false));
        assert!(record.placements.iter().all(is_road_grown));
        assert_eq!(strip_lot_buildings(&mut record), 1);
        assert!(record.generators.is_empty() && record.placements.is_empty());
    }

    /// #1555: Downtown puts landmarks on the top ~15% of the ranked lots (at
    /// least one) and secondaries on the rest. Where a rank's own role is
    /// empty the tail falls back to a prop before a landmark (the critic of
    /// #1553 found a Pirate district with no secondary at its tier standing
    /// one landmark on all 68 lots), and the head to a secondary first.
    #[test]
    fn downtown_ranks_landmarks_then_secondaries_with_props_last() {
        use crate::pds::generator::LotTierBias;
        let pools: [&[u8]; 3] = [&[0], &[1], &[2]];
        for n in [1usize, 2, 6, 7, 10, 20, 100, 160, 256] {
            let first: Vec<u8> = (0..n)
                .map(|i| role_order(LotTierBias::Downtown, i, n, pools)[0][0])
                .collect();
            let landmarks = (n * 15).div_ceil(100).max(1);
            assert!(
                first[..landmarks].iter().all(|r| *r == 0),
                "n={n}: {first:?}"
            );
            assert!(
                first[landmarks..].iter().all(|r| *r == 1),
                "n={n}: {first:?}"
            );
            for i in 0..n {
                let order = role_order(LotTierBias::Downtown, i, n, pools);
                let want: [u8; 3] = if i < landmarks { [0, 1, 2] } else { [1, 2, 0] };
                assert_eq!(
                    [order[0][0], order[1][0], order[2][0]],
                    want,
                    "n={n} lot {i}: a landmark lot falls back to a secondary, \
                     a secondary lot to a prop before a landmark"
                );
            }
        }
    }

    /// #1555: on a real district, Downtown grows no prop while the theme
    /// offers a secondary building at its tier (where it offers none, the
    /// tail takes props: `downtown_without_a_secondary_fills_its_tail_with_props`),
    /// and at least one landmark.
    #[test]
    fn downtown_grows_no_prop_while_the_theme_has_a_secondary() {
        use crate::pds::generator::LotTierBias;
        let did = urban_did();
        let scene = SceneCharacter::for_did(&did);
        let lots: Vec<BuildingLot> = (0..40)
            .map(|i| lot(i as f32 * 60.0, 0.0, 20.0 + i as f32, 30.0))
            .collect();
        let mut both_roles = 0;
        for theme in ThemeArchetype::ALL {
            let settings = LotSettings {
                theme_override: theme.label().to_string(),
                tier_bias: LotTierBias::Downtown,
                ..LotSettings::default()
            };
            let grows = if entries_for(theme, StructureRole::Landmark).next().is_some() {
                theme
            } else {
                FALLBACK_THEME
            };
            let (prosperity, escalation) = lot_character(&scene, &settings);
            let offers = |role| !pool_for(grows, role, prosperity, escalation).is_empty();
            let mut record = RoomRecord::default_for_did(&did);
            let prefix = seed_prefix(99);
            inject_lot_buildings(&mut record, &lots, &did, 99, &prefix, &settings);
            let roles: Vec<StructureRole> = grown(&record, &lots, &prefix)
                .into_iter()
                .map(|(key, ..)| entry_of(key, &prefix).role())
                .collect();
            let count = |role| roles.iter().filter(|r| **r == role).count();
            if [
                StructureRole::Landmark,
                StructureRole::Secondary,
                StructureRole::Prop,
            ]
            .into_iter()
            .any(offers)
            {
                assert_eq!(roles.len(), lots.len(), "{theme:?}: every lot builds");
            }
            if offers(StructureRole::Secondary) {
                assert_eq!(
                    count(StructureRole::Prop),
                    0,
                    "{theme:?}: a prop grew while the theme has secondaries"
                );
            }
            if offers(StructureRole::Landmark) {
                assert!(count(StructureRole::Landmark) >= 1, "{theme:?}");
            }
            if offers(StructureRole::Landmark) && offers(StructureRole::Secondary) {
                assert_eq!(count(StructureRole::Landmark), 6, "{theme:?}: 15% of 40");
                assert_eq!(count(StructureRole::Secondary), 34, "{theme:?}");
                both_roles += 1;
            }
        }
        assert!(
            both_roles >= 10,
            "too few themes offer both roles to say anything: {both_roles}"
        );
    }

    /// #1555: a mix this build does not know grows as Balanced, so an older
    /// client handed a Downtown network still builds a town rather than
    /// nothing.
    #[test]
    fn an_unknown_mix_grows_as_balanced() {
        use crate::pds::generator::LotTierBias;
        let did = urban_did();
        let lots: Vec<BuildingLot> = (0..40)
            .map(|i| lot(i as f32 * 60.0, 0.0, 12.0 + i as f32, 30.0))
            .collect();
        let grow = |tier_bias| {
            let mut record = RoomRecord::default_for_did(&did);
            let settings = LotSettings {
                tier_bias,
                ..LotSettings::default()
            };
            inject_lot_buildings(&mut record, &lots, &did, 5, &seed_prefix(5), &settings);
            record
        };
        let balanced = grow(LotTierBias::Balanced);
        assert!(
            !crate::state::records_differ(&balanced, &grow(LotTierBias::Unknown)),
            "an unknown mix must grow exactly the Balanced district"
        );
        assert!(
            crate::state::records_differ(&balanced, &grow(LotTierBias::Downtown)),
            "control: Downtown is not Balanced"
        );
        let newer: LotSettings = serde_json::from_value(serde_json::json!({
            "tier_bias": { "$type": "network.symbios.lot_bias.arcology" }
        }))
        .expect("a newer mix reads");
        assert_eq!(newer.tier_bias, LotTierBias::Unknown);
    }

    /// #1555: the network's prosperity and escalation replace the room's
    /// seeded scene everywhere the lot layer reads it - the pools, the
    /// finish, the ruin. Sequence from the report: a futuristic city on an
    /// account that rolled "open conflict" grew barricades and wreckage, and
    /// the ruin leaned and collapsed its buildings, with no field to stop it.
    ///
    /// Every comparison here is against an entry's identity or against a
    /// derivation with no ruin in it; the ruin's own determinism is
    /// `pds::ruin`'s test (it walks a node's materials in key order since
    /// the critic of #1553).
    #[test]
    fn the_lot_overrides_replace_the_scene_in_the_pools_the_finish_and_the_ruin() {
        use crate::seeded_defaults::{EscalationBand, ProsperityBand};
        let did = did_where(|s| {
            s.escalation_tier() == EscalationTier::Conflict
                && s.prosperity_tier() == ProsperityTier::Rich
        });
        let scene = SceneCharacter::for_did(&did);
        let lots: Vec<BuildingLot> = (0..80)
            .map(|i| lot(i as f32 * 60.0, 0.0, 10.0 + i as f32 * 0.5, 30.0))
            .collect();
        let prefix = seed_prefix(11);
        let room = RoomRecord::default_for_did(&did);
        let grow = |settings: &LotSettings| {
            let mut record = room.clone();
            inject_lot_buildings(&mut record, &lots, &did, 11, &prefix, settings);
            record
        };
        let has_band = |record: &RoomRecord, wanted: &dyn Fn(&dyn CatalogueEntry) -> bool| {
            grown(record, &lots, &prefix)
                .into_iter()
                .any(|(key, ..)| wanted(entry_of(key, &prefix)))
        };
        let conflict = |e: &dyn CatalogueEntry| {
            e.escalation_band() == EscalationBand::only(EscalationTier::Conflict)
        };
        let rich = |e: &dyn CatalogueEntry| {
            e.prosperity_band() == ProsperityBand::only(ProsperityTier::Rich)
        };

        // Control: the scene's own open conflict and wealth.
        let own = grow(&LotSettings::default());
        assert!(has_band(&own, &conflict), "the scene grows conflict props");
        assert!(has_band(&own, &rich), "the scene grows rich props");

        // Peace: no conflict prop, and no ruin - every generator is its
        // entry's un-ruined derivation, finished at the scene's prosperity
        // and no scorch, drawn at its lot's fit.
        let peaceful = LotSettings {
            escalation: Some(Fp(0.0)),
            ..LotSettings::default()
        };
        let calm = grow(&peaceful);
        assert!(
            !has_band(&calm, &conflict),
            "escalation 0 grew a conflict prop"
        );
        for (key, lot, ..) in grown(&calm, &lots, &prefix) {
            let entry = entry_of(key, &prefix);
            let mut want = entry
                .build(&did)
                .with_shape_seed(11 ^ fnv1a_64(entry.slug()));
            material_finish::apply_socio_finish(&mut want, scene.prosperity, 0.0);
            crate::seeded_defaults::room::build::scale_about_ground(
                &mut want,
                drawn_scale(entry, lot, &peaceful),
            );
            assert!(
                calm.generators[key] == want,
                "{key} was ruined or scorched by the scene's escalation"
            );
        }

        // Poverty: no rich-only entry.
        let poor = grow(&LotSettings {
            prosperity: Some(Fp(0.0)),
            ..LotSettings::default()
        });
        assert!(!has_band(&poor, &rich), "prosperity 0 grew a rich prop");
    }

    /// #1555: an override equal to the room's own scene changes nothing -
    /// it replaces exactly the values the scene would have supplied - while
    /// a different one does change the district. On a Tense room, whose
    /// ruin (a lean and a settle) is deterministic, so whole derivations
    /// can be compared.
    #[test]
    fn an_override_at_the_scenes_own_values_grows_the_scenes_district() {
        let did = did_where(|s| s.escalation_tier() == EscalationTier::Tense);
        let scene = SceneCharacter::for_did(&did);
        let lots: Vec<BuildingLot> = (0..40)
            .map(|i| lot(i as f32 * 60.0, 0.0, 10.0 + i as f32, 30.0))
            .collect();
        let room = RoomRecord::default_for_did(&did);
        let grow = |settings: &LotSettings| {
            let mut record = room.clone();
            inject_lot_buildings(&mut record, &lots, &did, 13, &seed_prefix(13), settings);
            record
        };
        let own = grow(&LotSettings::default());
        let same = grow(&LotSettings {
            prosperity: Some(Fp(scene.prosperity)),
            escalation: Some(Fp(scene.escalation)),
            ..LotSettings::default()
        });
        assert!(!crate::state::records_differ(&own, &same));
        let peaceful = grow(&LotSettings {
            escalation: Some(Fp(0.0)),
            ..LotSettings::default()
        });
        assert!(
            crate::state::records_differ(&own, &peaceful),
            "control: peace on a Tense room lifts its wear"
        );
    }

    /// #1555: the street props take the same overrides as the buildings - a
    /// network told to be peaceful does not line its streets with
    /// barricades, nor wear them down.
    #[test]
    fn the_street_props_take_the_lot_overrides() {
        use crate::seeded_defaults::EscalationBand;
        let did = did_where(|s| s.escalation_tier() == EscalationTier::Conflict);
        let scene = SceneCharacter::for_did(&did);
        let spots: Vec<crate::urban::FurnitureSpot> = (0..120)
            .map(|i| crate::urban::FurnitureSpot {
                position: [i as f32 * 7.0, 3.0],
                yaw: 0.2,
            })
            .collect();
        let prefix = net_prefix(FURNITURE_PREFIX, 0, 3);
        let room = RoomRecord::default_for_did(&did);
        let grow = |settings: &LotSettings| {
            let mut record = room.clone();
            inject_street_furniture(&mut record, &spots, &did, 3, &prefix, settings);
            record
        };
        let conflict_props = |record: &RoomRecord| {
            record
                .generators
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .filter(|k| {
                    entry_of(k, &prefix).escalation_band()
                        == EscalationBand::only(EscalationTier::Conflict)
                })
                .count()
        };
        assert!(
            conflict_props(&grow(&LotSettings::default())) > 0,
            "control: the scene's conflict lines the streets"
        );
        let peaceful = LotSettings {
            escalation: Some(Fp(0.0)),
            ..LotSettings::default()
        };
        let calm = grow(&peaceful);
        assert_eq!(conflict_props(&calm), 0);
        for (key, generator) in calm
            .generators
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
        {
            let entry = entry_of(key, &prefix);
            let mut want = entry
                .build(&did)
                .with_shape_seed(3 ^ fnv1a_64(entry.slug()) ^ FURNITURE_STREAM_SALT);
            material_finish::apply_socio_finish(&mut want, scene.prosperity, 0.0);
            assert!(
                *generator == want,
                "{key} was worn by the scene's escalation"
            );
        }
    }

    /// #1555: the lot fingerprint carries `{:?}` of the lot settings, so an
    /// override or the mix moving re-derives the district.
    #[test]
    fn the_lot_overrides_and_the_mix_re_derive_the_lots() {
        let base = RoadConfig::default();
        let edits: [fn(&mut RoadConfig); 4] = [
            |c| c.lots.prosperity = Some(Fp(0.9)),
            |c| c.lots.escalation = Some(Fp(0.0)),
            |c| c.lots.escalation = Some(Fp(0.5)),
            |c| c.lots.tier_bias = crate::pds::generator::LotTierBias::Downtown,
        ];
        let fp = |c: &RoadConfig| layout_fingerprint("did:test:0", c, None);
        let mut seen = vec![fp(&base)];
        for edit in edits {
            let mut edited = base.clone();
            edit(&mut edited);
            let key = fp(&edited);
            assert!(!seen.contains(&key), "{key} did not move");
            seen.push(key);
        }
    }

    /// The `Adopt` arm, end to end through the system (#882, #1553): a
    /// record loaded with its network's buildings already in it - here as
    /// a record saved before the fit was baked, one generator per entry
    /// and the fit on the placement's scale - is trusted as saved. No
    /// strip, no re-injection, not one byte changed, however long the
    /// session runs.
    #[test]
    fn a_loaded_record_carrying_its_buildings_is_adopted_untouched() {
        let did = urban_did();
        let mut record = RoomRecord::default_for_did(&did);
        let config = RoadConfig {
            seed: 77,
            ..RoadConfig::default()
        };
        let terrain = record
            .generators
            .values_mut()
            .find(|g| matches!(g.kind, GeneratorKind::Terrain(_)))
            .expect("a seeded room has terrain");
        terrain.children.push(Generator {
            kind: GeneratorKind::RoadNetwork(config.clone()),
            ..Generator::default()
        });
        let saved = format!("{}neon_megatower", net_prefix(LOT_PREFIX, 0, config.seed));
        record
            .generators
            .insert(saved.clone(), Generator::default());
        record.placements.push(Placement::Absolute {
            generator_ref: saved,
            transform: TransformData {
                scale: Fp3([2.0, 2.0, 2.0]),
                ..TransformData::default()
            },
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: Fp(16.0),
            seed: None,
        });
        let before = record.clone();

        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<crate::state::RoomWriteSignals>()
            .init_resource::<super::super::RoadPanelStats>()
            .init_resource::<crate::notify::Toasts>()
            .insert_resource(LiveRoomRecord(record))
            .insert_resource(CurrentRoomDid(did))
            .insert_resource(FinishedHeightMap(
                crate::urban::test_support::sloped_heightmap(),
                None,
            ))
            .add_systems(Update, maybe_populate_lots);
        for _ in 0..6 {
            app.update();
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs(1));
        }
        assert!(
            !crate::state::records_differ(&before, &app.world().resource::<LiveRoomRecord>().0),
            "a loaded record's saved buildings were replaced"
        );
        assert_eq!(
            app.world()
                .resource::<super::super::RoadPanelStats>()
                .buildings,
            1,
            "the adopted building is what the readout counts"
        );
    }

    /// The lot injector as it stood before the fit (#1553), frozen: the
    /// oracle the default injection is held to. Copied from HEAD 877b100 by
    /// the critic of #1553, only renamed - Downtown, which HEAD lacked,
    /// grows as Balanced here and is never asked of it.
    fn pre_fit_inject_lot_buildings(
        record: &mut RoomRecord,
        lots: &[crate::urban::BuildingLot],
        did: &str,
        seed: u64,
        prefix: &str,
        settings: &LotSettings,
    ) -> InjectReport {
        let mut report = InjectReport {
            found: lots.len(),
            ..InjectReport::default()
        };
        let scene = SceneCharacter::for_seed(fnv1a_64(did));
        let base_theme = resolve_lot_theme(&settings.theme_override).unwrap_or(scene.theme);
        let theme = if entries_for(base_theme, StructureRole::Landmark)
            .next()
            .is_some()
        {
            base_theme
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
            return report;
        }
        let mut ranked: Vec<&crate::urban::BuildingLot> = lots.iter().collect();
        ranked.sort_by(|a, b| (b.width * b.depth).total_cmp(&(a.width * a.depth)));
        let keep = ((ranked.len() as f32 * settings.density.0.clamp(0.0, 1.0)).ceil() as usize)
            .min(ranked.len());
        ranked.truncate(keep);
        report.kept = keep;
        let (cap, capped_by_budget) = placement_cap(MAX_LOT_BUILDINGS, record);
        report.dropped_to_cap = ranked.len().saturating_sub(cap);
        report.capped_by_budget = capped_by_budget && report.dropped_to_cap > 0;
        ranked.truncate(cap);
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ LOT_STREAM_SALT);
        let mut by_slug: HashMap<&'static str, String> = HashMap::new();
        let mut placed = 0usize;
        for (i, lot) in ranked.iter().enumerate() {
            use crate::pds::generator::LotTierBias;
            let n = ranked.len();
            let order: [&[&'static dyn CatalogueEntry]; 3] = match settings.tier_bias {
                LotTierBias::Balanced | LotTierBias::Unknown | LotTierBias::Downtown => {
                    if i == 0 {
                        [&landmark, &secondary, &prop]
                    } else if i * 5 < n {
                        [&secondary, &prop, &landmark]
                    } else {
                        [&prop, &secondary, &landmark]
                    }
                }
                LotTierBias::Monumental => {
                    if i * 10 < n.max(1) || i == 0 {
                        [&landmark, &secondary, &prop]
                    } else if i * 10 < n * 4 {
                        [&secondary, &prop, &landmark]
                    } else {
                        [&prop, &secondary, &landmark]
                    }
                }
                LotTierBias::Residential => {
                    if i * 3 < n {
                        [&secondary, &prop, &prop]
                    } else {
                        [&prop, &secondary, &secondary]
                    }
                }
                LotTierBias::PropsOnly => [&prop, &prop, &prop],
            };
            let Some(chosen) = order.into_iter().find(|p| !p.is_empty()) else {
                continue;
            };
            let entry = chosen[(rng.next_u32() as usize) % chosen.len()];
            let slug = entry.slug();
            let name = if let Some(existing) = by_slug.get(slug) {
                existing.clone()
            } else if record.generators.len() >= limits::MAX_GENERATORS {
                report.generator_cap_skips += 1;
                continue;
            } else {
                let entry_seed = seed ^ fnv1a_64(slug);
                let mut tree = entry.build(did).with_shape_seed(entry_seed);
                material_finish::apply_socio_finish(&mut tree, scene.prosperity, scene.escalation);
                // The entry's own ruin bound (#1559) is item data, like its
                // geometry, which this oracle also reads from the catalogue.
                ruin::apply_ruin_bounded(
                    &mut tree,
                    scene.escalation,
                    entry_seed,
                    entry.ruin_max_lean(),
                );
                let name = format!("{prefix}{slug}");
                record.generators.insert(name.clone(), tree);
                by_slug.insert(slug, name.clone());
                name
            };
            let fp = entry.footprint();
            let (smin, smax) = (
                settings.scale_min.0.min(settings.scale_max.0),
                settings.scale_max.0.max(settings.scale_min.0),
            );
            let fit = (lot.width.min(lot.depth) / (2.0 * fp.clearance.max(0.5))).clamp(smin, smax);
            let half_yaw = lot.yaw * 0.5;
            record.placements.push(Placement::Absolute {
                generator_ref: name,
                transform: TransformData {
                    translation: Fp3([lot.position[0], -FOUNDATION_SINK_M, lot.position[1]]),
                    rotation: Fp4([0.0, libm::sinf(half_yaw), 0.0, libm::cosf(half_yaw)]),
                    scale: Fp3([fit, fit, fit]),
                },
                snap_to_terrain: true,
                avoid_water: true,
                // The ground radius an entry declares (#1559), as the live
                // path writes it; every other entry's clearance, as before.
                avoid_water_clearance: Fp(entry.ground_radius().unwrap_or(fp.clearance)),
                seed: None,
            });
            placed += 1;
        }
        report.placed = placed;
        report
    }

    /// #1553, the critic's first two findings: with the fit off - the
    /// default, and every network saved before it - the injection writes
    /// exactly what it wrote before, byte for byte, in every theme, every
    /// mix HEAD knew, the default and a narrower clamp, a calm room and one
    /// at open conflict. So a saved district grown again (a portal into the
    /// room, a sibling network that grows nothing) comes back as it was
    /// saved rather than in a new form. The copy it is held to carries the
    /// one change made since, #1514's - every Shape node of an entry's
    /// generator draws with the entry's seed - so this does not pin the
    /// bytes a district grown before #1514 had.
    #[test]
    fn the_default_injection_is_byte_identical_to_the_one_before_the_fit() {
        use crate::pds::generator::LotTierBias;
        let lots: Vec<BuildingLot> = (0..40)
            .map(|i| {
                let i = i as f32;
                lot(
                    i * 60.0,
                    (i * 7.0) % 50.0,
                    4.0 + i * 1.3,
                    9.0 + (i * 3.1) % 30.0,
                )
            })
            .collect();
        let calm = did_where(|s| s.escalation < 0.3);
        let fought = did_where(|s| s.escalation > 0.7);
        // Every theme under the default mix and clamp; every other mix HEAD
        // knew and a narrower clamp over a spread of themes - the same code
        // paths, at a cost CI's unoptimised test build can carry.
        let mut cases = Vec::new();
        for (t, theme) in ThemeArchetype::ALL.into_iter().enumerate() {
            cases.push((theme, LotTierBias::Balanced, (0.5, 2.0)));
            if t % 6 == 0 {
                for bias in [
                    LotTierBias::Monumental,
                    LotTierBias::Residential,
                    LotTierBias::PropsOnly,
                    LotTierBias::Unknown,
                ] {
                    cases.push((theme, bias, (0.5, 2.0)));
                }
                cases.push((theme, LotTierBias::Balanced, (0.3, 1.2)));
            }
        }
        let mut compared = 0;
        for did in [calm, fought] {
            for &(theme, bias, (lo, hi)) in &cases {
                let settings = LotSettings {
                    theme_override: theme.label().to_string(),
                    tier_bias: bias,
                    scale_min: Fp(lo),
                    scale_max: Fp(hi),
                    ..LotSettings::default()
                };
                let (mut new, mut old) = (
                    RoomRecord::default_for_did(&did),
                    RoomRecord::default_for_did(&did),
                );
                let prefix = seed_prefix(31);
                let a = inject_lot_buildings(&mut new, &lots, &did, 31, &prefix, &settings);
                let b = pre_fit_inject_lot_buildings(&mut old, &lots, &did, 31, &prefix, &settings);
                assert_eq!(
                    (
                        a.found,
                        a.kept,
                        a.placed,
                        a.dropped_to_cap,
                        a.generator_cap_skips
                    ),
                    (
                        b.found,
                        b.kept,
                        b.placed,
                        b.dropped_to_cap,
                        b.generator_cap_skips
                    ),
                    "{theme:?} {bias:?} {lo}-{hi}: the report moved"
                );
                assert!(
                    !crate::state::records_differ(&new, &old),
                    "{theme:?} {bias:?} {lo}-{hi} in {did}: the default injection \
                     is not the one before the fit"
                );
                compared += a.placed;
            }
        }
        assert!(
            compared > 500,
            "the oracle compared a district's worth: {compared}"
        );
    }

    /// A seeded room with one road network under its terrain per `configs`,
    /// each carrying one saved building of its own, its heightmap and that
    /// heightmap's source.
    fn saved_road_room(did: &str, terrain_seed: Option<u64>, configs: &[RoadConfig]) -> RoomRecord {
        let mut record = RoomRecord::default_for_did(did);
        let terrain = record
            .generators
            .values_mut()
            .find(|g| matches!(g.kind, GeneratorKind::Terrain(_)))
            .expect("a seeded room has terrain");
        if let (Some(seed), GeneratorKind::Terrain(cfg)) = (terrain_seed, &mut terrain.kind) {
            cfg.seed = seed;
        }
        for config in configs {
            terrain
                .children
                .push(Generator::from_kind(GeneratorKind::RoadNetwork(
                    config.clone(),
                )));
        }
        for (i, config) in configs.iter().enumerate() {
            if !config.populate_lots || config.lots.density.0 == 0.0 {
                continue;
            }
            let saved = format!("{}neon_megatower", net_prefix(LOT_PREFIX, i, config.seed));
            record
                .generators
                .insert(saved.clone(), Generator::default());
            record.placements.push(Placement::Absolute {
                generator_ref: saved,
                transform: TransformData::default(),
                snap_to_terrain: true,
                avoid_water: true,
                avoid_water_clearance: Fp(16.0),
                seed: None,
            });
        }
        record
    }

    /// An app running the lot layer alone over `record` in `did`'s room,
    /// with its heightmap and that heightmap's source.
    fn lot_app(record: RoomRecord, did: &str) -> App {
        let source = super::super::terrain_source_key(&record);
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<crate::state::RoomWriteSignals>()
            .init_resource::<super::super::RoadPanelStats>()
            .init_resource::<crate::notify::Toasts>()
            .insert_resource(LiveRoomRecord(record))
            .insert_resource(CurrentRoomDid(did.to_string()))
            .insert_resource(FinishedHeightMap(
                crate::urban::test_support::sloped_heightmap(),
                None,
            ))
            .insert_resource(HeightMapSource(source))
            .add_systems(Update, maybe_populate_lots);
        app
    }

    fn run_frames(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs(1));
        }
    }

    /// #1553, the critic's first finding: a portal swaps the record in place
    /// and leaves the last room's heightmap up until the new one lands. The
    /// lot layer used to meet the new room with the old room's fingerprint,
    /// call it a layout edit and regrow its saved district; and when the new
    /// heightmap arrived, call that a terrain edit and regrow it again. The
    /// new room's buildings are adopted as on a login, untouched, before and
    /// after its own ground arrives.
    #[test]
    fn a_portal_into_a_saved_road_room_adopts_its_district() {
        let (a, b) = ("did:test:portal:a", "did:test:portal:b");
        let room_a = saved_road_room(
            a,
            None,
            &[RoadConfig {
                seed: 5,
                ..RoadConfig::default()
            }],
        );
        let room_b = saved_road_room(
            b,
            Some(9_001),
            &[RoadConfig {
                seed: 6,
                major_spacing: Fp(80.0),
                ..RoadConfig::default()
            }],
        );
        let mut app = lot_app(room_a, a);
        run_frames(&mut app, 4);

        // The portal: the record and the room's DID swap; the old ground
        // (and its source) stays up for now.
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = room_b.clone();
        app.world_mut().resource_mut::<CurrentRoomDid>().0 = b.to_string();
        run_frames(&mut app, 4);
        assert!(
            !crate::state::records_differ(&room_b, &app.world().resource::<LiveRoomRecord>().0),
            "the new room's saved district was regrown on the old room's ground"
        );

        // Its own ground lands.
        let source = super::super::terrain_source_key(&room_b);
        app.world_mut().insert_resource(FinishedHeightMap(
            crate::urban::test_support::pilot_heightmap(),
            None,
        ));
        app.world_mut().insert_resource(HeightMapSource(source));
        run_frames(&mut app, 4);
        assert!(
            !crate::state::records_differ(&room_b, &app.world().resource::<LiveRoomRecord>().0),
            "the new room's saved district was regrown when its own ground arrived"
        );
        assert_eq!(
            app.world()
                .resource::<super::super::RoadPanelStats>()
                .last_replaced,
            0
        );
    }

    /// #1553, the critic's second finding: a network that grows nothing (a
    /// sibling at density 0) is never populated, and the load used to count
    /// it missing and strip and regrow EVERY network over the record just
    /// saved, on every login. It is as complete as a populated one.
    #[test]
    fn a_sibling_network_that_grows_nothing_leaves_a_saved_district_alone() {
        let did = "did:test:sibling";
        let room = saved_road_room(
            did,
            None,
            &[
                RoadConfig {
                    seed: 5,
                    ..RoadConfig::default()
                },
                RoadConfig {
                    seed: 8,
                    center: crate::pds::types::Fp2([60.0, 60.0]),
                    lots: LotSettings {
                        density: Fp(0.0),
                        ..LotSettings::default()
                    },
                    ..RoadConfig::default()
                },
            ],
        );
        let mut app = lot_app(room.clone(), did);
        run_frames(&mut app, 6);
        assert!(
            !crate::state::records_differ(&room, &app.world().resource::<LiveRoomRecord>().0),
            "a sibling that grows nothing made the load regrow the saved district"
        );
        assert_eq!(
            app.world()
                .resource::<super::super::RoadPanelStats>()
                .last_replaced,
            0
        );
    }

    /// #1514: a lot building's grammars draw with its entry's seed. Every
    /// grammar entry of the catalogue roots on a footing box with its Shape
    /// node beneath it, and the seed was stamped on the root alone, so a
    /// grown building drew its entry's catalogue seed in every room. Over
    /// every theme, every Shape node of every grown generator carries
    /// `seed ^ fnv1a_64(slug)` - and some sit below their roots, where the
    /// old stamp never reached.
    #[test]
    fn a_lot_buildings_grammars_draw_with_its_entrys_seed() {
        fn shape_seeds(node: &Generator, depth: usize, out: &mut Vec<(usize, u64)>) {
            if let GeneratorKind::Shape { seed, .. } = node.kind {
                out.push((depth, seed));
            }
            for child in &node.children {
                shape_seeds(child, depth + 1, out);
            }
        }
        let did = urban_did();
        let lots: Vec<BuildingLot> = (0..40)
            .map(|i| lot(i as f32 * 60.0, 0.0, 20.0 + i as f32, 30.0))
            .collect();
        let mut below_the_root = 0;
        for theme in ThemeArchetype::ALL {
            let settings = LotSettings {
                theme_override: theme.label().to_string(),
                ..LotSettings::default()
            };
            let mut record = RoomRecord::default_for_did(&did);
            let prefix = seed_prefix(77);
            inject_lot_buildings(&mut record, &lots, &did, 77, &prefix, &settings);
            for (key, generator) in &record.generators {
                if !key.starts_with(&prefix) {
                    continue;
                }
                let entry_seed = 77 ^ fnv1a_64(entry_of(key, &prefix).slug());
                let mut seeds = Vec::new();
                shape_seeds(generator, 0, &mut seeds);
                for (depth, seed) in seeds {
                    assert_eq!(
                        seed, entry_seed,
                        "{theme:?} {key}: a grammar {depth} deep draws another seed"
                    );
                    below_the_root += usize::from(depth > 0);
                }
            }
        }
        assert!(
            below_the_root >= 3,
            "the themes grow grammar buildings: {below_the_root} grammars below a root"
        );
    }

    /// #1555, the critic's fifth finding: the prosperity override reaches
    /// the material finish, not only the pools - every generator is its
    /// entry finished at the OVERRIDE's prosperity, in a calm room whose own
    /// prosperity is far from it.
    #[test]
    fn the_prosperity_override_reaches_the_finish() {
        let did = did_where(|s| s.escalation < 0.3 && s.prosperity > 0.7);
        let scene = SceneCharacter::for_did(&did);
        let lots: Vec<BuildingLot> = (0..24)
            .map(|i| lot(i as f32 * 80.0, 0.0, 12.0 + i as f32, 40.0))
            .collect();
        let poor = LotSettings {
            prosperity: Some(Fp(0.05)),
            ..LotSettings::default()
        };
        let mut record = RoomRecord::default_for_did(&did);
        let prefix = seed_prefix(19);
        let report = inject_lot_buildings(&mut record, &lots, &did, 19, &prefix, &poor);
        assert!(report.placed > 0);
        for (key, ..) in grown(&record, &lots, &prefix) {
            let entry = entry_of(key, &prefix);
            let mut want = entry
                .build(&did)
                .with_shape_seed(19 ^ fnv1a_64(entry.slug()));
            material_finish::apply_socio_finish(&mut want, 0.05, scene.escalation);
            ruin::apply_ruin_bounded(
                &mut want,
                scene.escalation,
                19 ^ fnv1a_64(entry.slug()),
                entry.ruin_max_lean(),
            );
            assert!(
                record.generators[key] == want,
                "{key} was not finished at the override's prosperity"
            );
        }
    }

    /// #1555, the critic's fourth finding: Downtown in a theme with no
    /// secondary building at its tier fills the tail with props, never the
    /// same landmark on every lot (a Pirate district at a low prosperity
    /// stood one on all 68).
    #[test]
    fn downtown_without_a_secondary_fills_its_tail_with_props() {
        let settings = LotSettings {
            theme_override: String::from("Pirate"),
            tier_bias: crate::pds::generator::LotTierBias::Downtown,
            prosperity: Some(Fp(0.1)),
            escalation: Some(Fp(0.0)),
            ..LotSettings::default()
        };
        let theme = resolve_lot_theme("Pirate").expect("a Pirate theme");
        assert!(
            pool_for(theme, StructureRole::Secondary, 0.1, 0.0).is_empty(),
            "the premise: Pirate has no secondary at a low prosperity"
        );
        assert!(!pool_for(theme, StructureRole::Prop, 0.1, 0.0).is_empty());
        let did = urban_did();
        let lots: Vec<BuildingLot> = (0..68)
            .map(|i| lot(i as f32 * 50.0, 0.0, 10.0 + i as f32 * 0.5, 30.0))
            .collect();
        let mut record = RoomRecord::default_for_did(&did);
        let prefix = seed_prefix(3);
        inject_lot_buildings(&mut record, &lots, &did, 3, &prefix, &settings);
        let landmarks = grown(&record, &lots, &prefix)
            .into_iter()
            .filter(|(key, ..)| entry_of(key, &prefix).role() == StructureRole::Landmark)
            .count();
        assert_eq!(
            landmarks,
            (68usize * 15).div_ceil(100),
            "only the top ~15% are landmarks; the tail grew props"
        );
    }

    /// #1554: the triangle report grows the districts a record carries no
    /// content for, as a client grows them on load, and leaves a saved
    /// district alone - so it counts what a visitor draws, once.
    #[test]
    fn the_report_grows_only_the_districts_a_record_does_not_carry() {
        let did = "did:test:report";
        let config = RoadConfig {
            seed: crate::urban::test_support::PILOT_ROAD_SEED,
            ..RoadConfig::default()
        };
        let mut record = RoomRecord::default_for_did(did);
        let terrain = record
            .generators
            .values_mut()
            .find(|g| matches!(g.kind, GeneratorKind::Terrain(_)))
            .expect("a seeded room has terrain");
        terrain
            .children
            .push(Generator::from_kind(GeneratorKind::RoadNetwork(config)));
        let hm = FinishedHeightMap(crate::urban::test_support::pilot_heightmap(), None);
        let planted = grow_missing_districts(&mut record, &hm, did);
        assert!(planted > 0, "an unsaved district is grown for the count");
        let grown = record.clone();
        assert_eq!(
            grow_missing_districts(&mut record, &hm, did),
            0,
            "a carried one is not"
        );
        assert!(!crate::state::records_differ(&grown, &record));
    }

    /// #1555: a district with a core ranks its lots nearest-first, so the
    /// landmarks stand round the core and density keeps the lots nearest
    /// it; without one the biggest lots lead, as before.
    #[test]
    fn a_core_ranks_the_lots_nearest_first() {
        use crate::pds::generator::LotTierBias;
        // Small lots near the origin, big ones far out: by size the far ones
        // lead, by distance to a core at the origin the near ones do.
        let lots: Vec<BuildingLot> = (0..20)
            .map(|i| {
                let i = i as f32;
                lot(30.0 + i * 25.0, 0.0, 10.0 + i * 2.0, 30.0)
            })
            .collect();
        let did = urban_did();
        let grow = |focus: Option<crate::pds::types::Fp2>, density: f32| {
            let settings = LotSettings {
                theme_override: String::from("Cyberpunk"),
                tier_bias: LotTierBias::Downtown,
                escalation: Some(Fp(0.0)),
                prosperity: Some(Fp(0.9)),
                density: Fp(density),
                focus,
                ..LotSettings::default()
            };
            let mut record = RoomRecord::default_for_did(&did);
            let prefix = seed_prefix(5);
            inject_lot_buildings(&mut record, &lots, &did, 5, &prefix, &settings);
            let mut by_x: Vec<(f32, StructureRole)> = grown(&record, &lots, &prefix)
                .into_iter()
                .map(|(key, lot, ..)| (lot.position[0], entry_of(key, &prefix).role()))
                .collect();
            by_x.sort_by(|a, b| a.0.total_cmp(&b.0));
            by_x
        };
        let core = Some(crate::pds::types::Fp2([0.0, 0.0]));
        let cored = grow(core, 1.0);
        assert_eq!(
            cored[0].1,
            StructureRole::Landmark,
            "the lot nearest the core: {cored:?}"
        );
        assert_ne!(
            cored[19].1,
            StructureRole::Landmark,
            "the farthest: {cored:?}"
        );
        let sized = grow(None, 1.0);
        assert_eq!(
            sized[19].1,
            StructureRole::Landmark,
            "by size the biggest leads: {sized:?}"
        );
        // Thinned to half, a core keeps the near half.
        let thinned = grow(core, 0.5);
        assert_eq!(thinned.len(), 10);
        assert!(
            thinned.iter().all(|(x, _)| *x < 30.0 + 10.0 * 25.0),
            "{thinned:?}"
        );
    }

    /// The lot layer alone over `record` in `did`'s room, on `hm`, with the
    /// record's own terrain named as the heightmap's source.
    fn lot_app_on(record: RoomRecord, did: &str, hm: bevy_symbios_ground::HeightMap) -> App {
        let mut app = lot_app(record, did);
        app.world_mut().insert_resource(FinishedHeightMap(hm, None));
        app
    }

    /// A seeded room with the given networks under its terrain and nothing
    /// grown yet.
    fn road_room(did: &str, configs: &[RoadConfig]) -> RoomRecord {
        let mut record = RoomRecord::default_for_did(did);
        let terrain = record
            .generators
            .values_mut()
            .find(|g| matches!(g.kind, GeneratorKind::Terrain(_)))
            .expect("a seeded room has terrain");
        for config in configs {
            terrain
                .children
                .push(Generator::from_kind(GeneratorKind::RoadNetwork(
                    config.clone(),
                )));
        }
        record
    }

    /// Grow `record`'s districts as a first session does, and hand back the
    /// record a save would keep.
    fn grown_and_saved(record: RoomRecord, did: &str) -> RoomRecord {
        let mut app = lot_app_on(record, did, crate::urban::test_support::pilot_heightmap());
        run_frames(&mut app, 6);
        app.world().resource::<LiveRoomRecord>().0.clone()
    }

    /// Load `saved` in a fresh session and say whether it came back as it
    /// was saved, with nothing replaced.
    fn adopted_untouched(saved: &RoomRecord, did: &str) -> bool {
        let mut app = lot_app_on(
            saved.clone(),
            did,
            crate::urban::test_support::pilot_heightmap(),
        );
        run_frames(&mut app, 6);
        let after = &app.world().resource::<LiveRoomRecord>().0;
        !crate::state::records_differ(saved, after)
            && app
                .world()
                .resource::<super::super::RoadPanelStats>()
                .last_replaced
                == 0
    }

    fn pilot_network() -> RoadConfig {
        RoadConfig {
            seed: crate::urban::test_support::PILOT_ROAD_SEED,
            ..RoadConfig::default()
        }
    }

    /// The end review of #1553: no test drove an unsaved network to its
    /// buildings through the system, so a guard that never let a decision
    /// through (lots never grow) passed every test. A network with nothing
    /// saved grows its district on its own ground.
    #[test]
    fn an_unsaved_network_grows_its_district_through_the_system() {
        let did = "did:test:grow";
        let grown = grown_and_saved(road_room(did, &[pilot_network()]), did);
        let (lots, _) = layer_content(&grown, 0, &pilot_network());
        assert!(lots, "the network grew no buildings");
        assert!(grown.placements.iter().any(is_road_grown));
    }

    /// The end review's first finding: a network whose lots grow nothing
    /// (density 0) but whose street props are on was counted missing, its
    /// LOT prefix absent, and stripped and regrown on every login. Each
    /// layer is judged on its own: the saved props are adopted.
    #[test]
    fn a_network_whose_lots_grow_nothing_keeps_its_saved_street_props() {
        let did = "did:test:props";
        let props_only = RoadConfig {
            lots: LotSettings {
                density: Fp(0.0),
                ..LotSettings::default()
            },
            furniture: crate::pds::generator::FurnitureSettings {
                enabled: true,
                ..Default::default()
            },
            ..pilot_network()
        };
        let saved = grown_and_saved(road_room(did, std::slice::from_ref(&props_only)), did);
        assert_eq!(
            layer_content(&saved, 0, &props_only),
            (false, true),
            "the premise: props grown, no buildings"
        );
        assert!(
            adopted_untouched(&saved, did),
            "the saved props were regrown"
        );
    }

    /// The end review's second finding: a sibling network starved by the
    /// shared placement budget grows nothing, but the old check freed the
    /// budget in its scratch copy, found that it would grow, and regrew the
    /// whole district on every login. The growth is simulated in order,
    /// against the one budget.
    #[test]
    fn a_sibling_starved_by_the_shared_budget_keeps_the_saved_district() {
        let did = "did:test:budget";
        let mut room = road_room(
            did,
            &[
                pilot_network(),
                RoadConfig {
                    seed: 31,
                    center: crate::pds::types::Fp2([40.0, 40.0]),
                    ..pilot_network()
                },
            ],
        );
        // Leave room for 20 grown placements in all.
        while room.placements.len() < limits::MAX_PLACEMENTS - 20 {
            room.placements.push(Placement::Absolute {
                generator_ref: String::from("filler"),
                transform: TransformData::default(),
                snap_to_terrain: true,
                avoid_water: false,
                avoid_water_clearance: Fp(0.0),
                seed: None,
            });
        }
        let saved = grown_and_saved(room, did);
        let first = layer_content(&saved, 0, &pilot_network()).0;
        let second = layer_content(
            &saved,
            1,
            &RoadConfig {
                seed: 31,
                ..pilot_network()
            },
        )
        .0;
        assert!(
            first && !second,
            "the premise: the first network took the budget"
        );
        assert!(
            adopted_untouched(&saved, did),
            "the starved sibling made the load regrow"
        );
    }

    /// The end review's third finding: logging out and back into the same
    /// room in one process brings the same ground back, and any heightmap
    /// change once a decision stood regrew the saved district. Only ground
    /// of ANOTHER terrain is a terrain edit.
    #[test]
    fn logging_out_and_back_into_the_same_room_keeps_its_district() {
        let did = "did:test:relog";
        let saved = grown_and_saved(road_room(did, &[pilot_network()]), did);
        let mut app = lot_app_on(
            saved.clone(),
            did,
            crate::urban::test_support::pilot_heightmap(),
        );
        run_frames(&mut app, 4);
        // Logout tears the ground down; the next login builds it again.
        app.world_mut().remove_resource::<FinishedHeightMap>();
        app.world_mut().remove_resource::<HeightMapSource>();
        run_frames(&mut app, 2);
        let source = super::super::terrain_source_key(&saved);
        app.world_mut().insert_resource(FinishedHeightMap(
            crate::urban::test_support::pilot_heightmap(),
            None,
        ));
        app.world_mut().insert_resource(HeightMapSource(source));
        run_frames(&mut app, 4);
        assert!(
            !crate::state::records_differ(&saved, &app.world().resource::<LiveRoomRecord>().0),
            "the same room's own ground, brought back, regrew its district"
        );
        // A regrow of a deterministic district puts back the same bytes, so
        // the record alone cannot tell: nothing may have been replaced.
        assert_eq!(
            app.world()
                .resource::<super::super::RoadPanelStats>()
                .last_replaced,
            0,
            "the district was stripped and grown again"
        );
    }

    /// #1245 f381, pinned at last: a terrain edit moves the ground the lots
    /// were cut from, so when the new terrain's heightmap lands the district
    /// is grown again on it - once, and not on the old ground while the new
    /// one is being built.
    #[test]
    fn a_terrain_edit_regrows_the_district_on_the_new_ground() {
        let did = "did:test:terrain";
        let saved = grown_and_saved(road_room(did, &[pilot_network()]), did);
        let mut app = lot_app_on(
            saved.clone(),
            did,
            crate::urban::test_support::pilot_heightmap(),
        );
        run_frames(&mut app, 4);
        // The owner edits the terrain: the record changes now, the ground a
        // regeneration later.
        let mut edited = saved.clone();
        for g in edited.generators.values_mut() {
            if let GeneratorKind::Terrain(cfg) = &mut g.kind {
                cfg.seed ^= 0xBEEF;
            }
        }
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = edited.clone();
        run_frames(&mut app, 3);
        assert_eq!(
            app.world()
                .resource::<super::super::RoadPanelStats>()
                .last_replaced,
            0,
            "nothing is regrown on the old ground"
        );
        let source = super::super::terrain_source_key(&edited);
        app.world_mut().insert_resource(FinishedHeightMap(
            crate::urban::test_support::sloped_heightmap(),
            None,
        ));
        app.world_mut().insert_resource(HeightMapSource(source));
        run_frames(&mut app, 4);
        assert!(
            app.world()
                .resource::<super::super::RoadPanelStats>()
                .last_replaced
                > 0,
            "the district was not grown again on the new ground"
        );
    }

    /// The end review's fifth finding: the report grew a network's street
    /// props again whenever it held no lot buildings, counting them twice.
    /// A layer the record carries is never grown twice.
    #[test]
    fn the_report_never_grows_a_carried_layer_twice() {
        let did = "did:test:report:props";
        let props_only = RoadConfig {
            lots: LotSettings {
                density: Fp(0.0),
                ..LotSettings::default()
            },
            furniture: crate::pds::generator::FurnitureSettings {
                enabled: true,
                ..Default::default()
            },
            ..pilot_network()
        };
        let mut record = road_room(did, &[props_only]);
        let hm = FinishedHeightMap(crate::urban::test_support::pilot_heightmap(), None);
        assert!(grow_missing_districts(&mut record, &hm, did) > 0);
        assert_eq!(
            grow_missing_districts(&mut record, &hm, did),
            0,
            "the props were grown a second time"
        );
    }

    /// The deadline asks the same question the arming did: a record that
    /// carries a network's buildings but not the street props it now grows
    /// (switched on in a client that never grew them) is incomplete, and the
    /// props grow - the deadline used to ask only whether the buildings were
    /// there, answer "adopt" and leave the props ungrown forever.
    #[test]
    fn a_layer_the_record_lacks_is_grown_on_load() {
        let did = "did:test:layer";
        let mut saved = grown_and_saved(road_room(did, &[pilot_network()]), did);
        for g in saved.generators.values_mut() {
            if let GeneratorKind::Terrain(_) = g.kind {
                for child in &mut g.children {
                    if let GeneratorKind::RoadNetwork(c) = &mut child.kind {
                        c.furniture.enabled = true;
                    }
                }
            }
        }
        let with_props = RoadConfig {
            furniture: crate::pds::generator::FurnitureSettings {
                enabled: true,
                ..Default::default()
            },
            ..pilot_network()
        };
        assert_eq!(layer_content(&saved, 0, &with_props), (true, false));
        let mut app = lot_app_on(saved, did, crate::urban::test_support::pilot_heightmap());
        run_frames(&mut app, 6);
        let after = &app.world().resource::<LiveRoomRecord>().0;
        assert_eq!(
            layer_content(after, 0, &with_props),
            (true, true),
            "the street props the network grows were never grown"
        );
    }
}
