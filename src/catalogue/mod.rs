//! Client-shipped catalogue of ready-to-place [`Generator`] blueprints.
//!
//! The catalogue is functionally analogous to a read-only
//! [`crate::pds::InventoryRecord`]: a flat list of named generator
//! blueprints the user can drag into a room or pick from an "Add from
//! Catalogue…" menu. The difference is purely sourcing — inventory
//! entries are user-authored and PDS-published; catalogue entries are
//! code-shipped, deterministic, and always present.
//!
//! Each entry lives in its own file under [`items`] and implements
//! the [`CatalogueEntry`] trait. The [`items::ENTRIES`] registry
//! aggregates them into a single `&'static [&'static dyn CatalogueEntry]`
//! that the UI and drag-drop handlers iterate over.
//!
//! Lookup by stable slug ([`by_slug`]) is the contract between the UI
//! and the drop handler: the catalogue window stamps the picked
//! entry's slug into the `PendingGeneratorDrop`, and the drop handler
//! resolves it back to the entry when the release lands on the
//! viewport. Slugs are stable across builds; renaming an entry must
//! preserve the slug or older drag-in-flight state would silently
//! resolve to the wrong blueprint.

pub mod items;

pub use items::{ENTRIES, by_slug};

use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

/// Top-level grouping for catalogue items. Used by the catalogue
/// window to section the list — `Buildings` shows the architectural
/// shape entries, `Plants` shows the L-system trees, `Patterns` is
/// for the abstract fractal demos (Koch, Sierpinski, branching),
/// `Tools` is for utility items like portals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CatalogueCategory {
    Buildings,
    Plants,
    Patterns,
    Tools,
}

impl CatalogueCategory {
    pub const ALL: [Self; 4] = [Self::Buildings, Self::Plants, Self::Patterns, Self::Tools];

    /// Display label shown as a section header in the catalogue
    /// window.
    pub fn label(self) -> &'static str {
        match self {
            Self::Buildings => "Buildings",
            Self::Plants => "Plants",
            Self::Patterns => "Patterns",
            Self::Tools => "Tools",
        }
    }
}

/// Functional role of a catalogue entry inside the seeded
/// mini-settlement system. The structure deriver fills each settlement
/// slot ([`Self::Landmark`] / [`Self::Secondary`] / [`Self::Prop`]) by
/// querying the catalogue for entries of the matching role and theme,
/// rather than from a hardcoded slug pool. It also feeds
/// [`CatalogueEntry::category`] so the UI section is derived from the
/// same source of truth — the two taxonomies can't drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StructureRole {
    /// Hero structure — one per themed settlement, anchored near spawn.
    Landmark,
    /// Supporting building ringed around the landmark.
    Secondary,
    /// Small repeated clutter scattered through the settlement.
    Prop,
    /// L-system plant (trees / foliage).
    Plant,
    /// Abstract fractal / ABOP demo.
    Pattern,
    /// Utility item personalised at build time (portals, etc.).
    Tool,
}

impl StructureRole {
    /// UI section this role displays under. Keeps [`CatalogueCategory`]
    /// a derived view of [`StructureRole`] so there's one taxonomy.
    pub fn category(self) -> CatalogueCategory {
        match self {
            Self::Landmark | Self::Secondary | Self::Prop => CatalogueCategory::Buildings,
            Self::Plant => CatalogueCategory::Plants,
            Self::Pattern => CatalogueCategory::Patterns,
            Self::Tool => CatalogueCategory::Tools,
        }
    }
}

/// Physical footprint hints the seeded settlement deriver reads when
/// placing an entry: how far to keep it from the spawn square, and how
/// wide a dry-land clearance the world compiler's water-avoidance walk
/// must honour around it. The default (see [`CatalogueEntry::footprint`])
/// suits a small prop; large structures override with their real extent.
#[derive(Clone, Copy, Debug)]
pub struct Footprint {
    /// Dry-land clearance radius (m) — roughly the structure's
    /// bounding-circle radius around its centred anchor.
    pub clearance: f32,
    /// Minimum distance (m) from the spawn origin, so the spawn scatter
    /// square never lands inside the structure.
    pub min_spawn_dist: f32,
}

/// Every entry of `role` tagged with `theme`, in registry order. The
/// seeded settlement deriver builds its landmark / secondary / prop
/// pools from this rather than a hardcoded slug list, so dropping a
/// themed entry into [`ENTRIES`] grows the settlements automatically.
pub fn entries_for(
    theme: ThemeArchetype,
    role: StructureRole,
) -> impl Iterator<Item = &'static dyn CatalogueEntry> {
    ENTRIES
        .iter()
        .copied()
        .filter(move |e| e.role() == role && e.themes().contains(&theme))
}

/// One catalogue entry. Every implementor lives in its own file under
/// [`items`]; the registry in [`items::ENTRIES`] is the source of
/// truth for what ships in the build.
pub trait CatalogueEntry: Sync {
    /// Stable identifier — written into [`crate::ui::inventory::
    /// PendingGeneratorDrop::generator_name`] when the entry is
    /// dragged. Must remain stable across builds (renaming a slug
    /// would silently misroute drag-state mid-frame).
    fn slug(&self) -> &'static str;

    /// Display name shown in the catalogue window row and the "Add
    /// from Catalogue…" menu.
    fn name(&self) -> &'static str;

    /// One-line tooltip blurb explaining what the entry produces.
    fn description(&self) -> &'static str;

    /// Themes this entry belongs to. An entry may serve several (a
    /// "well" fits both `Medieval` and `RuralFarmland`). The seeded
    /// settlement deriver only considers entries whose list contains the
    /// room's theme. Defaults to empty — a theme-agnostic entry (the
    /// abstract patterns, personalised tools) the settlements never
    /// auto-place.
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[]
    }

    /// Functional role in the seeded mini-settlement. Defaults to
    /// [`StructureRole::Tool`] (the inert bucket) so an entry that opts
    /// out of tagging is never mistaken for placeable settlement content.
    fn role(&self) -> StructureRole {
        StructureRole::Tool
    }

    /// Placement footprint — clearance radius + spawn standoff. Defaults
    /// to a small prop-sized footprint; structures override with their
    /// real extent so the deriver spaces a settlement without overlaps.
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    /// Section bucket — drives the row grouping in the catalogue
    /// window. Derived from [`Self::role`] so the UI grouping and the
    /// settlement taxonomy stay in lockstep; overridable for the rare
    /// entry whose display section differs from its structural role.
    fn category(&self) -> CatalogueCategory {
        self.role().category()
    }

    /// Build a fresh, independent [`Generator`] tree. Most entries
    /// are pure and ignore `local_did`; the personalisable ones
    /// ([`items::tools::my_teleporter::MyTeleporter`]) stamp the local
    /// user's DID into a slot inside the generator so the resulting
    /// blueprint is pre-targeted at the caller. Every call still
    /// returns a fresh deep-cloned tree — the parameter only changes
    /// what literal values populate it, never aliasing.
    fn build(&self, local_did: &str) -> Generator;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_derives_expected_category() {
        use CatalogueCategory::*;
        use StructureRole::*;
        assert_eq!(Landmark.category(), Buildings);
        assert_eq!(Secondary.category(), Buildings);
        assert_eq!(Prop.category(), Buildings);
        assert_eq!(Plant.category(), Plants);
        assert_eq!(Pattern.category(), Patterns);
        assert_eq!(Tool.category(), Tools);
    }
}
