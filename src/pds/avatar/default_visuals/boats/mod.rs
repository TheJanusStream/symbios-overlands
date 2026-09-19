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
/// tall as she is long. It is resolved against the TOP of a rig, not its
/// masthead - a gunter's yard and a square topsail's topmast stand over the
/// mast they are hoisted on (#1366).
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

/// How a craft is driven, which is what she sounds like (#1383): under sail a
/// boat carries the wash along her hull and the wind in her rig, and only an
/// engine hums. Every other family is engine-driven by construction - the
/// airship's rotors, the skiff's motor - so this is a boat's answer, asked of
/// the craft that is DRAWN (see [`propulsion`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Propulsion {
    /// Sails: no engine note at all.
    Sail,
    /// An engine, whose hum is the family's.
    Engine,
}

impl Propulsion {
    /// What `render --outfit` says of it.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Sail => "under sail",
            Self::Engine => "under power",
        }
    }
}

/// One buildable kind of boat.
pub(super) trait BoatCraft {
    /// This type's hull, from the seeded blueprint: her own plan form and
    /// section depth over dimensions everyone shares. The seed is here
    /// because a type may carry more than one plan form (the sloop's four,
    /// #1366), and which one a boat is built on is a property of the seed.
    fn profile(&self, bp: &BoatBlueprint, seed: u64) -> HullProfile;

    /// Draw her, at the origin, bow `+Z`, in true metres. The caller owns the
    /// root pose.
    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator;

    /// How she drives.
    fn feel(&self) -> BoatFeel;

    /// Where a seeded particle aura issues from, read off the hull.
    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile) -> [f32; 3];

    /// How she is driven, and so what she sounds like (#1383). Required, with
    /// no default: a type that lands without saying whether she has an engine
    /// does not compile, so no craft inherits a voice she never had.
    fn propulsion(&self) -> Propulsion;
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
    Some((craft, craft.profile(&bp, seed)))
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

/// How the boat drawn for `seed` is driven - her voice's answer (#1383).
///
/// Asked of the DRAWN craft, never of the picked type: until #1369-#1373 land,
/// most boat seeds pick a type nothing builds yet and are drawn as the sloop,
/// and a sloop's sails must not hum like the runabout a seed picked. Total
/// over every seed, as [`craft_for`] is.
pub(super) fn propulsion(seed: u64) -> Propulsion {
    craft_for(seed).propulsion()
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

    /// Every boat-family builder's inputs that the guards below sweep: the
    /// five blueprint corners, crossed with every rig, every hull form and
    /// every ornateness-by-wear pair a seed can roll - which is all of them,
    /// since the two axes are drawn independently. Returns the built tree and
    /// a label for the failure message.
    fn every_sloop() -> Vec<(Generator, String)> {
        use crate::seeded_defaults::{OrnatenessTier, SloopHull, SloopRig, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let mut out = Vec::new();
        for bp in corners() {
            for form in SloopHull::ALL {
                let hull = sloop::profile_of(&bp, form);
                for rig in SloopRig::ALL {
                    for o in OrnatenessTier::ALL {
                        for w in WearTier::ALL {
                            (ctx.ornateness, ctx.wear) = (o, w);
                            out.push((
                                sloop::build_rigged(&ctx, &hull, rig),
                                format!(
                                    "a {} m {} sloop, {}, {} / {}",
                                    hull.loa,
                                    form.label(),
                                    rig.label(),
                                    o.label(),
                                    w.label()
                                ),
                            ));
                        }
                    }
                }
            }
        }
        out
    }

    /// Nothing a seeded boat carries stands over the air-draft cap (#1359
    /// rule 6) - the hard clash constraint of the whole redesign, since
    /// visuals carry no colliders and a spar over it sails straight through a
    /// gateway lintel.
    ///
    /// EVERY RIG is checked on every boat seed, not only the rig the seed
    /// drew, and each is checked twice: on its own derivation (the height the
    /// cap was resolved against) and on the tree it actually DRAWS. The
    /// second is the one that matters for a rig whose highest point is not
    /// its masthead - a gunter's yard, a square topsail's topmast - because a
    /// rig that resolved its mast against the cap and then crossed a spar
    /// over it would pass the first and sail through a lintel (#1366). Every
    /// blueprint corner a seed can reach is in the population sweep, including
    /// the small end, where a floor after a cap is how a cap gets lost.
    #[test]
    fn no_seeded_boat_stands_over_the_air_draft_cap() {
        use super::super::common::touch;
        use crate::seeded_defaults::SloopRig;
        let mut worst = [0.0f32; SloopRig::ALL.len()];
        let mut checked = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let (_, hull) = hull_for(s).expect("a boat seed has a hull");
            let ctx = PartCtx::for_seed(s);
            for (i, rig) in SloopRig::ALL.into_iter().enumerate() {
                let derived = sloop::top_of_rig(&hull, rig) + hover(hull.draft);
                let drawn =
                    touch::highest(&sloop::build_rigged(&ctx, &hull, rig)) + hover(hull.draft);
                assert!(
                    derived.max(drawn) <= AIR_DRAFT_CAP,
                    "seed {s}, {}: the rig was resolved to {derived} m over the \
                     ground and draws to {drawn} m, past the {AIR_DRAFT_CAP} m cap",
                    rig.label()
                );
                worst[i] = worst[i].max(derived);
            }
            checked += 1;
        }
        assert!(checked > 100, "too few boats sampled: {checked}");
        // And it is a real bound on EVERY rig rather than a vacuous one: the
        // cap actually binds each of them somewhere in the population.
        for (rig, worst) in SloopRig::ALL.into_iter().zip(worst) {
            assert!(
                worst > AIR_DRAFT_CAP - AIR_DRAFT_MARGIN - 0.01,
                "the tallest {} only reaches {worst} m - the cap binds on no \
                 boat carrying it, so this test proves nothing about it",
                rig.label()
            );
        }
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
    /// her own blueprint, on every rig, hull form and tier (#1359 rule 8).
    ///
    /// Exact equality still holds, and that is because no node here carries
    /// a rotation except the root's 180 degree travel yaw, which is applied
    /// after this: the sanitiser renormalises quaternions, which moves the
    /// last ulp of almost any other rotation (#1364). A rig that authors one
    /// has to bring the skiffs' epsilon compare with it.
    #[test]
    fn a_boat_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        let mut n = 0;
        for (built, what) in every_sloop() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            assert!(built == sanitized, "{what} was rewritten by the sanitiser");
            n += 1;
        }
        assert_eq!(n, 5 * 4 * 5 * 9, "the sweep lost a combination");
    }

    /// Every part of a built boat meets another, and the whole craft is one
    /// connected component - the roadster's guard (#1364), and the sloop's on
    /// every rig, hull form and tier she can roll.
    ///
    /// Swept over the blueprint extremes rather than one seed, because the
    /// failure is size-dependent - anything floored at [`MIN_DIM`] stops
    /// shrinking with the hull, so a part that meets at the nominal size can
    /// come adrift at the small end. See [`super::common::touch`] for why this
    /// cannot be judged by eye, and for its one blind spot: it resolves a
    /// tortured cuboid as its UNDEFORMED box, so for the sails it is green
    /// partly for the wrong reason. That is #1382's to teach it, and why the
    /// sail patch the phase-1 prototype drew is not in the ladder (#1366).
    #[test]
    fn a_boat_is_one_machine_at_her_blueprint_extremes() {
        use super::super::common::touch;
        for (built, what) in every_sloop() {
            touch::assert_one_machine(&built, &what);
        }
    }

    /// A seeded boat's saved record stays well under the soft budget
    /// (#1359 rule 9), at a THIRD of it - the owner raised the sloop's guard
    /// from a quarter to the roadster's fraction for #1366's ladder.
    ///
    /// Two sweeps. The live seeds, as saved - FX emitter and voice included -
    /// and the heaviest thing the family can draw: every rig and hull form at
    /// every blueprint corner on the fullest ladder, Ornate and Battered,
    /// carrying the heaviest FX overhead any live seed carries. Measured
    /// rather than assumed, so a new aura that grows the emitter moves this
    /// too.
    #[test]
    fn a_seeded_boats_record_stays_well_inside_the_budget() {
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        use crate::seeded_defaults::{OrnatenessTier, SloopHull, SloopRig, WearTier};
        let bytes = |t: &Generator| serialized_record_bytes(t).expect("a boat serializes");
        let (mut worst_seed, mut fx_overhead) = (0usize, 0usize);
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let (record, _) = super::super::build_for_seed(s);
            let saved = serialized_record_bytes(&record).expect("a record serializes");
            worst_seed = worst_seed.max(saved);
            fx_overhead = fx_overhead.max(saved.saturating_sub(bytes(&super::build(s, None))));
        }
        assert!(worst_seed > 0 && fx_overhead > 0, "nothing was measured");
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        ctx.ornateness = OrnatenessTier::Ornate;
        ctx.wear = WearTier::Battered;
        let mut worst_corner = 0usize;
        for bp in corners() {
            for form in SloopHull::ALL {
                let hull = sloop::profile_of(&bp, form);
                for rig in SloopRig::ALL {
                    let mut built = sloop::build_rigged(&ctx, &hull, rig);
                    apply_travel_pose(&mut built, TRAVEL_DROP);
                    worst_corner = worst_corner.max(bytes(&built) + fx_overhead);
                }
            }
        }
        for (what, worst) in [
            ("seeded boat", worst_seed),
            ("fully dressed corner", worst_corner),
        ] {
            assert!(
                worst * 3 < SOFT_RECORD_BUDGET_BYTES,
                "the heaviest {what} is {worst} bytes, past a third of the \
                 {SOFT_RECORD_BUDGET_BYTES}-byte soft budget - a craft type is \
                 spending nodes where it should be spending shape"
            );
        }
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
