//! Styled vehicle part kits - crafted variants and ornaments for the airship /
//! skiff families. The boat left in #1363: her craft types draw their own
//! geometry off one hull profile, so she fills no slots and the bow / stack /
//! deck / mast kits that dressed her are gone.
//!
//! Fills the previously-empty optional vehicle slots
//! ([`PartSlot::Exhaust`](super::PartSlot::Exhaust) /
//! [`PartSlot::Ornament`](super::PartSlot::Ornament)) and
//! adds style-specific variants for the body slots, plus cross-family
//! ornaments. Tagged by style and by ornateness / wear bands, so a steam funnel
//! only appears on a steampunk / industrial craft, a neon strip on a cyberpunk
//! one, and so on. Geometry uses the shared primitive vocabulary with torture
//! shaping; finish comes from the seeded
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit).
//!
//! Every mood group (see the group consts) houses at least one of the 24
//! [`ThemeArchetype`]s, and every optional slot ships a **style-universal**
//! floor part (`skiff_exhaust_tailpipe` / `veh_orn_finial`, both
//! empty-styles) so no theme's optional slots are ever
//! permanently bare - the styled and band-tagged parts then layer flavour on
//! top of that floor (#792).

use crate::seeded_defaults::ThemeArchetype;
// The mood taxonomy these parts are tagged by lives beside `ThemeArchetype`
// itself, because the craft-type affinities (#1362) read the same groups.
// Imported privately so the submodules keep reaching them as `super::NEON`.
use crate::seeded_defaults::mood::{
    AGRARIAN, COASTAL, GRUBBY, HISTORIC, MARTIAL, NEON, REGAL, STEAM,
};
use crate::seeded_defaults::{ChassisFamily, OrnatenessBand, OrnatenessTier, WearBand, WearTier};

mod airship;
mod kits;
mod ornaments;
mod skiff;

// The per-family, cross-family, and bespoke-kit `PartDef` statics live in the
// submodules; glob them in so the shared `ENTRIES` registry below can list them
// and `parts::vehicle::ENTRIES` stays a single flat slice.
use airship::*;
use kits::*;
use ornaments::*;
use skiff::*;

const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];
/// The vehicle families that still assemble from parts. The boat left in
/// #1363: her craft types draw their own geometry off one hull profile, so she
/// has no slots for a shared ornament to fill.
const VEHICLES: &[ChassisFamily] = &[ChassisFamily::Airship, ChassisFamily::Skiff];

/// Empty style list - a **style-universal** part, eligible for every theme (see
/// the module docstring). Used for the per-slot floor parts that guarantee no
/// optional vehicle slot is ever bare.
const UNIVERSAL: &[ThemeArchetype] = &[];

/// "Fancy" ornateness band (Adorned upward) - a figurehead, a pennant, a crest:
/// a plain avatar never rolls one, so the ornateness tier finally reads on the
/// optional-slot pick rather than every styled part being `ANY`/`ANY` (#792).
const FANCY: OrnatenessBand =
    OrnatenessBand::range(OrnatenessTier::Adorned, OrnatenessTier::Ornate);
/// "Worn or worse" wear band - a battering ram, sooted exhaust pipes: gear that
/// only reads on a used or beaten craft, never a factory-fresh one.
const WORN_PLUS: WearBand = WearBand::range(WearTier::Worn, WearTier::Battered);
/// Battered-only wear band - the beaten-up counterpart parts (a tattered
/// banner), so the top wear tier reads distinctly from merely-worn.
const BATTERED: WearBand = WearBand::only(WearTier::Battered);
/// Pristine-only wear band - the "clean" counterpart parts (a polished aero
/// fairing), so the *bottom* wear tier reads too, not just the worn / battered
/// ends (#793).
const CLEAN: WearBand = WearBand::only(WearTier::Pristine);

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Every styled vehicle part. The four style-universal floors (`bowsprit` /
/// `smokestack`-slug's peer `vent` / `tailpipe` / `finial`) sit alongside the
/// mood-tagged and band-tagged variants; the outfit deriver draws from the
/// union, so every theme fills every optional slot from the floor up (#792).
pub(super) static ENTRIES: &[&dyn super::BodyPart] = &[
    &TEARDROP_ENVELOPE,
    &POD_DUCTED,
    &POD_SCREW,
    &GONDOLA_BASKET,
    &GONDOLA_CARGO,
    &BUBBLE_CANOPY,
    &TWIN_PIPES,
    &EXHAUST_TAILPIPE,
    &CHASSIS_DUNE,
    &CHASSIS_TRIKE,
    &CHASSIS_ARMORED,
    &WHEEL_SPOKED,
    &WHEEL_KNOBBY,
    &WHEEL_GLOW,
    &PENNANT,
    &NEON_STRIP,
    &ORNAMENT_FINIAL,
    &ORNAMENT_TATTERED,
    // #793 bespoke mood-group kits.
    &CANOPY_BUCKBOARD,
    &CANOPY_AERO,
    &CANOPY_TARGA_RACK,
    &ORN_BULL_BAR,
    &ORN_LANTERNS,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::parts::{PartSlot, optional_slots, parts_for, parts_for_avatar};
    // The bare archetype names the expectations below are written in. Scoped
    // to the tests since the mood groups moved out to `seeded_defaults::mood`
    // (#1362) and the registry itself now names only the groups.
    use crate::seeded_defaults::ThemeArchetype::Cyberpunk;
    use crate::seeded_defaults::{OrnatenessTier, WearTier};

    /// The three vehicle families (the humanoid is a separate kit).
    const FAMILIES: [ChassisFamily; 3] = [
        ChassisFamily::Boat,
        ChassisFamily::Airship,
        ChassisFamily::Skiff,
    ];

    #[test]
    fn universal_floors_only_fill_optional_slots() {
        let ctx = super::super::PartCtx::for_seed(13);
        for part in ENTRIES {
            assert!(!part.chassis().is_empty(), "{} no chassis", part.slug());
            let a = part.build(&ctx);
            let b = part.build(&ctx);
            assert_eq!(a, b, "{} non-deterministic", part.slug());
            if part.styles().is_empty() {
                // A style-universal part is a per-slot floor; it must fill an
                // OPTIONAL slot for every family it serves (a required slot
                // already carries its universal default - an untagged body-slot
                // variant would be an authoring slip, not an intentional floor).
                for &fam in part.chassis() {
                    assert!(
                        optional_slots(fam).contains(&part.slot()),
                        "{} is style-universal but fills required slot {:?} for {fam:?}",
                        part.slug(),
                        part.slot()
                    );
                }
            }
        }
    }

    #[test]
    fn every_theme_fills_every_optional_vehicle_slot() {
        // The vehicle analogue of `every_required_slot_is_fillable_for_every_style`
        // (#792): after folding the nine desert themes into mood groups and
        // shipping a style-universal floor per optional slot, no
        // (family, optional slot, theme) query is ever empty.
        for chassis in FAMILIES {
            for &slot in optional_slots(chassis) {
                for style in ThemeArchetype::ALL {
                    assert!(
                        parts_for(chassis, slot, style).next().is_some(),
                        "{chassis:?}/{slot:?}/{style:?} has no vehicle part"
                    );
                }
            }
        }
    }

    #[test]
    fn optional_slots_have_a_floor_at_every_tier() {
        // Stronger than the style-level guarantee: the band-tagged variants
        // layer on top of an `ANY`/`ANY` style-universal floor, so the
        // band-gated pool an avatar actually draws from is non-empty at every
        // ornateness/wear tier too - no roll hits an empty pool.
        for chassis in FAMILIES {
            for &slot in optional_slots(chassis) {
                for style in ThemeArchetype::ALL {
                    for o in OrnatenessTier::ALL {
                        for w in WearTier::ALL {
                            assert!(
                                parts_for_avatar(chassis, slot, style, o, w)
                                    .next()
                                    .is_some(),
                                "{chassis:?}/{slot:?}/{style:?} empty at {o:?}/{w:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    // The buccaneer-kit and pirate-rig tests lived here, pinning
    // `boat_mast_black_colours` / `boat_bow_skull_head` / `boat_deck_gunports`
    // / `boat_mast_square_rig` to the Pirate style. They went with the legacy
    // boat catalogue in #1363: boats are no longer assembled from parts at
    // all, so there are no boat slugs left to pin. The pirate's black colours
    // are a livery and a rig variant now, and belong to #1365 (liveries) and
    // #1366 (rig variants); the guard that they stay pirate-only belongs to
    // #1382 with the rest of the refitted guards. Deliberately NOT rewritten
    // against the sloop - pinning an unfinished design is the trap the
    // geometry-before-instruments rule exists for.

    #[test]
    fn every_theme_belongs_to_a_mood_group() {
        // Fold guarantee: every archetype sits in at least one styling group, so
        // it draws at least one styled part *somewhere* (on some chassis / slot),
        // not only the universal floors. It does NOT promise a styled BODY variant
        // on every chassis - e.g. a COASTAL boat still draws the default hull + the
        // universal floors, since COASTAL's only body part is the sporty skiff
        // canopy; per-chassis body coverage is the bespoke-parts job (#793).
        for style in ThemeArchetype::ALL {
            let grouped = [NEON, STEAM, MARTIAL, REGAL, GRUBBY, HISTORIC, COASTAL]
                .iter()
                .any(|g| g.contains(&style));
            assert!(grouped, "{style:?} belongs to no mood group");
        }
    }

    #[test]
    fn ornateness_and_wear_bands_gate_optional_skiff_parts() {
        // The tier axes are not inert (#792): the sooted twin pipes show only
        // on worn+ craft. The boat halves of this test - the fancy figurehead,
        // the battering ram, the tattered banner, the bowsprit floor - went
        // with the legacy boat catalogue in #1363; boat dressing by ornateness
        // and wear is #1379's, and its guard is #1382's.
        let has = |chassis, slot, style, o, w, slug: &str| {
            parts_for_avatar(chassis, slot, style, o, w).any(|p| p.slug() == slug)
        };
        use ChassisFamily::Skiff;
        use OrnatenessTier::Ornate;
        use WearTier::{Battered, Pristine};
        assert!(!has(
            Skiff,
            PartSlot::Exhaust,
            Cyberpunk,
            Ornate,
            Pristine,
            "skiff_exhaust_twin_pipes"
        ));
        assert!(has(
            Skiff,
            PartSlot::Exhaust,
            Cyberpunk,
            Ornate,
            Battered,
            "skiff_exhaust_twin_pipes"
        ));
    }
}
