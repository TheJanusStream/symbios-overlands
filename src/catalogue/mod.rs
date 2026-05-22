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

    /// Section bucket — drives the row grouping in the catalogue
    /// window.
    fn category(&self) -> CatalogueCategory;

    /// Build a fresh, independent [`Generator`] tree. Most entries
    /// are pure and ignore `local_did`; the personalisable ones
    /// ([`items::my_teleporter::MyTeleporter`]) stamp the local
    /// user's DID into a slot inside the generator so the resulting
    /// blueprint is pre-targeted at the caller. Every call still
    /// returns a fresh deep-cloned tree — the parameter only changes
    /// what literal values populate it, never aliasing.
    fn build(&self, local_did: &str) -> Generator;
}
