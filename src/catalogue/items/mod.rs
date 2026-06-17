//! Catalogue item registry. Entries live in per-theme subfolders
//! (`ancient`, `medieval`, …) for structures and per-role subfolders
//! (`plants`, `patterns`, `tools`) for everything else. Adding a new
//! entry is three steps: drop the file in the right subfolder, declare
//! it in that subfolder's `mod.rs`, and append `&path::Type` to
//! [`ENTRIES`].
//!
//! The flat [`ENTRIES`] list with categorisation via
//! [`super::CatalogueCategory`] (itself derived from
//! [`super::StructureRole`]) lets us re-bucket entries in the UI without
//! moving files — see the parent module's docstring for the rationale.

use super::CatalogueEntry;

pub mod ancient;
pub mod cyberpunk;
pub mod medieval;
pub mod patterns;
pub mod plants;
pub mod tools;

mod util;

#[cfg(test)]
mod shape_grammar_test;

/// The full set of catalogue entries the client ships with. Order is
/// preserved by the UI for display, so think of this as the
/// presentation order within each section.
pub const ENTRIES: &[&dyn CatalogueEntry] = &[
    // Buildings — architectural entries (shape-grammar and
    // primitive-built), grouped into per-theme subfolders.
    &ancient::villa::Villa,
    &medieval::medieval_castle::MedievalCastle,
    &medieval::watchtower::Watchtower,
    &ancient::ruined_temple::RuinedTemple,
    &ancient::lighthouse::Lighthouse,
    &ancient::stone_circle::StoneCircle,
    &ancient::ziggurat::Ziggurat,
    &ancient::observatory::Observatory,
    // Buildings — Cyberpunk theme (landmark + secondaries + props).
    &cyberpunk::neon_megatower::NeonMegatower,
    &cyberpunk::data_spire::DataSpire,
    &cyberpunk::arcade_block::ArcadeBlock,
    &cyberpunk::holo_billboard::HoloBillboard,
    &cyberpunk::parking_stack::ParkingStack,
    &cyberpunk::neon_kiosk::NeonKiosk,
    &cyberpunk::drone_perch::DronePerch,
    &cyberpunk::cable_arch::CableArch,
    // Plants — L-system tree entries.
    &plants::lsys_monopodial_tree::MonopodialTree,
    &plants::lsys_sympodial_tree::SympodialTree,
    &plants::lsys_ternary_gravity::TernaryGravityTree,
    &plants::lsys_ternary_props::TernaryPropsTree,
    // Patterns — abstract L-system / ABOP demos.
    &patterns::lsys_branching::BranchingPattern,
    &patterns::lsys_koch_island::QuadraticKochIsland,
    &patterns::lsys_sierpinski::SierpinskiGasket,
    // Tools — utility items personalised at build time.
    &tools::my_teleporter::MyTeleporter,
];

/// Resolve a slug to its entry. Returns `None` if the slug doesn't
/// match any current entry — the drop handler treats that as a
/// silently-dropped stale drag (renaming a slug between sessions, or
/// a record referencing a removed entry, both land here).
pub fn by_slug(slug: &str) -> Option<&'static dyn CatalogueEntry> {
    ENTRIES.iter().copied().find(|e| e.slug() == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<&str> = ENTRIES.iter().map(|e| e.slug()).collect();
        slugs.sort();
        let len_before = slugs.len();
        slugs.dedup();
        assert_eq!(
            len_before,
            slugs.len(),
            "duplicate slug in catalogue ENTRIES — slugs must be unique"
        );
    }

    #[test]
    fn by_slug_resolves_every_entry() {
        for entry in ENTRIES {
            let resolved = by_slug(entry.slug());
            assert!(resolved.is_some(), "by_slug failed for {}", entry.slug());
        }
        assert!(by_slug("not-a-real-entry").is_none());
    }

    #[test]
    fn settlement_structures_are_themed() {
        use crate::catalogue::StructureRole::{Landmark, Prop, Secondary};
        for e in ENTRIES {
            if matches!(e.role(), Landmark | Secondary | Prop) {
                assert!(
                    !e.themes().is_empty(),
                    "entry {} has a settlement role but no themes() — the deriver \
                     would never place it",
                    e.slug()
                );
            }
        }
    }

    #[test]
    fn categories_unchanged_after_role_migration() {
        use crate::catalogue::CatalogueCategory::*;
        let count = |c| ENTRIES.iter().filter(|e| e.category() == c).count();
        // Deriving category() from role() must keep every entry in its
        // expected section. 8 ancient/medieval + 8 cyberpunk = 16 buildings.
        assert_eq!(count(Buildings), 16);
        assert_eq!(count(Plants), 4);
        assert_eq!(count(Patterns), 3);
        assert_eq!(count(Tools), 1);
    }
}
