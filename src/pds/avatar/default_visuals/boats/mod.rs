//! The seeded boat family: one builder per craft type, over one hull profile.
//!
//! Replaces the part-assembled hover-boat pipeline of #778, which drew a hull
//! as a blob iso-surface and bolted a deck, a mast and a funnel onto guessed
//! fractions of it. Nothing could be predicted off that hull, so every trim
//! line floated and every mount needed an embed fudge factor; and at 1.32 m it
//! was a toy beside a 1.7 m person (#1359 diagnosis, owner decision 1).
//!
//! # A type is a builder, not an arrangement
//!
//! [`BoatType`] (#1362) is the discrete pick inside the family, and this is
//! where it becomes geometry. One struct per type implements [`BoatCraft`],
//! one file per type, and [`craft`] is the only match over the enum - the
//! central [`build_for_seed`](super::build_for_seed) just delegates.
//!
//! Only the sloop is built so far. That is stated once, in [`craft`], as an
//! explicit `None` for the types with no implementor rather than an arm that
//! quietly draws something else: [`craft_for`] resolves an unbuilt pick to
//! [`BoatType::UNIVERSAL`] - the sloop is the family's universal floor exactly
//! so it can be that answer - while the PICK itself stays a property of the
//! seed, so `render --outfit` and `render --family-seeds --craft` keep
//! answering for types before anyone has drawn them.
//!
//! # Where a boat sits
//!
//! The visual origin is the hull's **design waterline**. On water, buoyancy
//! rests a floating chassis origin [`TRAVEL_DROP`] above the surface, so the
//! assembler hangs the visual that far under the chassis and the boat floats
//! on her own painted waterline. On land there is no buoyancy plane to meet,
//! so the suspension holds her at [`land_ride_height`] instead - the drop,
//! plus her draft, plus a clearance that keeps the keel off the ground. The
//! two are a pair, and both are derived (#1361).

mod profile;
mod sloop;

pub(crate) use profile::HullProfile;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BoatBlueprint, BoatType, ParticleAura};

/// The boat family's colours, which live in the fleet's one livery home
/// (#1365) rather than beside its geometry - the sloop and every type after
/// her read them through here.
pub(crate) use crate::pds::avatar::livery::{BoatColours, boat_colours};

use super::assemble::apply_travel_pose;

/// Travel-pose drop (m): how far under the chassis origin the assembler hangs
/// the hull's design waterline.
///
/// **Derived, not tuned** (#1361). Buoyancy rests a floating chassis origin
/// exactly `water_rest_length` above the surface (`player::hover_boat`), so
/// dropping the visual by that same distance is the one value that lands the
/// design waterline *on* the water - the boat floats on her own painted
/// waterline instead of wading 0.1 m deep.
pub(super) const TRAVEL_DROP: f32 = crate::config::rover::WATER_REST_LENGTH;

/// Keel clearance over the suspension ground line, as a fraction of the
/// hull's draft - what makes a hover-boat read as *hovering* rather than
/// beached. A fraction rather than a constant so the hover reads the same at
/// any size.
const KEEL_CLEARANCE_FRAC: f32 = 0.25;

/// The tallest anything on a seeded boat may stand above the **ground**, hover
/// included (m).
///
/// Visuals carry no colliders, and the lowest lintel on a seeded gateway is
/// 2.86 m, so a mast over this sails straight through one (#1359 rule 6). It
/// is what makes a realistic bermudan rig impossible at this scale and a gaff
/// or gunter rig the answer: a boat's whole rig has to fit inside a box as
/// tall as she is long.
pub(crate) const AIR_DRAFT_CAP: f32 = 2.8;

/// Headroom (m) a rig leaves under [`AIR_DRAFT_CAP`], so a masthead fitting,
/// a burgee or a vane still has somewhere to go.
pub(crate) const AIR_DRAFT_MARGIN: f32 = 0.10;

/// Smallest dimension anything on a boat is built at (m).
///
/// A hair over the sanitiser's own 0.01 m floor. Every radius and every
/// thickness here scales with the hull, so the smallest seeded boat would
/// otherwise draw a masthead or a shroud *under* that floor - and a part the
/// sanitiser CHANGES fails the round-trip the family owes (#1359 rule 8).
/// Flooring it here instead means the record round-trips untouched at every
/// blueprint extreme.
pub(crate) const MIN_DIM: f32 = 0.011;

/// Floor a dimension at [`MIN_DIM`].
fn dim(v: f32) -> f32 {
    v.max(MIN_DIM)
}

/// Height (m) above flat ground a hull of this `draft` wants its **chassis
/// origin** at.
///
/// The other half of [`TRAVEL_DROP`]: on land there is no buoyancy plane to
/// meet, so the hull is placed by her keel instead. Under the chassis origin
/// go the drop, then the draft, then the clearance.
/// [`boat_locomotion`](super::boat_locomotion) turns this into the suspension
/// rest length that actually holds her there - the two are a pair, and a boat
/// twice the size needs both or she reads as beached.
pub(super) fn land_ride_height(draft: f32) -> f32 {
    TRAVEL_DROP + draft * (1.0 + KEEL_CLEARANCE_FRAC)
}

/// How far the design waterline floats above flat ground on land (m) - the
/// ride height less the drop, and so the height every rig is measured against
/// when it is checked against [`AIR_DRAFT_CAP`].
pub(crate) fn hover(draft: f32) -> f32 {
    land_ride_height(draft) - TRAVEL_DROP
}

/// How a craft type drives: the numbers
/// [`boat_locomotion`](super::boat_locomotion) scales its preset by.
///
/// Per type rather than per hull arrangement, which is what the arrangements
/// used to carry. The sloop's are the old monohull's exactly, so the feel the
/// owner validated in #1361 is unchanged; a real per-type feel sweep is
/// #1381's.
#[derive(Clone, Copy, Debug)]
pub(super) struct BoatFeel {
    /// Mass over the family's 50 kg baseline, before the size re-basing.
    pub(super) mass_factor: f32,
    pub(super) drive_accel: f32,
    pub(super) turn_accel: f32,
    pub(super) linear_damping: f32,
    pub(super) angular_damping: f32,
}

/// One buildable kind of boat.
pub(super) trait BoatCraft {
    /// This type's hull, from the seeded blueprint: her own plan form and
    /// section depth over dimensions everyone shares.
    fn profile(&self, bp: &BoatBlueprint) -> HullProfile;

    /// Draw her, at the origin, bow `+Z`, in true metres. The caller owns the
    /// root pose.
    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator;

    /// How she drives.
    fn feel(&self) -> BoatFeel;

    /// Where a seeded particle aura issues from, read off the hull.
    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile) -> [f32; 3];
}

/// The builder for a craft type, or `None` while nothing implements it.
///
/// The seam, and deliberately not a stub (#1362's own note): a match over a
/// non-empty enum needs an arm per variant, and an arm that drew *something*
/// for an unimplemented type would be a lie the population census could not
/// see. The unbuilt group is named in full, so adding a type is a compile
/// error here until it is listed - which is what each of #1369-#1373 will do.
fn craft(t: BoatType) -> Option<&'static dyn BoatCraft> {
    match t {
        BoatType::Sloop => Some(&sloop::Sloop),
        BoatType::Longship
        | BoatType::SteamTug
        | BoatType::Junk
        | BoatType::Runabout
        | BoatType::Scow => None,
    }
}

/// The builder a seed actually draws with: its own type where that type is
/// built, and the family's universal floor where it is not.
///
/// The pick itself is untouched - [`BoatType::for_seed`] still answers with
/// the longship a Nordic seed rolled, and the readouts still print it. This is
/// only what gets drawn until #1369-#1373 land, and it is why #1363 can go
/// live for every boat seed rather than half of them.
fn craft_for(seed: u64) -> &'static dyn BoatCraft {
    craft(BoatType::for_seed(seed)).unwrap_or_else(|| {
        craft(BoatType::UNIVERSAL).expect("the family's universal floor is always built")
    })
}

/// The seeded hull for `seed`, or `None` for a seed that is not a boat.
fn hull_for(seed: u64) -> Option<(&'static dyn BoatCraft, HullProfile)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.boat().copied())?;
    let craft = craft_for(seed);
    Some((craft, craft.profile(&bp)))
}

/// Assemble the seeded boat for `seed`, posed for travel.
pub(super) fn build(seed: u64, livery: Option<usize>) -> Generator {
    let mut ctx = PartCtx::for_seed(seed);
    ctx.livery = livery;
    let (craft, hull) = hull_for(seed).expect("a boat seed carries a boat blueprint");
    let mut root = craft.build(&ctx, &hull);
    // No scale: since #1363 a boat is authored at the size she is drawn at, so
    // the airship-class bridge the legacy pipeline carried has nothing left to
    // convert.
    apply_travel_pose(&mut root, TRAVEL_DROP);
    root
}

/// How the seeded boat for `seed` drives, and how deep she floats.
pub(super) fn feel_and_draft(seed: u64) -> (BoatFeel, f32) {
    match hull_for(seed) {
        Some((craft, hull)) => (craft.feel(), hull.draft),
        // Defensive: a boat seed always has a blueprint. Falling back to the
        // floor's own feel keeps a locomotion query total rather than panicking
        // in a sanitiser round-trip that exercises the family off-seed.
        None => (
            craft(BoatType::UNIVERSAL)
                .expect("the floor is always built")
                .feel(),
            0.28,
        ),
    }
}

/// Where a seeded boat's particle aura issues from (root-local, before the
/// travel pose) - read off her own hull rather than from a constant, so a
/// wake leaves the transom of the boat that is actually drawn.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, hull) = hull_for(seed)?;
    Some(craft.fx_mount(aura, &hull))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ChassisFamily;

    /// The seam (#1362 → #1363). `BoatType::implemented` is what the readouts
    /// and the fan-out slices ask; [`craft`] is what actually draws. They are
    /// two matches over one enum, so pin them together - the failure they
    /// prevent is a type that says it is built and silently draws a sloop.
    #[test]
    fn a_type_is_implemented_exactly_when_something_builds_it() {
        for t in BoatType::ALL {
            assert_eq!(
                craft(t).is_some(),
                t.implemented(),
                "{t:?}: `implemented()` and the builder table disagree"
            );
        }
        assert!(
            BoatType::UNIVERSAL.implemented(),
            "the universal floor must be built - every unbuilt pick resolves to it"
        );
    }

    /// Every boat seed draws a boat, whatever type it picked. This is what
    /// "go live for every boat seed" means, and the unbuilt types are the
    /// reason it needs saying.
    #[test]
    fn every_boat_seed_resolves_to_a_built_craft() {
        let mut unbuilt = 0;
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let picked = BoatType::for_seed(s);
            if !picked.implemented() {
                unbuilt += 1;
            }
            let (_, hull) = hull_for(s).expect("a boat seed has a hull");
            assert!(hull.loa > 0.5, "seed {s}: degenerate hull");
        }
        assert!(
            unbuilt > 0,
            "no seed picked an unbuilt type - the floor fallback is untested"
        );
    }

    /// Nothing a seeded boat carries stands over the air-draft cap (#1359
    /// rule 6) - the hard clash constraint of the whole redesign, since
    /// visuals carry no colliders and a mast over it sails straight through a
    /// gateway lintel.
    ///
    /// Checked on the rig's own derivation rather than on the built mesh,
    /// because the derivation is where the cap lives; a mesh walk would only
    /// re-measure what this arithmetic already decides. Every blueprint corner
    /// the seeded band can reach is swept, including the small end, where the
    /// masthead's own lower floor could in principle defeat the cap - a
    /// `.max()` after a `.min()` is exactly how a cap gets quietly lost.
    #[test]
    fn no_seeded_boat_stands_over_the_air_draft_cap() {
        let mut worst: f32 = 0.0;
        let mut checked = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let (_, hull) = hull_for(s).expect("a boat seed has a hull");
            let top = sloop::masthead(&hull) + hover(hull.draft);
            assert!(
                top <= AIR_DRAFT_CAP,
                "seed {s}: her masthead stands {top} m over the ground, past the \
                 {AIR_DRAFT_CAP} m cap"
            );
            worst = worst.max(top);
            checked += 1;
        }
        assert!(checked > 100, "too few boats sampled: {checked}");
        // And it is a real bound rather than a vacuous one: the tallest boat
        // in the population actually approaches it.
        assert!(
            worst > AIR_DRAFT_CAP - 0.5,
            "the tallest seeded boat only reaches {worst} m - the cap is not \
             binding on anything, so this test proves nothing"
        );
    }

    /// The blueprint corners a seeded hull can actually reach - where a
    /// dimension floors or a clamp bites. The smallest hull draws the thinnest
    /// shroud and the finest stem; the largest draws the deepest keel.
    fn corners() -> Vec<BoatBlueprint> {
        [
            (1.60f32, 3.2f32, 0.10f32, 0.92f32),
            (1.60, 3.8, 0.12, 1.08),
            (2.80, 3.5, 0.11, 1.00),
            (4.40, 3.2, 0.12, 1.08),
            (4.40, 3.8, 0.10, 0.92),
        ]
        .iter()
        .map(|&(loa, ratio, fb, sheer)| BoatBlueprint {
            stance: crate::seeded_defaults::VehicleStance::Sleek,
            hull_len: loa,
            beam: loa / ratio,
            freeboard: loa * fb,
            sheer_bow: loa * 0.060 * sheer,
            sheer_stern: loa * 0.025 * sheer,
            draft: loa * 0.100 * sheer,
        })
        .collect()
    }

    /// Every boat survives the record sanitiser UNCHANGED at the extremes of
    /// her own blueprint, not only at the seeds the population happens to
    /// contain (#1359 rule 8).
    #[test]
    fn a_boat_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        let ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let craft = craft(BoatType::UNIVERSAL).expect("the floor is built");
        for bp in corners() {
            let (loa, ratio) = (bp.hull_len, bp.hull_len / bp.beam);
            let built = craft.build(&ctx, &craft.profile(&bp));
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            assert_eq!(
                built, sanitized,
                "a {loa} m hull at L:B {ratio} was rewritten by the sanitiser"
            );
        }
    }

    /// Every part of a built boat meets another, and the whole craft is one
    /// connected component - the roadster's guard (#1364), now the sloop's
    /// too.
    ///
    /// The prototype this hull was ported from was in four pieces - both
    /// forward lit ports hung up to 25 mm off the cabin trunk and the gaff's
    /// throat was marginal against the mast - and #1366 recorded the
    /// reasonable belief that the port had carried the same three defects.
    /// This test is what answered it: the built sloop meets herself at every
    /// corner, and did so BEFORE the reads were tidied. The tidying stands
    /// anyway (a port now reads the trunk's drawn centreline), but the margin
    /// it was holding was a few millimetres of luck, and luck is what a guard
    /// is for. Swept over the blueprint extremes rather than one seed,
    /// because the failure is size-dependent - anything floored at
    /// [`MIN_DIM`] stops shrinking with the hull, so a part that meets at the
    /// nominal size can come adrift at the small end. See
    /// [`super::common::touch`] for why this cannot be judged by eye.
    #[test]
    fn a_boat_is_one_machine_at_her_blueprint_extremes() {
        use super::super::common::touch;
        let ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let craft = craft(BoatType::UNIVERSAL).expect("the floor is built");
        for bp in corners() {
            let built = craft.build(&ctx, &craft.profile(&bp));
            touch::assert_one_machine(&built, &format!("a {} m sloop", bp.hull_len));
        }
    }

    /// A seeded boat's saved record stays well under the soft budget
    /// (#1359 rule 9).
    ///
    /// The old fleet spent its budget on cuboids - a blob hull at resolution
    /// 44 plus a deck of planks plus a rail of them - and one swept node
    /// replaces a great many of those, so the redesigned boat was expected to
    /// come in lighter. This pins that it did, and would catch a type that
    /// grew back toward the cap by adding nodes rather than shaping them.
    #[test]
    fn a_seeded_boats_record_stays_well_inside_the_budget() {
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        let mut worst = 0usize;
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let built = super::build(s, None);
            let bytes = serialized_record_bytes(&built).expect("a built boat serializes");
            worst = worst.max(bytes);
        }
        assert!(worst > 0, "no boat seed was measured");
        assert!(
            worst * 4 < SOFT_RECORD_BUDGET_BYTES,
            "the heaviest seeded boat is {worst} bytes, past a quarter of the \
             {SOFT_RECORD_BUDGET_BYTES}-byte soft budget - a craft type is \
             spending nodes where it should be spending shape"
        );
    }

    /// The hover and the ride height are one derivation, and the waterline is
    /// where the two meet.
    #[test]
    fn the_waterline_floats_a_quarter_of_a_draft_clear_of_the_ground() {
        for draft in [0.12f32, 0.28, 0.45] {
            let ride = land_ride_height(draft);
            assert!((ride - TRAVEL_DROP - hover(draft)).abs() < 1e-6);
            // The keel clears the ground by a quarter of the draft, which is
            // what makes her read as hovering.
            assert!((hover(draft) - draft - 0.25 * draft).abs() < 1e-6);
        }
    }
}
