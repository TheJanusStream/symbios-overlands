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
    /// Wearables (#1086) — items that also land on an avatar rig socket.
    Attachments,
}

impl CatalogueCategory {
    pub const ALL: [Self; 5] = [
        Self::Buildings,
        Self::Plants,
        Self::Patterns,
        Self::Tools,
        Self::Attachments,
    ];

    /// Display label shown as a section header in the catalogue
    /// window.
    pub fn label(self) -> &'static str {
        match self {
            Self::Buildings => "Buildings",
            Self::Plants => "Plants",
            Self::Patterns => "Patterns",
            Self::Tools => "Tools",
            Self::Attachments => "Attachments",
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
    /// Social gateway (#747) — the themed gate every seeded room places
    /// near spawn. Selected by the seeded wiring via `entries_for(theme,
    /// Gateway)`; each `ThemeArchetype` has a bespoke gateway (#749-772),
    /// with the theme-agnostic `civic_gateway` as the cross-theme
    /// fallback. Never part of the settlement Landmark/Secondary/Prop
    /// pools.
    Gateway,
    /// Wearable item (#1086) — an entry the catalogue offers to **wear**
    /// on the local avatar (via [`CatalogueEntry::wear_socket`]) as well as
    /// to place. Never part of any seeded settlement pool, and the only
    /// role whose entries may be themeless by design.
    Attachment,
    /// Owner-identity monument (#975) — the themed monument every seeded
    /// room stands beside its gateway, carrying the room owner's profile
    /// picture on a square panel
    /// (`items::util::pfp_panel`). Selected exactly like
    /// [`Gateway`](Self::Gateway): the seeded wiring asks
    /// `entries_for(theme, Monument)` and falls back to the theme-agnostic
    /// `civic_monument`. Never part of the settlement Landmark/Secondary/
    /// Prop pools.
    Monument,
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
            Self::Gateway => "Gateway",
            Self::Monument => "Monument",
            Self::Attachment => "Attachment",
        }
    }

    /// UI section this role displays under. Keeps [`CatalogueCategory`]
    /// a derived view of [`StructureRole`] so there's one taxonomy.
    pub fn category(self) -> CatalogueCategory {
        match self {
            Self::Landmark | Self::Secondary | Self::Prop | Self::Gateway | Self::Monument => {
                CatalogueCategory::Buildings
            }
            Self::Attachment => CatalogueCategory::Attachments,
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

/// A measurement-fit declaration on a wearable entry (#1089): which body
/// measurement the worn subtree is scaled to match, and the authored
/// dimension that measurement replaces. The scale is uniform and computed at
/// dress time from the wearer's *built* body (`src/player/attachments.rs`,
/// `fit_scale`), so the same record fits every head it lands on; the
/// authored size stays what world placement and the decor sheets show, and
/// what a body whose measurement is unavailable falls back to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WearFit {
    /// A band encircling the head at the hat line — a circlet, a crown, a
    /// hat brim. `inner_diameter` is the authored band's inner diameter in
    /// metres; worn, the subtree is scaled so that diameter matches the
    /// wearer's brow circumference / π (the equivalent-circle diameter of
    /// the head's widest line above the eyes).
    ///
    /// **Authoring convention: the band circles the origin**, in the X–Z
    /// plane at `y = 0`, ornament rising above it — because the fitted
    /// seat places the attach origin on the head's axis at the measured
    /// hat line (`src/player/attachments.rs`, `fitted_seat`), not at the
    /// generic engine crown seat (which stands well forward of the head).
    HeadBand {
        /// Authored inner diameter of the band, metres.
        inner_diameter: f32,
    },
}

impl WearFit {
    /// The wire form of the fit dimension: the authored band inner diameter
    /// in whole millimetres, the unit
    /// [`AttachmentRecord`](crate::pds::avatar::AttachmentRecord) carries
    /// (atproto records hold no floats). `0` is the wire's "no fit".
    pub fn band_mm(&self) -> u32 {
        match self {
            Self::HeadBand { inner_diameter } => (inner_diameter.max(0.0) * 1000.0).round() as u32,
        }
    }
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

    /// Material re-skins this entry offers (#910). A
    /// [`StructureRole::Plant`] entry lists the bark/foliage palettes its
    /// one grammar can wear, so a single skeleton covers several biomes —
    /// see the `items::plants::variant` module. The seeded species pools name a
    /// variant per biome; an unnamed or unknown one falls back to the
    /// entry's authored materials. Defaults to empty (no re-skins).
    fn variants(&self) -> &'static [items::plants::variant::PlantVariant] {
        &[]
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

    /// The rig socket this entry lands on when **worn** (#1086/#1087).
    /// `Some` marks the entry wearable: the catalogue's detail panel offers
    /// Wear beside the usual drag-to-place, and wearing writes the built
    /// generator into the wardrobe as an
    /// [`AttachmentRecord`](crate::pds::avatar::AttachmentRecord) at this
    /// socket with an identity offset — the sentinel that lets the engine
    /// seat the prop against the measured body surface. Wearable entries
    /// use [`StructureRole::Attachment`] and the pairing is guard-tested in
    /// [`items`]. Attachment-ness is overlands-only metadata: the avatar
    /// engine stays attachment-agnostic, and a wearable entry remains
    /// placeable in the world like any other item. Defaults to `None`.
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        None
    }

    /// The measurement-fit this entry declares when **worn** (#1089).
    /// `Some` means the worn subtree is scaled uniformly at dress time so
    /// the declared authored dimension matches the wearer's measured body
    /// (see [`WearFit`]); the Wear path copies the declaration onto the
    /// [`AttachmentRecord`](crate::pds::avatar::AttachmentRecord) so peers
    /// dress the same fit from the wire. Meaningless without
    /// [`Self::wear_socket`]. Defaults to `None` — worn at authored size.
    fn wear_fit(&self) -> Option<WearFit> {
        None
    }
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
        assert_eq!(Gateway.category(), Buildings);
        assert_eq!(Monument.category(), Buildings);
        assert_eq!(Attachment.category(), Attachments);
    }

    /// Every `ThemeArchetype` a seeded room can be must resolve to exactly
    /// one bespoke gateway via `entries_for(theme, Gateway)` (#749-772), so
    /// the seeded wiring never has to fall through to the cross-theme
    /// `civic_gateway`. Two gates matching one theme would make the picked
    /// gate registry-order-dependent; zero would strand that theme on the
    /// fallback. Adding a theme without a gateway trips this.
    #[test]
    fn every_theme_resolves_to_exactly_one_gateway() {
        for theme in ThemeArchetype::ALL {
            let gates: Vec<&str> = entries_for(theme, StructureRole::Gateway)
                .map(|e| e.slug())
                .collect();
            assert_eq!(
                gates.len(),
                1,
                "{theme:?} must have exactly one bespoke gateway, got {gates:?}"
            );
        }
        // The cross-theme fallback exists, is themeless (so it never enters
        // the per-theme query above), and is Gateway-role.
        let fallback = by_slug("civic_gateway").expect("civic_gateway fallback registered");
        assert_eq!(fallback.role(), StructureRole::Gateway);
        assert!(
            fallback.themes().is_empty(),
            "the fallback must be themeless so it stays out of entries_for"
        );
    }

    /// The same contract for the owner monument (#975): every
    /// `ThemeArchetype` resolves to exactly one bespoke monument, so no
    /// seeded room ever falls through to `civic_monument`.
    ///
    /// This is the test that makes "every room has the owner's face on it"
    /// true rather than aspirational. Zero for a theme strands it on the
    /// cross-theme fallback; two makes the pick registry-order-dependent, so
    /// the same room could show a different monument after an unrelated
    /// registry edit.
    #[test]
    fn every_theme_resolves_to_exactly_one_monument() {
        for theme in ThemeArchetype::ALL {
            let found: Vec<&str> = entries_for(theme, StructureRole::Monument)
                .map(|e| e.slug())
                .collect();
            assert_eq!(
                found.len(),
                1,
                "{theme:?} must have exactly one bespoke monument, got {found:?}"
            );
        }
        let fallback = by_slug("civic_monument").expect("civic_monument fallback registered");
        assert_eq!(fallback.role(), StructureRole::Monument);
        assert!(
            fallback.themes().is_empty(),
            "the fallback must be themeless so it stays out of entries_for"
        );
    }

    /// A monument is never settlement dressing. The deriver fills its
    /// Landmark / Secondary / Prop slots by role, so a monument that also
    /// claimed one of those roles could be scattered through the settlement
    /// as ordinary content — several owner portraits in one room, which is
    /// the one thing this system must not do.
    #[test]
    fn monuments_stay_out_of_the_settlement_pools() {
        for theme in ThemeArchetype::ALL {
            for role in [
                StructureRole::Landmark,
                StructureRole::Secondary,
                StructureRole::Prop,
            ] {
                for e in entries_for(theme, role) {
                    assert!(
                        !e.slug().ends_with("_monument"),
                        "{} is in the {role:?} pool for {theme:?}",
                        e.slug()
                    );
                }
            }
        }
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
