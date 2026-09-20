//! The seeded land-skiff family: one builder per craft type, over one body
//! plan.
//!
//! Replaces the part-assembled skiff pipeline of #778, which stacked a body
//! out of superellipsoid pillows, hung 8.5 cm fender tubes on a 21 cm wheel,
//! and mounted its canopy at a fixed `y = 0.33` that only one of its four
//! chassis ever reached - so on the other three the cabin hovered in open air.
//! At 1.5 m it was a toy beside a 1.7 m person (#1359 diagnosis, owner
//! decision 1), and its wheels and fenders agreed only because three files
//! repeated the same magic numbers.
//!
//! # A type is a builder, not an arrangement
//!
//! [`SkiffType`] (#1362) is the discrete pick inside the family, and this is
//! where it becomes geometry. One struct per type implements [`SkiffCraft`],
//! one file per type, and [`craft`] is the only match over the enum - the
//! central [`build_for_seed`](super::build_for_seed) just delegates. This is
//! the skiff half of the seam the sloop opened in #1363, and it is written the
//! same way, down to the explicit `None` group for the types with no
//! implementor: [`craft_for`] resolves an unbuilt pick to
//! [`SkiffType::UNIVERSAL`] while the PICK stays a property of the seed.
//!
//! Five types are built: the roadster, the universal floor; since #1377 the
//! horseless [`wagon`], which takes all ten historic themes and is 31 % of the
//! family against the Roadster's 30; since #1374 the dune [`buggy`], on the
//! six leisure and frontier themes, 13 % of it; since #1376 the three-wheeled
//! [`cyclecar`], on the neon themes and the campus - the first type on an
//! unpaired axle; and since #1375 the [`armoured`] car, on the three modern
//! martial themes, 8 % of it - the first type drawn in FACETS rather than in
//! sweeps. A seed that picked the rover, the last unbuilt type, is still
//! drawn as the roadster (13 of the 151 skiff seeds under 600), and the
//! readouts say so.
//!
//! # Where a skiff sits
//!
//! The visual origin is the body's **datum** - the plane the body sweeps are
//! cut on, which is the cockpit coaming line. The ground is
//! [`BodyPlan::datum_height`] below it, and [`travel_drop`] is simply the
//! difference between where the suspension rests the chassis origin and that
//! number, which is what puts the tyres on the suspension's own ground line
//! for the wheels this seed actually rolls on (#1361).

mod armoured;
mod buggy;
mod cyclecar;
mod plan;
mod roadster;
mod shape;
mod wagon;

pub(crate) use plan::BodyPlan;

use crate::pds::avatar::locomotion::CarParams;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{ParticleAura, SkiffBlueprint, SkiffType};

/// The skiff family's colours, which live in the fleet's one livery home
/// (#1365) rather than beside its geometry - the roadster and every type after
/// it read them through here.
pub(crate) use crate::pds::avatar::livery::{SkiffColours, skiff_colours};

use super::Propulsion;
use super::assemble::apply_travel_pose;

/// Smallest dimension anything on a skiff is built at (m).
///
/// A hair over the sanitiser's own 0.01 m floor. Every radius and every
/// thickness here scales with the machine, so the smallest seeded skiff would
/// otherwise draw a bonnet bead or a track rod *under* that floor - and a part
/// the sanitiser CHANGES fails the round-trip the family owes (#1359 rule 8).
/// Flooring it here instead means the record round-trips untouched at every
/// blueprint extreme.
pub(crate) const MIN_DIM: f32 = 0.011;

/// Floor a dimension at [`MIN_DIM`].
fn dim(v: f32) -> f32 {
    v.max(MIN_DIM)
}

/// How a craft type drives: the numbers
/// [`skiff_locomotion`](super::skiff_locomotion) scales its preset by.
///
/// Per type rather than per chassis *class*, which is what the four legacy
/// chassis slugs used to carry. The roadster's are the old default chassis's
/// exactly, so the drive the owner validated in #1361 is the drive that ships;
/// a real per-type feel sweep is #1381's.
#[derive(Clone, Copy, Debug)]
pub(super) struct SkiffFeel {
    /// Mass over the family's 900 kg baseline, before the size re-basing.
    pub(super) mass_factor: f32,
    pub(super) drive_accel: f32,
    pub(super) turn_accel: f32,
}

/// One buildable kind of land craft.
pub(super) trait SkiffCraft {
    /// This type's body, from the seeded blueprint: its own plan form, section
    /// depth and layout over dimensions everyone shares. It takes the seed
    /// because a type can build more than one body and roll on more than one
    /// wheel, and both are properties of the seed that the plan must carry -
    /// the roadster's boat-tail, bobtail and tourer, and the balloon tyre that
    /// raises its axle line (#1367).
    fn plan(&self, bp: &SkiffBlueprint, seed: u64) -> BodyPlan;

    /// Draw it, at the origin, nose `+Z`, in true metres. The caller owns the
    /// root pose.
    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator;

    /// How it drives.
    fn feel(&self) -> SkiffFeel;

    /// The widest the type is actually drawn, guards included (m) - what the
    /// gateway-mouth guard measures, and a type's own knowledge rather than
    /// the plan's, because a guard is the type's choice. It takes the seed
    /// because what stands outermost can be the seed's own pick: a wagon's
    /// naves, bigger on an ox-cart (#1377).
    fn overall_width(&self, plan: &BodyPlan, seed: u64) -> f32;

    /// Where a seeded particle aura issues from, read off the body - and off
    /// the seed's own picks, because a flourish that hovers over an open
    /// cockpit would hover inside a closed one (#1367).
    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan, seed: u64) -> [f32; 3];

    /// How it is driven, and so what it sounds like and whether it trails an
    /// exhaust (#1377). Required, with no default - the boat's rule (#1383):
    /// a type that lands without saying whether it has an engine does not
    /// compile, so no wagon inherits a putter.
    fn propulsion(&self) -> Propulsion;
}

/// The builder for a craft type, or `None` while nothing implements it.
///
/// The seam, and deliberately not a stub (the same reasoning as the boats'):
/// a match over a non-empty enum needs an arm per variant, and an arm that
/// drew *something* for an unimplemented type would be a lie the population
/// census could not see. The unbuilt group is named in full, so adding a type
/// is a compile error here until it is listed - which is what each of
/// #1374-#1378 will do.
fn craft(t: SkiffType) -> Option<&'static dyn SkiffCraft> {
    match t {
        SkiffType::Roadster => Some(&roadster::Roadster),
        SkiffType::Wagon => Some(&wagon::Wagon),
        SkiffType::DuneBuggy => Some(&buggy::Buggy),
        SkiffType::Cyclecar => Some(&cyclecar::Cyclecar),
        SkiffType::ArmouredCar => Some(&armoured::Armoured),
        SkiffType::Rover => None,
    }
}

/// The builder a seed actually draws with: its own type where that type is
/// built, and the family's universal floor where it is not.
fn craft_for(seed: u64) -> &'static dyn SkiffCraft {
    craft(SkiffType::for_seed(seed)).unwrap_or_else(|| {
        craft(SkiffType::UNIVERSAL).expect("the family's universal floor is always built")
    })
}

/// The seeded body for `seed`, or `None` for a seed that is not a skiff.
fn body_for(seed: u64) -> Option<(&'static dyn SkiffCraft, BodyPlan)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.skiff().copied())?;
    let craft = craft_for(seed);
    Some((craft, craft.plan(&bp, seed)))
}

// ---------------------------------------------------------------------------
// Where the machine stands
// ---------------------------------------------------------------------------

/// Half-extents (m) of the chassis collider for this plan.
///
/// Read off the bodywork the craft actually draws, which is what makes it safe
/// at airship-class size. The legacy `0.4 * (body_len / 1.5)` tracked the
/// body's *length*, so a machine scaled to 2.65 m stood in a 1.4 m tall
/// collider - the tall-narrow shape behind the #804 rollovers, on something
/// only 0.7 m high. The width is the DRAWN width rather than the coachwork's,
/// because what a player collides with is the wheels: this era's body is far
/// narrower than its track, and a collider cut to the tub would let the guards
/// pass through a wall.
pub(super) fn chassis_half_extents(craft: &dyn SkiffCraft, plan: &BodyPlan, seed: u64) -> [f32; 3] {
    [
        craft.overall_width(plan, seed) * 0.5,
        plan.depth(),
        plan.length * 0.5,
    ]
}

/// Height (m) above flat ground the skiff's **chassis origin** rests at.
///
/// The four corner springs carry the weight from `half_y` below the origin,
/// compressing by
/// [`static_suspension_compression`](super::static_suspension_compression), so
/// the origin floats that much less than a full suspension rest length above
/// the ground. Its compression term is seed-invariant and its `half_y` is not,
/// which is why [`travel_drop`] has to be derived per craft rather than
/// written down once.
fn chassis_ride_height(craft: &dyn SkiffCraft, plan: &BodyPlan, seed: u64) -> f32 {
    let p = CarParams::default();
    chassis_half_extents(craft, plan, seed)[1] + p.suspension_rest_length.0
        - super::static_suspension_compression(super::SKIFF_REF_MASS, p.suspension_stiffness.0)
}

/// Travel-pose drop (m): how far under the chassis origin the assembler hangs
/// the body's datum.
///
/// **Derived, not tuned** (#1361): the datum sits [`BodyPlan::datum_height`]
/// over the ground by construction, so dropping the visual by the difference
/// is the one value that puts the tyre bottoms exactly on the suspension's
/// ground line - for the wheels THIS seed rolls on, not a nominal pair. The
/// hand-set 0.55 this replaced assumed one nominal wheel and floated or sank
/// them by centimetres across the seeded radius band.
pub(super) fn travel_drop(craft: &dyn SkiffCraft, plan: &BodyPlan, seed: u64) -> f32 {
    chassis_ride_height(craft, plan, seed) - plan.datum_height()
}

/// Assemble the seeded skiff for `seed`, posed for travel.
pub(super) fn build(seed: u64, livery: Option<usize>) -> Generator {
    let mut ctx = PartCtx::for_seed(seed);
    ctx.livery = livery;
    let (craft, plan) = body_for(seed).expect("a skiff seed carries a skiff blueprint");
    let mut root = craft.build(&ctx, &plan);
    // No scale: since #1364 a skiff is authored at the size she is drawn at,
    // so the airship-class bridge the legacy pipeline carried has nothing left
    // to convert - and the skiff was the last family holding one.
    apply_travel_pose(&mut root, travel_drop(craft, &plan, seed));
    root
}

/// How the seeded skiff for `seed` drives, and the collider box it drives in.
pub(super) fn feel_and_box(seed: u64) -> (SkiffFeel, [f32; 3]) {
    match body_for(seed) {
        Some((craft, plan)) => (craft.feel(), chassis_half_extents(craft, &plan, seed)),
        // Defensive: a skiff seed always has a blueprint. Falling back to the
        // floor's own feel keeps a locomotion query total rather than
        // panicking in a sanitiser round-trip that exercises the family
        // off-seed.
        None => {
            let craft = craft(SkiffType::UNIVERSAL).expect("the floor is always built");
            (craft.feel(), [0.66, 0.29, 1.33])
        }
    }
}

/// How far the ground is under the visual origin for `seed` (m), or `None` for
/// a seed that is not a skiff - the number [`travel_drop`] places against, and
/// what the family's pose test checks the assembled tree against.
#[cfg(test)]
pub(super) fn datum_height_for_seed(seed: u64) -> Option<f32> {
    body_for(seed).map(|(_, plan)| plan.datum_height())
}

/// How the skiff `seed` DRAWS is driven (#1377) - the drawn craft's answer,
/// so an unbuilt pick drawn as the roadster keeps the roadster's engine. Any
/// seed answers, as the boats' does: the voice asks before it knows the
/// family is a skiff's.
pub(super) fn propulsion(seed: u64) -> Propulsion {
    craft_for(seed).propulsion()
}

/// Where a seeded skiff's particle aura issues from (root-local, before the
/// travel pose) - read off its own body by the craft type that drew it, so an
/// exhaust wisp leaves the pipe mouth of the machine that is actually there.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, plan) = body_for(seed)?;
    Some(craft.fx_mount(aura, &plan, seed))
}

#[cfg(test)]
mod tests {
    use super::super::common::first_difference;
    use super::*;
    use crate::seeded_defaults::{ChassisFamily, VehicleStance};

    /// The blueprint corners a seeded skiff can actually reach - the smallest
    /// machine draws the thinnest bead and the finest track rod, the largest
    /// the widest guard, and a clamp corner is still a car.
    fn corners() -> Vec<SkiffBlueprint> {
        let mut out = Vec::new();
        for &length in &[1.90f32, 2.65, 3.60] {
            for &(body, track, wheel, belt, height) in &[
                (0.250f32, 0.390f32, 0.110f32, 0.285f32, 0.360f32),
                (0.300, 0.455, 0.125, 0.315, 0.420),
            ] {
                out.push(SkiffBlueprint {
                    stance: VehicleStance::Sleek,
                    length,
                    body_w: length * body,
                    wheelbase: length * 0.64,
                    track: length * track,
                    wheel_r: length * wheel,
                    beltline: length * belt,
                    height: length * height,
                });
            }
        }
        out
    }

    fn a_skiff_seed() -> u64 {
        (0u64..600)
            .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff)
            .expect("some seed is a skiff")
    }

    /// The seam (#1362 → #1364). [`SkiffType::implemented`] is what the
    /// readouts and the fan-out slices ask; [`craft`] is what actually draws.
    /// They are two matches over one enum, so pin them together - the failure
    /// they prevent is a type that says it is built and silently draws a
    /// roadster.
    #[test]
    fn a_skiff_type_is_implemented_exactly_when_something_builds_it() {
        for t in SkiffType::ALL {
            assert_eq!(
                craft(t).is_some(),
                t.implemented(),
                "{t:?}: `implemented()` and the builder table disagree"
            );
        }
        assert!(
            SkiffType::UNIVERSAL.implemented(),
            "the universal floor must be built - every unbuilt pick resolves to it"
        );
    }

    /// Every skiff seed draws a skiff, whatever type it picked. This is what
    /// "go live for every skiff seed" means, and the unbuilt type is the
    /// reason it needs saying: with the roadster, the wagon (#1377), the dune
    /// buggy (#1374), the cyclecar (#1376) and the armoured car (#1375)
    /// built, 13 of the 151 skiff seeds under 600 still pick the one type
    /// nothing draws yet - the rover, whose slice (#1378) is what ends this
    /// `unbuilt > 0`. The boats' form: the floor fallback must still be
    /// exercised, and how much of the family it carries is the census's
    /// business, not a threshold to re-tune each slice.
    #[test]
    fn every_skiff_seed_resolves_to_a_built_craft() {
        let (mut unbuilt, mut total) = (0, 0);
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            if !SkiffType::for_seed(s).implemented() {
                unbuilt += 1;
            }
            total += 1;
            let (_, plan) = body_for(s).expect("a skiff seed has a body");
            assert!(plan.length > 1.0, "seed {s}: degenerate body");
        }
        assert!(total > 50, "too few skiffs sampled: {total}");
        assert!(
            unbuilt > 0,
            "no seed picked an unbuilt type - the floor fallback is untested"
        );
    }

    /// Every roadster the family can draw at the blueprint corners: every
    /// body, top and wheel crossed with every ornateness-by-wear pair a seed
    /// can roll - which is all of them, since the axes are drawn
    /// independently. Returns the built tree and a label for the failure.
    fn every_roadster() -> Vec<(Generator, String)> {
        use crate::seeded_defaults::{
            OrnatenessTier, RoadsterBody, RoadsterTop, RoadsterWheels, WearTier,
        };
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for body in RoadsterBody::ALL {
                for rolls in RoadsterWheels::ALL {
                    let plan = roadster::plan_of(&bp, body, rolls);
                    for top in RoadsterTop::ALL {
                        for o in OrnatenessTier::ALL {
                            for w in WearTier::ALL {
                                (ctx.ornateness, ctx.wear) = (o, w);
                                out.push((
                                    roadster::build_dressed(&ctx, &plan, top, rolls),
                                    format!(
                                        "a {} m {} {} roadster on {} wheels, {} / {}",
                                        bp.length,
                                        top.label(),
                                        body.label(),
                                        rolls.label(),
                                        o.label(),
                                        w.label()
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Every wagon the family can draw at the blueprint corners: every body
    /// crossed with every ornateness-by-wear pair. Returns the built tree, its
    /// plan and a label for the failure.
    fn every_wagon() -> Vec<(Generator, wagon::WagonPlan, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WagonBody, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for body in WagonBody::ALL {
                let plan = wagon::plan_of(&bp, body);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            wagon::build_dressed(&ctx, &plan),
                            plan,
                            format!(
                                "a {} m {}, {} / {}",
                                bp.length,
                                body.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        assert_eq!(out.len(), 6 * 5 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built wagon meets another and the whole machine is one
    /// component, on every body and tier at every blueprint corner (#1377) -
    /// on the tree AS SAVED, through the record's 0.1 mm wire, as the
    /// roadster's is.
    ///
    /// What it cannot see: the guard ignores `hollow`, so it judges a wheel's
    /// bored felloe as a solid disc and a spoke meets it whatever. A spoke's
    /// contact is by construction - each is seated half the felloe's depth
    /// into it (`running_gear::spoked_wheel`).
    #[test]
    fn a_wagon_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_wagon() {
            let json = serde_json::to_string(&built).expect("a wagon serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every wagon survives the record sanitiser UNCHANGED at the extremes of
    /// its own blueprint, on every body and tier (#1359 rule 8) - the bored
    /// wheel bands included, whose `hollow` is held under the sanitiser's
    /// 0.95 cap by construction.
    #[test]
    fn a_wagon_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_wagon() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every wheel of every wagon stands on the ground: each axle's centre is
    /// its OWN wheel's radius over it - the small front pair of a four-wheeler
    /// as much as the big rear one, and the two-wheelers' single pair
    /// (#1377).
    #[test]
    fn every_wagon_wheel_stands_on_the_ground() {
        for (_, plan, what) in every_wagon() {
            for (at, r) in plan.wheels() {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
            }
        }
    }

    /// Every dune buggy the family can draw at the blueprint corners: every
    /// variant crossed with every ornateness-by-wear pair (#1374). Returns the
    /// built tree, its plan and a label for the failure.
    fn every_buggy() -> Vec<(Generator, buggy::BuggyPlan, String)> {
        use crate::seeded_defaults::{BuggyVariant, OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for variant in BuggyVariant::ALL {
                let plan = buggy::plan_of(&bp, variant);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            buggy::build_dressed(&ctx, &plan),
                            plan,
                            format!(
                                "a {} m {}, {} / {}",
                                bp.length,
                                variant.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        assert_eq!(out.len(), 6 * 3 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built dune buggy meets another and the whole machine
    /// is one component, on every variant and tier at every blueprint corner
    /// (#1374) - on the tree AS SAVED, through the record's 0.1 mm wire.
    ///
    /// Nothing on her is fattened to meet: every frame member is drawn
    /// through joints another member also runs through, and a mass hung
    /// between joints sits on the tube's drawn centreline (see the buggy's
    /// module docs). What the guard cannot see: it ignores `hollow` and
    /// `path_cut`, so the pod's bore and the canopy's stripes are judged as
    /// whole tubes, and it samples a turned tyre only at its profile rings
    /// (#1382).
    #[test]
    fn a_buggy_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_buggy() {
            let json = serde_json::to_string(&built).expect("a buggy serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every buggy survives the record sanitiser UNCHANGED at the extremes of
    /// her own blueprint, on every variant and tier (#1359 rule 8) - her
    /// rotated tyres, lamps, cylinder banks, seat backs and spare compared
    /// through the quaternion epsilon the sanitiser's renormalising needs.
    #[test]
    fn a_buggy_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_buggy() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every buggy stands on her wheels, under the air draft and inside the
    /// gateway, at every blueprint corner (#1374): each axle's centre is its
    /// OWN wheel's radius over the ground - the small fronts as much as the
    /// big rears - nothing she draws stands higher than rule 6's 2.8 m over
    /// it, and she is narrower than the 2.6 m mouth.
    ///
    /// The first skiff air-draft guard (owner decision 11): the dune whip
    /// reaches 2.57 m at the largest corner, held there by construction, and
    /// this reads the DRAWN tree rather than the arithmetic that held it.
    #[test]
    fn a_buggy_stands_on_her_wheels_under_the_air_draft() {
        use super::super::common::touch;
        /// Rule 6's air draft and the narrowest gateway mouth (m).
        const AIR_DRAFT: f32 = 2.8;
        const MOUTH: f32 = 2.6;
        for (built, plan, what) in every_buggy() {
            let mut radii = Vec::new();
            for (at, r) in plan.wheels() {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
                if !radii.contains(&r) {
                    radii.push(r);
                }
            }
            assert_eq!(radii.len(), 2, "{what}: her two axles share a radius");
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT,
                "{what}: she stands {top} m over the ground, past the {AIR_DRAFT} m air draft"
            );
            let wide = buggy::Buggy.overall_width(&plan, 0);
            assert!(
                wide < MOUTH,
                "{what}: {wide} m wide, past the {MOUTH} m mouth"
            );
        }
    }

    /// Every dune buggy seed is drawn as a buggy, on the variant her theme
    /// picks, air-cooled (#1374) - and no other skiff seed is: a skiff is
    /// air-cooled exactly when it is drawn as a buggy. And the aura her
    /// record carries - her exhaust, a Roadside buggy's folded steam, or a
    /// frontier theme's embers - leaves her stinger's mouth.
    #[test]
    fn a_buggy_seed_draws_a_buggy() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::BuggyVariant;
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let buggy = SkiffType::for_seed(s) == SkiffType::DuneBuggy;
            assert_eq!(
                propulsion(s) == Propulsion::AirCooled,
                buggy,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !buggy {
                continue;
            }
            let v = BuggyVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
            let (_, plan) = body_for(s).expect("a buggy seed has a body");
            let (record, _) = super::super::build_for_seed(s);
            let emitter = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .find(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .expect("a buggy trails an aura");
            assert_eq!(
                emitter.transform.translation.0,
                buggy::stinger_mouth(&buggy::with_variant(plan, v)),
                "seed {s}: her aura does not leave her stinger's mouth"
            );
        }
        assert_eq!(
            seen.len(),
            BuggyVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
    }

    /// Every cyclecar the family can draw at the blueprint corners: every
    /// ornateness-by-wear pair (#1376). Returns the built tree, its plan and
    /// a label for the failure.
    fn every_cyclecar() -> Vec<(Generator, cyclecar::CyclecarPlan, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            let plan = cyclecar::plan_of(&bp);
            for o in OrnatenessTier::ALL {
                for w in WearTier::ALL {
                    (ctx.ornateness, ctx.wear) = (o, w);
                    out.push((
                        cyclecar::build_dressed(&ctx, &plan),
                        plan,
                        format!("a {} m cyclecar, {} / {}", bp.length, o.label(), w.label()),
                    ));
                }
            }
        }
        assert_eq!(out.len(), 6 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built cyclecar meets another and the whole machine is
    /// one component, on every tier at every blueprint corner (#1376) - on
    /// the tree AS SAVED, through the record's 0.1 mm wire.
    ///
    /// What the guard cannot see: it ignores `path_cut`, so the spat's box
    /// reaches under the ground and the window band's sectors are judged as
    /// whole tubes, which lie inside the pod (#1382).
    #[test]
    fn a_cyclecar_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_cyclecar() {
            let json = serde_json::to_string(&built).expect("a cyclecar serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every cyclecar survives the record sanitiser UNCHANGED at the extremes
    /// of her own blueprint, on every tier (#1359 rule 8) - her rotated tyres
    /// and lamps compared through the quaternion epsilon the sanitiser's
    /// renormalising needs.
    #[test]
    fn a_cyclecar_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_cyclecar() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every cyclecar stands on her three wheels, under the air draft and
    /// inside the gateway, at every blueprint corner (#1376): each wheel's
    /// centre is its radius over the ground - the single rear one on the
    /// centreline as much as the front pair - the spat's cut plane stands
    /// over the ground, nothing she draws stands higher than rule 6's 2.8 m,
    /// and she is narrower than the 2.6 m mouth.
    #[test]
    fn a_cyclecar_stands_on_her_three_wheels_under_the_air_draft() {
        use super::super::common::touch;
        /// Rule 6's air draft and the narrowest gateway mouth (m).
        const AIR_DRAFT: f32 = 2.8;
        const MOUTH: f32 = 2.6;
        for (built, plan, what) in every_cyclecar() {
            let wheels = plan.wheels();
            assert_eq!(
                wheels.len(),
                3,
                "{what}: she rolls on {} wheels",
                wheels.len()
            );
            assert_eq!(
                wheels.iter().filter(|(at, _)| at[0] == 0.0).count(),
                1,
                "{what}: she has no single wheel on the centreline"
            );
            for (at, r) in wheels {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
            }
            let cut = cyclecar::spat_cut_y(&plan) + plan.datum_height();
            assert!(
                cut > 0.0,
                "{what}: the spat's cut plane is {cut} m over the ground"
            );
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT,
                "{what}: she stands {top} m over the ground, past the {AIR_DRAFT} m air draft"
            );
            let wide = cyclecar::Cyclecar.overall_width(&plan, 0);
            assert!(
                wide < MOUTH,
                "{what}: {wide} m wide, past the {MOUTH} m mouth"
            );
        }
    }

    /// Every cyclecar seed is drawn as a cyclecar, electric (#1376) - and no
    /// other skiff seed is: a skiff is electric exactly when it is drawn as
    /// one. Her neon themes' haze is the one aura she carries, on the record,
    /// over her roof; a Solarpunk or CivicCampus cyclecar's exhaust floor is
    /// dropped, so she carries no emitter at all.
    #[test]
    fn a_cyclecar_seed_draws_a_cyclecar() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::{AvatarCharacter, ThemeArchetype};
        let (mut hazy, mut clear) = (0, 0);
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let cyclecar = SkiffType::for_seed(s) == SkiffType::Cyclecar;
            assert_eq!(
                propulsion(s) == Propulsion::Electric,
                cyclecar,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !cyclecar {
                continue;
            }
            let (record, _) = super::super::build_for_seed(s);
            let emitters: Vec<_> = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .filter(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .collect();
            match AvatarCharacter::for_seed(s).style {
                ThemeArchetype::Cyberpunk | ThemeArchetype::AlienMonolithic => {
                    assert_eq!(emitters.len(), 1, "seed {s}: no neon haze");
                    assert_eq!(
                        Some(emitters[0].transform.translation.0),
                        fx_mount(s, ParticleAura::NeonHaze),
                        "seed {s}: her haze is not over her roof"
                    );
                    hazy += 1;
                }
                ThemeArchetype::Solarpunk | ThemeArchetype::CivicCampus => {
                    assert!(
                        emitters.is_empty(),
                        "seed {s}: an electric pod trails an aura"
                    );
                    clear += 1;
                }
                style => panic!("seed {s}: a cyclecar on {style:?}"),
            }
        }
        assert!(
            hazy > 10 && clear > 10,
            "{hazy} hazy cyclecars, {clear} clear"
        );
    }

    /// Every armoured car the family can draw at the blueprint corners: every
    /// variant crossed with every ornateness-by-wear pair (#1375). Returns
    /// the built tree, its plan and a label for the failure.
    fn every_armoured() -> Vec<(Generator, armoured::ArmouredPlan, String)> {
        use crate::seeded_defaults::{ArmouredVariant, OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for variant in ArmouredVariant::ALL {
                let plan = armoured::plan_of(&bp, variant);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            armoured::build_dressed(&ctx, &plan),
                            plan,
                            format!(
                                "a {} m {}, {} / {}",
                                bp.length,
                                variant.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        assert_eq!(out.len(), 6 * 2 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built armoured car meets another and the whole machine
    /// is one component, on every variant and tier at every blueprint corner
    /// (#1375) - on the tree AS SAVED, through the record's 0.1 mm wire.
    ///
    /// Her contact is by CONSTRUCTION rather than by fit, and it has to be:
    /// the guard reads a Wedge and a Bevel as their BOX, so her `taper`,
    /// `taper_bottom` and chamfers are all invisible to it and it is generous
    /// about every plate she carries. Each mount is bedded against the DRAWN
    /// flank at its own height ([`armoured::ArmouredPlan::flank_x`]), which
    /// lies inside that box; and the spare, the stern rails and the tail
    /// lamps reach inside the stern PLANE, which a swept form's ball would
    /// have met on its own.
    #[test]
    fn an_armoured_car_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_armoured() {
            let json = serde_json::to_string(&built).expect("an armoured car serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every armoured car survives the record sanitiser UNCHANGED at the
    /// extremes of her own blueprint, on every variant and tier (#1359 rule
    /// 8).
    ///
    /// She carries no node scale at all - the first type that does not - so
    /// what is at risk here is her TAPERS and her chamfers, which the
    /// sanitiser clamps, and the rotations she authors: her slits, her flash,
    /// her bins, her cans and an Ornate machine's thrown-open hatch, compared
    /// through the quaternion epsilon the sanitiser's renormalising needs.
    #[test]
    fn an_armoured_car_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_armoured() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every armoured car stands on her four wheels with her hull clear of the
    /// ground, under the air draft and inside the gateway, at every blueprint
    /// corner (#1375).
    ///
    /// The ground clearance is the point of it: her section is DERIVED from
    /// the roof she stands under and the floor she stands over, and with a
    /// constant section instead that clearance swings 3.4x across these six
    /// corners. So this reads the DRAWN tree's lowest plate rather than the
    /// arithmetic that held it - the tyres excepted, which are the only thing
    /// of hers that touches the ground.
    #[test]
    fn an_armoured_car_stands_clear_of_the_ground_under_the_air_draft() {
        use super::super::common::touch;
        /// Rule 6's air draft and the narrowest gateway mouth (m).
        const AIR_DRAFT: f32 = 2.8;
        const MOUTH: f32 = 2.6;
        for (built, plan, what) in every_armoured() {
            let wheels = plan.wheels();
            assert_eq!(
                wheels.len(),
                4,
                "{what}: she rolls on {} wheels",
                wheels.len()
            );
            for (at, r) in wheels {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
            }
            let floor = plan.sill_at(0.0) + plan.datum_height();
            assert!(
                floor > 0.10,
                "{what}: the hull's floor is {floor} m over the ground"
            );
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT,
                "{what}: she stands {top} m over the ground, past the {AIR_DRAFT} m air draft"
            );
            let wide = armoured::Armoured.overall_width(&plan, 0);
            assert!(
                wide < MOUTH,
                "{what}: {wide} m wide, past the {MOUTH} m mouth"
            );
        }
    }

    /// Every armoured-car seed is drawn as an armoured car, on the variant her
    /// theme picks, a diesel (#1375) - and no other skiff seed is: a skiff is
    /// a diesel exactly when she is drawn as one. And the aura her record
    /// carries - her theme's steam, embers or exhaust, none of them folded -
    /// leaves her own drawn pipe's mouth.
    #[test]
    fn an_armoured_car_seed_draws_an_armoured_car() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::ArmouredVariant;
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let car = SkiffType::for_seed(s) == SkiffType::ArmouredCar;
            assert_eq!(
                propulsion(s) == Propulsion::Diesel,
                car,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !car {
                continue;
            }
            let v = ArmouredVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
            let (_, plan) = body_for(s).expect("an armoured-car seed has a body");
            let (record, _) = super::super::build_for_seed(s);
            let emitter = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .find(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .expect("an armoured car trails an aura");
            assert_eq!(
                emitter.transform.translation.0,
                armoured::pipe_mouth(&armoured::with_variant(plan, v)),
                "seed {s}: her aura does not leave her pipe's mouth"
            );
        }
        assert_eq!(
            seen.len(),
            ArmouredVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
    }

    /// Every part of a built roadster meets another, and the whole machine is
    /// one connected component (#1364, the owner's complaint on the
    /// prototype) - on every body, top, wheel and tier it can roll (#1367).
    ///
    /// Swept over the blueprint EXTREMES rather than one seed, because the
    /// failure mode is size-dependent: a bead or a track rod floored at
    /// [`MIN_DIM`] stops shrinking with the body, so a part that touches at
    /// the nominal size can come adrift at the small end - or push through at
    /// the large one. See [`super::super::common::touch`] for why this cannot
    /// be judged by eye: the chase camera looks down, so nothing under a craft
    /// is ever in frame at play distance.
    ///
    /// Checked on the tree AS SAVED - through the record's own 0.1 mm wire -
    /// rather than on the f32 tree in memory, because a part authored exactly
    /// flush with what it stands on touches in f32 and not after the rounding:
    /// the radiator's filler cap did exactly that on seed 134 (#1367 defect 1).
    ///
    /// What it cannot see, and the reason a binary test sits beside it: the
    /// guard models a sweep as a chain of capsules, which bulge past a blunt
    /// end, so a wheel standing clear of a bobtail's back still reads as
    /// touching - see the roadster's tail-mount test.
    #[test]
    fn a_roadster_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, what) in every_roadster() {
            let json = serde_json::to_string(&built).expect("a roadster serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every roadster survives the record sanitiser UNCHANGED at the extremes
    /// of its own blueprint, on every body, top, wheel and tier, not only at
    /// the seeds the population happens to contain (#1359 rule 8).
    #[test]
    fn a_skiff_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        let mut n = 0;
        for (built, what) in every_roadster() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            n += 1;
        }
        assert_eq!(n, 6 * 3 * 3 * 2 * 9, "the sweep lost a combination");
    }

    /// Building the same seed twice gives the same tree, bit for bit.
    #[test]
    fn a_seeded_skiff_builds_deterministically() {
        for s in (0u64..200).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            assert_eq!(
                build(s, None),
                build(s, None),
                "seed {s} is not deterministic"
            );
        }
    }

    /// A seeded skiff's saved record stays well under the soft budget
    /// (#1359 rule 9), at a THIRD of it.
    ///
    /// Two sweeps, in the sloop's form (#1366). The live seeds, as saved - FX
    /// emitter and engine voice included - and the heaviest thing the family
    /// can draw: every roadster body, top and wheel, every wagon body, every
    /// buggy variant, every armoured-car variant and the cyclecar at every
    /// blueprint corner on the fullest ladder,
    /// Ornate and Battered, carrying the heaviest FX overhead
    /// any live seed carries. Measured rather than assumed, so a new aura that
    /// grows the emitter moves this too. The phase-1 prototype put the worst
    /// of it - an open tourer on wire wheels at 3.6 m - at nine per cent under
    /// the guard (#1367), and it is the wire wheels that spend the margin.
    #[test]
    fn a_seeded_skiffs_record_stays_well_inside_the_budget() {
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        use crate::seeded_defaults::{
            OrnatenessTier, RoadsterBody, RoadsterTop, RoadsterWheels, WearTier,
        };
        let bytes = |t: &Generator| serialized_record_bytes(t).expect("a skiff serializes");
        let (mut worst_seed, mut fx_overhead) = (0usize, 0usize);
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let (record, _) = super::super::build_for_seed(s);
            let saved = serialized_record_bytes(&record).expect("a record serializes");
            worst_seed = worst_seed.max(saved);
            fx_overhead = fx_overhead.max(saved.saturating_sub(bytes(&build(s, None))));
        }
        assert!(worst_seed > 0 && fx_overhead > 0, "nothing was measured");
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        ctx.ornateness = OrnatenessTier::Ornate;
        ctx.wear = WearTier::Battered;
        let mut worst_corner = 0usize;
        for bp in corners() {
            for body in RoadsterBody::ALL {
                for rolls in RoadsterWheels::ALL {
                    let plan = roadster::plan_of(&bp, body, rolls);
                    for top in RoadsterTop::ALL {
                        let mut built = roadster::build_dressed(&ctx, &plan, top, rolls);
                        apply_travel_pose(&mut built, travel_drop(&roadster::Roadster, &plan, 0));
                        worst_corner = worst_corner.max(bytes(&built) + fx_overhead);
                    }
                }
            }
        }
        // And the wagon's, every body at every corner, on its fullest ladder
        // (#1377): the heaviest is a 3.6 m Ornate / Battered cart at about
        // 27 KB, twelve spokes a wheel spending most of it.
        let mut worst_wagon = 0usize;
        for bp in corners() {
            for body in crate::seeded_defaults::WagonBody::ALL {
                let plan = wagon::plan_of(&bp, body);
                let mut built = wagon::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&wagon::Wagon, &plan, 0));
                worst_wagon = worst_wagon.max(bytes(&built) + fx_overhead);
            }
        }
        // And the dune buggy's, every variant at every corner on her fullest
        // ladder (#1374): the heaviest is a 3.6 m Ornate / Battered beach
        // buggy at about 28 KB, her canopy's six stripes and her frame's
        // members spending most of it.
        let mut worst_buggy = 0usize;
        for bp in corners() {
            for variant in crate::seeded_defaults::BuggyVariant::ALL {
                let plan = buggy::plan_of(&bp, variant);
                let mut built = buggy::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&buggy::Buggy, &plan, 0));
                worst_buggy = worst_buggy.max(bytes(&built) + fx_overhead);
            }
        }
        // And the cyclecar's, at every corner on her fullest ladder and in
        // the heavier of her one-colour and split pods (#1376): the lightest
        // skiff, about 14 KB, one pod sweep most of her.
        let mut worst_cyclecar = 0usize;
        for bp in corners() {
            let plan = cyclecar::plan_of(&bp);
            let split = crate::pds::avatar::livery::CYCLECAR_LIVERIES
                .iter()
                .position(|l| l.name == "Pearl over obsidian")
                .expect("her two-tone is in her list");
            for livery in [0, split] {
                let mut ctx = ctx;
                ctx.livery = Some(livery);
                let mut built = cyclecar::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&cyclecar::Cyclecar, &plan, 0));
                worst_cyclecar = worst_cyclecar.max(bytes(&built) + fx_overhead);
            }
        }
        // And the armoured car's, every variant at every corner on her
        // fullest ladder (#1375): the heaviest is a 3.6 m Ornate / Battered
        // raider at about 18 KB, her plates and her four wheels most of it.
        // She spends the least of any type on thin tubes, which is what keeps
        // her the second lightest.
        let mut worst_armoured = 0usize;
        for bp in corners() {
            for variant in crate::seeded_defaults::ArmouredVariant::ALL {
                let plan = armoured::plan_of(&bp, variant);
                let mut built = armoured::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&armoured::Armoured, &plan, 0));
                worst_armoured = worst_armoured.max(bytes(&built) + fx_overhead);
            }
        }
        for (what, worst) in [
            ("seeded skiff", worst_seed),
            ("fully dressed corner", worst_corner),
            ("fully dressed wagon", worst_wagon),
            ("fully dressed buggy", worst_buggy),
            ("fully dressed cyclecar", worst_cyclecar),
            ("fully dressed armoured car", worst_armoured),
        ] {
            assert!(
                worst * 3 < SOFT_RECORD_BUDGET_BYTES,
                "the heaviest {what} is {worst} bytes, past a third of the \
                 {SOFT_RECORD_BUDGET_BYTES}-byte soft budget - a craft type is \
                 spending nodes where it should be spending shape"
            );
        }
    }

    /// No seeded skiff is wider than the narrowest gateway mouth it has to
    /// drive through.
    ///
    /// The skiff's counterpart of the boat's air-draft cap, and the reason the
    /// brief asked for it: visuals carry no colliders, and wheels and guards
    /// stand a long way outboard of coachwork this narrow, so the width is not
    /// the blueprint's `body_w` but the guards' own. Measured rather than
    /// assumed - the legacy fleet reached 2.39 m against this same 2.6 m.
    #[test]
    fn no_seeded_skiff_is_wider_than_the_narrowest_gateway_mouth() {
        /// The narrowest mouth on a seeded gateway (m) - see #1359 rule 6.
        const MOUTH: f32 = 2.6;
        let mut worst: f32 = 0.0;
        let mut checked = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let (craft, plan) = body_for(s).expect("a skiff seed has a body");
            let w = craft.overall_width(&plan, s);
            assert!(
                w < MOUTH,
                "seed {s} is {w} m wide, past the {MOUTH} m mouth"
            );
            worst = worst.max(w);
            checked += 1;
        }
        assert!(checked > 100, "too few skiffs sampled: {checked}");
    }

    /// The datum, the ground and the axle line are one derivation - on every
    /// body and every wheel, balloons included: a balloon tyre is the PLAN's
    /// wheel radius, so the axle rises with it and the tyre still stands on
    /// the ground (#1367).
    #[test]
    fn the_axle_line_is_one_wheel_radius_over_the_ground() {
        use crate::seeded_defaults::{RoadsterBody, RoadsterWheels};
        for bp in corners() {
            for body in RoadsterBody::ALL {
                for rolls in RoadsterWheels::ALL {
                    let plan = roadster::plan_of(&bp, body, rolls);
                    assert!(
                        (plan.axle_y() + plan.datum_height() - plan.wheel_r).abs() < 1e-5,
                        "a {} m {body:?} on {rolls:?}: the axle line is not its wheel \
                         radius over the ground",
                        bp.length
                    );
                    // And the body really does stand where the beltline says.
                    assert!((plan.datum_height() + plan.depth() - plan.beltline).abs() < 1e-5);
                }
            }
        }
    }
}
