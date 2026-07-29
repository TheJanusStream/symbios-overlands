//! Foundation-depth requirement for settlement structures (#1009).
//!
//! A settlement building is snapped to the highest ground under its
//! footprint (#1008, `world_builder::compile::pad`), so no wall ever
//! starts below grade. The cost of that rule is at the other end: where
//! the ground falls away, the downhill edge now stands clear of it by
//! the full drop across the footprint, and daylight shows underneath.
//!
//! Real construction answers this the same way — a graded pad sets
//! finished floor at the high point, and a foundation wall carries the
//! building down to grade on the low side. That wall is what
//! [`super::util::foundation_block`] already builds: a plinth buried
//! below the entry's own origin, invisible on flat ground and
//! progressively revealed as the ground drops away.
//!
//! What was missing is a *rule* for how deep. Depths across the
//! catalogue were picked by eye, from 1.0 m to 5.0 m, with no relation
//! to how wide the building is — and two thirds of settlement structures
//! carried no plinth at all. Since the drop a footprint spans is
//! `slope × diameter`, the requirement has to scale with the footprint,
//! which is what [`required_depth`] does.

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::Generator;

use super::measure;

/// Drop a plinth is sized to cover, as a fraction of the footprint
/// radius. **Measured, not assumed**: `render --settlement-drop 12`
/// walks every seeded structure in twelve rooms and reports the terrain
/// drop across its footprint. Over 121 footprints the median drop is
/// 0.32 m — half of all buildings are already covered by the placement
/// sink alone — and the 90th percentile for building-sized footprints
/// sits near `0.35 x radius`, which is what this is.
///
/// Sizing to the *worst case* instead (`2 x BUILD_SLOPE_LIMIT`, the
/// steepest ground the siter accepts) demands roughly double this and
/// would put a 4.5 m podium under an ordinary 8 m house to cover ground
/// that 90 % of them never stand on. The long tail is real — the
/// measured maximum is 25 m, on cliff-edge outliers — but no plinth
/// makes those look right; grading the ground does (#1007 stage 3).
pub const FOOTPRINT_DROP_RATIO: f32 = 0.35;

/// The sink every seeded placement already carries (`FOUNDATION_SINK_M`
/// in the room deriver and the lot builder). It bites the building into
/// the high point, so it counts toward the drop the plinth must cover.
const PLACEMENT_SINK: f32 = 0.35;

/// Slack over the computed requirement, absorbing the difference between
/// the ~10 m proxy the siter measured slope on and the ~2 m map the
/// building is finally snapped against.
const MARGIN: f32 = 0.4;

/// Shallowest plinth worth having: less than this and a structure may as
/// well sit flush, since the placement sink alone covers the drop.
const MIN_DEPTH: f32 = 1.0;

/// Deepest plinth required. Beyond this the structure is wide enough
/// that a taller podium would read as a mesa rather than a foundation —
/// those are the cases for grading the ground instead (#1007 stage 3).
const MAX_DEPTH: f32 = 6.0;

/// How deep an entry of the given footprint radius must reach below its
/// own origin for no daylight to show under its downhill edge.
///
/// The drop across a footprint scales with its radius; the placement
/// sink already covers part of it.
pub fn required_depth(clearance: f32) -> f32 {
    let drop = FOOTPRINT_DROP_RATIO * clearance.max(0.0);
    (drop - PLACEMENT_SINK + MARGIN).clamp(MIN_DEPTH, MAX_DEPTH)
}

/// How far below its own origin a built entry's solid geometry actually
/// reaches. `0.0` for an entry that sits entirely at or above grade.
pub fn buried_depth(built: &Generator) -> f32 {
    measure::solids(built)
        .iter()
        .map(|s| s.bounds.min.y)
        .fold(0.0_f32, f32::min)
        .abs()
}

/// Whether an entry is a *building* placed by the seeded wiring, and so
/// has to hold a floor level across uneven ground. Gateways and
/// monuments land on the same ground by the same route, so they are held
/// to the rule too.
///
/// [`StructureRole::Prop`] is deliberately outside it. Props are small
/// repeated clutter — barrels, mailboxes, bollards — with footprints a
/// fraction of a building's, so the drop across one is small (measured
/// median 0.24 m, inside the placement sink) and a prop that beds
/// slightly into a slope reads as natural rather than broken. Plants,
/// patterns and tools are not buildings at all.
pub fn is_settlement_structure(entry: &dyn CatalogueEntry) -> bool {
    matches!(
        entry.role(),
        StructureRole::Landmark
            | StructureRole::Secondary
            | StructureRole::Gateway
            | StructureRole::Monument
    )
}

/// One entry's standing against the rule.
#[derive(Clone, Debug)]
pub struct FoundationReport {
    pub slug: &'static str,
    pub role: StructureRole,
    pub clearance: f32,
    pub required: f32,
    pub actual: f32,
}

impl FoundationReport {
    pub fn shortfall(&self) -> f32 {
        (self.required - self.actual).max(0.0)
    }

    pub fn passes(&self) -> bool {
        // A centimetre of tolerance forgives float drift in the mesher,
        // not a plinth that is genuinely too shallow.
        self.actual >= self.required - 0.01
    }
}

/// Measure every settlement-placeable entry against the rule.
pub fn audit() -> Vec<FoundationReport> {
    crate::catalogue::ENTRIES
        .iter()
        .filter(|e| is_settlement_structure(**e))
        .map(|e| {
            let clearance = e.footprint().clearance;
            FoundationReport {
                slug: e.slug(),
                role: e.role(),
                clearance,
                required: required_depth(clearance),
                actual: buried_depth(&e.build("did:plc:foundationaudit")),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{cuboid_tapered, foundation_block, id_quat, prim, solid};
    use crate::pds::SovereignMaterialSettings;

    #[test]
    fn requirement_scales_with_the_footprint() {
        // A wider building spans more drop on the same hillside, so it
        // needs to reach further down — the whole point of the rule.
        assert!(required_depth(9.0) > required_depth(6.0));
        // And it tracks the measured drop, less the sink already applied.
        let want = FOOTPRINT_DROP_RATIO * 8.0 - PLACEMENT_SINK + MARGIN;
        assert!((required_depth(8.0) - want).abs() < 1e-4);
    }

    #[test]
    fn requirement_is_bounded_at_both_ends() {
        assert!((required_depth(0.0) - MIN_DEPTH).abs() < 1e-4);
        assert!((required_depth(-5.0) - MIN_DEPTH).abs() < 1e-4);
        assert!((required_depth(500.0) - MAX_DEPTH).abs() < 1e-4);
    }

    #[test]
    fn buried_depth_reads_the_plinth_it_is_given() {
        let mut root = foundation_block(4.0, 4.0, [0.0, 0.0], 3.0);
        root.children.push(prim(
            solid(cuboid_tapered(
                [4.0, 2.0, 4.0],
                0.0,
                SovereignMaterialSettings::default(),
            )),
            [0.0, 1.0, 0.0],
            id_quat(),
        ));
        // `foundation_block` puts its underside at -depth.
        assert!(
            (buried_depth(&root) - 3.0).abs() < 0.02,
            "{}",
            buried_depth(&root)
        );
    }

    /// Every shipped building reaches deep enough for the ground it
    /// gets dropped on (#1009).
    ///
    /// The one that matters. Before this rule, 197 of 201 settlement
    /// buildings were too shallow for their own footprint and two thirds
    /// carried no plinth at all, so on any slope the downhill edge stood
    /// clear of the ground. Adding a building without a footing re-opens
    /// that, and this says so by name.
    #[test]
    fn every_shipped_building_is_founded_deep_enough() {
        let rows = audit();
        assert!(
            rows.len() >= 150,
            "only {} settlement buildings found — registry slipped?",
            rows.len()
        );
        let bad: Vec<String> = rows
            .iter()
            .filter(|r| !r.passes())
            .map(|r| {
                format!(
                    "{} ({:?}, clearance {:.1}) reaches {:.2} m, needs {:.2} m",
                    r.slug, r.role, r.clearance, r.actual, r.required
                )
            })
            .collect();
        assert!(
            bad.is_empty(),
            "{} building(s) are too shallow for their footprint:\n  {}\n\
             Give each a `util::footing(..)` sized to its own base, or \
             `footing_disc` for a round one. Run \
             `cargo run --bin render -- --foundation-audit` for the table.",
            bad.len(),
            bad.join("\n  ")
        );
    }

    #[test]
    fn a_structure_sitting_on_grade_reads_as_zero() {
        let above = prim(
            solid(cuboid_tapered(
                [2.0, 2.0, 2.0],
                0.0,
                SovereignMaterialSettings::default(),
            )),
            [0.0, 1.0, 0.0],
            id_quat(),
        );
        assert!(buried_depth(&above) < 0.01);
    }
}
