//! Styled vehicle part kits — crafted variants and ornaments for the boat /
//! airship / skiff families.
//!
//! Fills the previously-empty optional vehicle slots ([`PartSlot::Bow`](super::PartSlot::Bow) /
//! [`PartSlot::Stack`](super::PartSlot::Stack) / [`PartSlot::Exhaust`](super::PartSlot::Exhaust) /
//! [`PartSlot::Ornament`](super::PartSlot::Ornament)) and
//! adds style-specific variants for the body slots, plus cross-family
//! ornaments. Tagged by style and by ornateness / wear bands, so a steam funnel
//! only appears on a steampunk / industrial craft, a neon strip on a cyberpunk
//! one, and so on. Geometry uses the shared primitive vocabulary with torture
//! shaping; finish comes from the seeded
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit).
//!
//! Every mood group (see the group consts) houses at least one of the 23
//! [`ThemeArchetype`]s, and every optional slot ships a **style-universal**
//! floor part (`boat_bow_bowsprit` / `boat_stack_vent` / `skiff_exhaust_tailpipe`
//! / `veh_orn_finial`, all empty-styles) so no theme's optional slots are ever
//! permanently bare — the styled and band-tagged parts then layer flavour on
//! top of that floor (#792).

use crate::seeded_defaults::ChassisFamily;
use crate::seeded_defaults::ThemeArchetype::{
    self, AlienMonolithic, AlienOrganic, AncientClassical, CivicCampus, CoastalResort, Cyberpunk,
    Fantasy, FeudalJapan, GothicHorror, IndustrialPark, Medieval, Mesoamerican, ModernCity, Nordic,
    PostApoc, Roadside, RuralFarmland, Solarpunk, SpaceOutpost, SportsRec, Steampunk, Suburban,
    WildWest,
};
use crate::seeded_defaults::{OrnatenessBand, OrnatenessTier, WearBand, WearTier};

use super::PartCtx;

mod airship;
mod boat;
mod kits;
mod ornaments;
mod skiff;

// The per-family, cross-family, and bespoke-kit `PartDef` statics live in the
// submodules; glob them in so the shared `ENTRIES` registry below can list them
// and `parts::vehicle::ENTRIES` stays a single flat slice.
use airship::*;
use boat::*;
use kits::*;
use ornaments::*;
use skiff::*;

const BOAT: &[ChassisFamily] = &[ChassisFamily::Boat];
const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];
const VEHICLES: &[ChassisFamily] = &[
    ChassisFamily::Boat,
    ChassisFamily::Airship,
    ChassisFamily::Skiff,
];

// Mood groups — the vehicle-styling taxonomy. Each of the 23 `ThemeArchetype`s
// belongs to at least one group so no population is a "desert" with zero styled
// parts (#792). A theme may sit in several (a grimy neon craft is both NEON and
// GRUBBY); a part draws the group whose read it wants. NEON/STEAM/MARTIAL/REGAL/
// GRUBBY/HISTORIC are the originals, widened to fold the desert themes that fit
// them: FeudalJapan / Mesoamerican / GothicHorror → HISTORIC (old-world / ritual);
// AlienOrganic → NEON (bioluminescent); Roadside / RuralFarmland / Suburban →
// GRUBBY (worn, workaday, off-road ground craft). COASTAL is a new home for the
// seaside / sporting moods that fit none of the originals.
const NEON: &[ThemeArchetype] = &[
    Cyberpunk,
    SpaceOutpost,
    AlienMonolithic,
    Solarpunk,
    AlienOrganic,
];
const STEAM: &[ThemeArchetype] = &[Steampunk, IndustrialPark, ModernCity];
const MARTIAL: &[ThemeArchetype] = &[Medieval, Nordic, WildWest, PostApoc];
const REGAL: &[ThemeArchetype] = &[Fantasy, AncientClassical, CivicCampus];
// GRUBBY = grimy / worn / workaday ground craft: industrial soot, frontier
// scrap, and the ordinary agrarian / roadside / suburban beaters (buggies,
// knobby tyres, cargo decks). Widened past the original five so the farm /
// roadside / suburban desert themes get bespoke parts without losing the
// steampunk / industrial ones (a straight fold, no re-tag regression).
const GRUBBY: &[ThemeArchetype] = &[
    Steampunk,
    IndustrialPark,
    WildWest,
    PostApoc,
    Cyberpunk,
    Roadside,
    RuralFarmland,
    Suburban,
];
const HISTORIC: &[ThemeArchetype] = &[
    Medieval,
    Nordic,
    WildWest,
    PostApoc,
    Fantasy,
    AncientClassical,
    FeudalJapan,
    Mesoamerican,
    GothicHorror,
];
/// Seaside / leisure / sporting moods — resort cruisers, sport skiffs. Homes
/// CoastalResort / SportsRec, which fit none of the ground-craft groups.
const COASTAL: &[ThemeArchetype] = &[CoastalResort, SportsRec, Solarpunk];
/// Empty style list — a **style-universal** part, eligible for every theme (see
/// the module docstring). Used for the per-slot floor parts that guarantee no
/// optional vehicle slot is ever bare.
const UNIVERSAL: &[ThemeArchetype] = &[];

/// "Fancy" ornateness band (Adorned upward) — a figurehead, a pennant, a crest:
/// a plain avatar never rolls one, so the ornateness tier finally reads on the
/// optional-slot pick rather than every styled part being `ANY`/`ANY` (#792).
const FANCY: OrnatenessBand =
    OrnatenessBand::range(OrnatenessTier::Adorned, OrnatenessTier::Ornate);
/// "Worn or worse" wear band — a battering ram, sooted exhaust pipes: gear that
/// only reads on a used or beaten craft, never a factory-fresh one.
const WORN_PLUS: WearBand = WearBand::range(WearTier::Worn, WearTier::Battered);
/// Battered-only wear band — the beaten-up counterpart parts (a tattered
/// banner), so the top wear tier reads distinctly from merely-worn.
const BATTERED: WearBand = WearBand::only(WearTier::Battered);
/// Pristine-only wear band — the "clean" counterpart parts (a polished aero
/// fairing), so the *bottom* wear tier reads too, not just the worn / battered
/// ends (#793).
const CLEAN: WearBand = WearBand::only(WearTier::Pristine);

// Narrow bespoke-part audiences (#793 mood-group depth) — finer than the broad
// mood groups above, for parts whose read only fits a couple of themes.
/// Longship / dragon-prow craft — a Spine serpent figurehead's home.
const NORSE_FEY: &[ThemeArchetype] = &[Nordic, Fantasy];
/// Working / labouring craft — rope coils, cleats, capstans read on these.
const WORKING: &[ThemeArchetype] = &[
    Nordic,
    Medieval,
    WildWest,
    PostApoc,
    Steampunk,
    IndustrialPark,
];
/// Funereal / temple / old-world craft — a hanging stern lantern's home.
const SEPULCHRAL: &[ThemeArchetype] = &[GothicHorror, FeudalJapan, Medieval];
/// Agrarian / roadside / ordinary-ground craft — the wooden buckboard read
/// (the #793 issue's "RUSTIC", folded into GRUBBY in #792 but kept as a narrow
/// audience here so the buckboard doesn't land on a cyberpunk skiff).
const AGRARIAN: &[ThemeArchetype] = &[RuralFarmland, Roadside, Suburban, WildWest];

/// Blueprint mast height (deck → masthead), or the pre-blueprint nominal (a
/// boat ctx always carries a blueprint; the fallback is defensive, and lets a
/// styled mast still build valid geometry when the round-trip test exercises it
/// against a non-boat seed).
fn mast_height(ctx: &PartCtx) -> f32 {
    ctx.boat().map_or(0.42, |b| b.mast_h)
}

/// Boat deck footprint multipliers `(dw, dl)` — the seeded beam / length over
/// the nominal, so a deck variant scales with its hull like the default deck.
fn deck_dims(ctx: &PartCtx) -> (f32, f32) {
    let (beam, length) = ctx.boat().map_or((0.5, 1.32), |b| (b.beam, b.hull_len));
    (beam / 0.5, length / 1.32)
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Every styled vehicle part. The four style-universal floors (`bowsprit` /
/// `smokestack`-slug's peer `vent` / `tailpipe` / `finial`) sit alongside the
/// mood-tagged and band-tagged variants; the outfit deriver draws from the
/// union, so every theme fills every optional slot from the floor up (#792).
pub(super) static ENTRIES: &[&dyn super::BodyPart] = &[
    &BOW_RAM,
    &BOW_FIGUREHEAD,
    &BOW_BOWSPRIT,
    &SMOKESTACK,
    &STACK_THRUSTERS,
    &STACK_VENT,
    &MAST_SQUARE_RIG,
    &MAST_ANTENNA,
    &MAST_DERRICK,
    &DECK_CARGO,
    &DECK_BENCH,
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
    &BOW_SERPENT,
    &BOW_ROPE_COIL,
    &STACK_STERN_LANTERN,
    &DECK_VERANDA,
    &DECK_BARRELS,
    &DECK_ENGINEWORKS,
    &ORN_DECK_LANTERN,
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
    use crate::seeded_defaults::{OrnatenessTier, WearTier};

    /// The three vehicle families (the humanoid is a separate kit).
    const FAMILIES: [ChassisFamily; 3] = [
        ChassisFamily::Boat,
        ChassisFamily::Airship,
        ChassisFamily::Skiff,
    ];

    #[test]
    fn universal_floors_only_fill_optional_slots() {
        let ctx = PartCtx::for_seed(13);
        for part in ENTRIES {
            assert!(!part.chassis().is_empty(), "{} no chassis", part.slug());
            let a = part.build(&ctx);
            let b = part.build(&ctx);
            assert_eq!(a, b, "{} non-deterministic", part.slug());
            if part.styles().is_empty() {
                // A style-universal part is a per-slot floor; it must fill an
                // OPTIONAL slot for every family it serves (a required slot
                // already carries its universal default — an untagged body-slot
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
        // ornateness/wear tier too — no roll hits an empty pool.
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

    #[test]
    fn every_theme_belongs_to_a_mood_group() {
        // Fold guarantee: every archetype sits in at least one styling group, so
        // it draws at least one styled part *somewhere* (on some chassis / slot),
        // not only the universal floors. It does NOT promise a styled BODY variant
        // on every chassis — e.g. a COASTAL boat still draws the default hull + the
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
    fn ornateness_and_wear_bands_gate_optional_vehicle_parts() {
        // The tier axes are no longer inert (#792): fancy prow / pennant show
        // only on adorned+ craft, the ram / twin pipes only on worn+ craft, and
        // the tattered banner only on battered craft.
        let has = |chassis, slot, style, o, w, slug: &str| {
            parts_for_avatar(chassis, slot, style, o, w).any(|p| p.slug() == slug)
        };
        use ChassisFamily::{Boat, Skiff};
        use OrnatenessTier::{Ornate, Plain};
        use WearTier::{Battered, Pristine, Worn};
        // Fancy figurehead — gated by ornateness (Adorned upward); Fantasy is REGAL.
        assert!(!has(
            Boat,
            PartSlot::Bow,
            Fantasy,
            Plain,
            Worn,
            "boat_bow_figurehead"
        ));
        assert!(has(
            Boat,
            PartSlot::Bow,
            Fantasy,
            Ornate,
            Worn,
            "boat_bow_figurehead"
        ));
        // Battering ram — gated by wear (Worn upward).
        assert!(!has(
            Boat,
            PartSlot::Bow,
            Medieval,
            Ornate,
            Pristine,
            "boat_bow_ram"
        ));
        assert!(has(
            Boat,
            PartSlot::Bow,
            Medieval,
            Ornate,
            Worn,
            "boat_bow_ram"
        ));
        // Sooted twin pipes — worn upward.
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
        // Tattered banner — battered only.
        assert!(!has(
            Boat,
            PartSlot::Ornament,
            Medieval,
            Ornate,
            Worn,
            "veh_orn_tattered"
        ));
        assert!(has(
            Boat,
            PartSlot::Ornament,
            Medieval,
            Ornate,
            Battered,
            "veh_orn_tattered"
        ));
        // The universal floor is always there regardless of tier.
        assert!(has(
            Boat,
            PartSlot::Bow,
            FeudalJapan,
            Plain,
            Pristine,
            "boat_bow_bowsprit"
        ));
    }

    #[test]
    fn steam_boat_can_fit_a_funnel_stack() {
        let stacks: Vec<&str> = parts_for_avatar(
            ChassisFamily::Boat,
            PartSlot::Stack,
            ThemeArchetype::Steampunk,
            OrnatenessTier::Adorned,
            WearTier::Worn,
        )
        .map(|p| p.slug())
        .collect();
        assert!(stacks.contains(&"boat_stack_funnel"), "got {stacks:?}");
    }
}
