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
use crate::seeded_defaults::{
    EscalationBand, EscalationTier, ProsperityBand, ProsperityTier, ThemeArchetype,
};

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
    /// Human-readable display name — used by the catalogue browser.
    pub fn label(self) -> &'static str {
        match self {
            Self::Landmark => "Landmark",
            Self::Secondary => "Secondary",
            Self::Prop => "Prop",
            Self::Plant => "Plant",
            Self::Pattern => "Pattern",
            Self::Tool => "Tool",
        }
    }

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

/// [`entries_for`] further gated by the room's socio-political tiers: an
/// entry is kept only if its [`CatalogueEntry::prosperity_band`] and
/// [`CatalogueEntry::escalation_band`] both accept the room's tiers. Since
/// both bands default to `ANY`, this returns exactly the same set as
/// [`entries_for`] until entries opt into a band — letting the settlement
/// deriver thread the room's prosperity/escalation through without any
/// selection change for untagged content.
pub fn entries_for_room(
    theme: ThemeArchetype,
    role: StructureRole,
    prosperity: ProsperityTier,
    escalation: EscalationTier,
) -> impl Iterator<Item = &'static dyn CatalogueEntry> {
    entries_for(theme, role).filter(move |e| {
        e.prosperity_band().accepts(prosperity) && e.escalation_band().accepts(escalation)
    })
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

    /// Prosperity-tier span this entry suits (e.g. a scrap shanty is
    /// `Poor`, a marble fountain is `Rich`). Defaults to
    /// [`ProsperityBand::ANY`] so an untagged entry is eligible in rooms of
    /// any prosperity. Consulted by [`entries_for_room`].
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::ANY
    }

    /// Escalation-tier span this entry suits (e.g. a barricade is
    /// `Conflict`, a market stall is `Calm`). Defaults to
    /// [`EscalationBand::ANY`] so an untagged entry is eligible in rooms of
    /// any escalation. Consulted by [`entries_for_room`].
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::ANY
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

    #[test]
    fn socio_bands_default_to_any() {
        // An entry that doesn't override the band methods must accept every
        // tier — this is what keeps theme structures (which never tag a
        // band) eligible regardless of a room's prosperity / escalation.
        struct Bare;
        impl CatalogueEntry for Bare {
            fn slug(&self) -> &'static str {
                "bare"
            }
            fn name(&self) -> &'static str {
                "Bare"
            }
            fn description(&self) -> &'static str {
                ""
            }
            fn build(&self, _local_did: &str) -> Generator {
                Generator::default_cuboid()
            }
        }
        assert_eq!(Bare.prosperity_band(), ProsperityBand::ANY);
        assert_eq!(Bare.escalation_band(), EscalationBand::ANY);
    }

    #[test]
    fn room_query_is_the_band_filtered_theme_query() {
        // entries_for_room is exactly entries_for with the band predicate:
        // an entry survives iff both its bands accept the room's tiers, and
        // it never introduces an entry outside the theme query.
        for theme in ThemeArchetype::ALL {
            for role in [
                StructureRole::Landmark,
                StructureRole::Secondary,
                StructureRole::Prop,
            ] {
                let base: Vec<&str> = entries_for(theme, role).map(|e| e.slug()).collect();
                for p in ProsperityTier::ALL {
                    for x in EscalationTier::ALL {
                        let gated: Vec<&str> = entries_for_room(theme, role, p, x)
                            .map(|e| e.slug())
                            .collect();
                        for s in &gated {
                            assert!(base.contains(s), "room query introduced {s}");
                        }
                        for e in entries_for(theme, role) {
                            let accepted =
                                e.prosperity_band().accepts(p) && e.escalation_band().accepts(x);
                            assert_eq!(
                                accepted,
                                gated.contains(&e.slug()),
                                "{} band/membership mismatch at {p:?}/{x:?}",
                                e.slug()
                            );
                        }
                    }
                }
            }
        }
    }
}
