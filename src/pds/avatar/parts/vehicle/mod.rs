//! Styled vehicle part kits - crafted variants and ornaments for the airship,
//! the one family still assembled from parts. The boat left in #1363 and the
//! land-skiff in #1364: both families' craft types draw their own geometry off
//! one profile, so neither fills a slot, and the bow / stack / deck / mast
//! kits that dressed a boat and the chassis / canopy / wheel / exhaust kits
//! that dressed a skiff are gone with them.
//!
//! Fills the airship's optional [`PartSlot::Ornament`](super::PartSlot::Ornament)
//! and adds style-specific variants for its body slots. Tagged by style and by
//! ornateness / wear bands, so a flown pennant only appears on a stately
//! craft, a neon strip on a cyberpunk one, and a tattered banner only on a
//! battered one. Geometry uses the shared primitive vocabulary with torture
//! shaping; finish comes from the seeded
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit).
//!
//! Every mood group (see the group consts) houses at least one of the 24
//! [`ThemeArchetype`]s, and the Ornament slot ships a **style-universal**
//! floor part (`veh_orn_finial`, empty styles) so no theme's ornament slot is
//! ever permanently bare - the styled and band-tagged parts then layer flavour
//! on top of that floor (#792). The Exhaust slot had a floor of its own
//! (`skiff_exhaust_tailpipe`) and went with the skiff catalogue: no family
//! fills that slot now.

use crate::seeded_defaults::ThemeArchetype;
// The mood taxonomy these parts are tagged by lives beside `ThemeArchetype`
// itself, because the craft-type affinities (#1362) read the same groups.
// Imported privately so the submodules keep reaching them as `super::NEON`.
use crate::seeded_defaults::mood::{GRUBBY, HISTORIC, NEON, REGAL, STEAM};
use crate::seeded_defaults::{ChassisFamily, OrnatenessBand, OrnatenessTier, WearBand, WearTier};

mod airship;
mod kits;
mod ornaments;

// The per-family, cross-family, and bespoke-kit `PartDef` statics live in the
// submodules; glob them in so the shared `ENTRIES` registry below can list them
// and `parts::vehicle::ENTRIES` stays a single flat slice.
use airship::*;
use kits::*;
use ornaments::*;

const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];

/// Empty style list - a **style-universal** part, eligible for every theme (see
/// the module docstring). Used for the per-slot floor parts that guarantee no
/// optional vehicle slot is ever bare.
const UNIVERSAL: &[ThemeArchetype] = &[];

/// "Fancy" ornateness band (Adorned upward) - a figurehead, a pennant, a crest:
/// a plain avatar never rolls one, so the ornateness tier finally reads on the
/// optional-slot pick rather than every styled part being `ANY`/`ANY` (#792).
const FANCY: OrnatenessBand =
    OrnatenessBand::range(OrnatenessTier::Adorned, OrnatenessTier::Ornate);
/// Battered-only wear band - the beaten-up counterpart parts (a tattered
/// banner), so the top wear tier reads distinctly from merely-worn.
const BATTERED: WearBand = WearBand::only(WearTier::Battered);

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
    &PENNANT,
    &NEON_STRIP,
    &ORNAMENT_FINIAL,
    &ORNAMENT_TATTERED,
    // #793 bespoke mood-group kits.
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
    // The two mood groups only the expectations below name, now that the
    // skiff kits they tagged have gone.
    use crate::seeded_defaults::mood::{COASTAL, MARTIAL};
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
        // it draws at least one styled part *somewhere*, not only the
        // universal floors. It does NOT promise a styled part on every chassis
        // and slot; since #1363 and #1364 the airship is the only family with
        // parts at all, and the two redesigned families vary by craft TYPE
        // instead (#1362).
        for style in ThemeArchetype::ALL {
            let grouped = [NEON, STEAM, MARTIAL, REGAL, GRUBBY, HISTORIC, COASTAL]
                .iter()
                .any(|g| g.contains(&style));
            assert!(grouped, "{style:?} belongs to no mood group");
        }
    }

    #[test]
    fn ornateness_and_wear_bands_gate_optional_vehicle_parts() {
        // The tier axes are not inert (#792). This used to be shown on the
        // skiff's sooted twin pipes and the boat's fancy figurehead; both
        // families' catalogues have gone (#1363, #1364), so the airship
        // carries the claim now - the same claim, on the one family that still
        // assembles from parts. Per-type dressing by ornateness and wear for
        // the redesigned boats and skiffs is #1379's, and its guard is #1382's.
        let has = |chassis, slot, style, o, w, slug: &str| {
            parts_for_avatar(chassis, slot, style, o, w).any(|p| p.slug() == slug)
        };
        use ChassisFamily::Airship;
        use OrnatenessTier::{Ornate, Plain};
        use WearTier::{Battered, Pristine};
        // Wear gates the tattered banner: a battered craft flies one and a
        // factory-fresh one does not.
        assert!(!has(
            Airship,
            PartSlot::Ornament,
            Cyberpunk,
            Ornate,
            Pristine,
            "veh_orn_tattered"
        ));
        assert!(has(
            Airship,
            PartSlot::Ornament,
            Cyberpunk,
            Ornate,
            Battered,
            "veh_orn_tattered"
        ));
        // And ornateness gates the flown pennant on a craft whose style has
        // one at all: a plain ship never rolls a flourish.
        use crate::seeded_defaults::ThemeArchetype::Fantasy;
        assert!(!has(
            Airship,
            PartSlot::Ornament,
            Fantasy,
            Plain,
            Pristine,
            "veh_orn_pennant"
        ));
        assert!(has(
            Airship,
            PartSlot::Ornament,
            Fantasy,
            Ornate,
            Pristine,
            "veh_orn_pennant"
        ));
    }
}
