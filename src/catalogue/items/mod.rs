//! Catalogue item registry. Adding a new entry is two lines:
//! declare the submodule and append `&item::Type` to [`ENTRIES`].
//!
//! The flat list with categorisation via [`super::CatalogueCategory`]
//! lets us re-bucket entries without moving files — see the parent
//! module's docstring for the rationale.

use super::CatalogueEntry;

pub mod lsys_branching;
pub mod lsys_koch_island;
pub mod lsys_monopodial_tree;
pub mod lsys_sierpinski;
pub mod lsys_sympodial_tree;
pub mod lsys_ternary_gravity;
pub mod lsys_ternary_props;
pub mod medieval_castle;
pub mod villa;

/// The full set of catalogue entries the client ships with. Order is
/// preserved by the UI for display, so think of this as the
/// alphabetic-within-category presentation order.
pub const ENTRIES: &[&dyn CatalogueEntry] = &[
    // Buildings — architectural shape-grammar entries.
    &villa::Villa,
    &medieval_castle::MedievalCastle,
    // Plants — L-system tree entries.
    &lsys_monopodial_tree::MonopodialTree,
    &lsys_sympodial_tree::SympodialTree,
    &lsys_ternary_gravity::TernaryGravityTree,
    &lsys_ternary_props::TernaryPropsTree,
    // Patterns — abstract L-system / ABOP demos.
    &lsys_branching::BranchingPattern,
    &lsys_koch_island::QuadraticKochIsland,
    &lsys_sierpinski::SierpinskiGasket,
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
}
