//! The six-wheeled rover: a thin equipment deck riding high on rocker-bogie
//! arms, with an instrument mast and a lit camera head. The machine of the
//! outpost and alien themes - SpaceOutpost, AlienOrganic and AlienMonolithic
//! (#1378).
//!
//! Three variants by theme ([`RoverVariant`]): the SpaceOutpost **surveyor**,
//! who carries a tilted solar panel aft and a dish on her far side; the
//! AlienOrganic **carapace**, a chitin shell over a dark running deck; and
//! the AlienMonolithic **monolith**, an upright lit slab with an obelisk in
//! place of the mast. One machine drawn on all three themes was rendered
//! beside them and rejected: it is the same rover in a different paint.
//!
//! [`deck`] draws the plate and the band round it; [`running_gear`] the
//! rocker-bogie and the six wheels; [`instruments`] the mast, the camera
//! head and the variant's own mass; and [`dressing`] the ladder of secondary
//! masses by ornateness and wear.
//!
//! # Six wheels are her identity, and their spacing is half of it
//!
//! Three paired [`Axle`] rows at `1.0 / 0.0 / -1.0`. Evenly spaced she is a
//! rover at a glance; on a truck's tandem rear (`1.0 / -0.30 / -1.0`) the
//! after pair bunch into one blob from the chase quarter and she reads as a
//! lorry, and on four she is a flat trailer. The wheels are VISUAL only -
//! `player/car.rs` is a four-corner raycast chassis off
//! [`chassis_half_extents`](super::chassis_half_extents) and never reads a
//! drawn wheel - so the middle pair stand on the ground by the plan's own
//! arithmetic and nothing in locomotion changes.
//!
//! # The rocker-bogie is load-bearing, not decoration
//!
//! Her wheelbase is 1.12 x the blueprint's over a deck 0.70 of the length
//! long, so her front and rear axles lie BEYOND the deck's own ends: the
//! deck's nose stands at 0.93 to 0.97 of the front axle's station on every
//! seed and every corner. A straight stub axle from the deck's flank to a
//! hub would therefore start in open air, and the prototype drawn that way
//! came apart into three components. The rocker reaches out there from a
//! pivot UNDER the deck, which is exactly what the real linkage is for; the
//! DIFFERENTIAL BAR athwart both rocker pivots is what hangs the running
//! gear on the machine, and at the narrow-track corner - where the arm plane
//! is buried 3 % inside the deck's flank - it is the only thing that does.
//!
//! # The plan idiom, for a plate
//!
//! The family's [`BodyPlan`] fits her with no new field: six wheels are
//! three paired [`Axle`] rows, which [`BodyPlan::wheels`] and
//! [`BodyPlan::axle_lines`] already emit. Her plan form is a box's - ONE
//! half-width from tail to nose, because a deck is a box and every mount
//! then reads the number the plate is cut to.
//!
//! **The section is DERIVED, not typed** (the armoured car's idiom): the
//! deck's top stands at the blueprint's beltline and its floor at
//! [`BELLY`] of the plan's wheel radius, just clear of the tyres' crowns,
//! and the section that puts them both there falls out of the two. That one
//! arithmetic holds her deck thickness and her ground clearance at every
//! blueprint corner.
//!
//! The DATUM is mid-deck, so [`BodyPlan::depth`] is HALF THE DECK'S
//! THICKNESS - the smallest in the fleet - and [`BodyPlan::datum_height`] is
//! large.
//!
//! # Her travel drop is not signed, and she is the first that is not
//!
//! Every type before her hung its datum below the chassis origin. A thin
//! deck riding high makes `depth()` small and `datum_height()` large, so
//! [`travel_drop`](super::travel_drop) comes out NEGATIVE over the survey
//! (-0.035 to -0.132 m) and CROSSES ZERO inside her own blueprint range:
//! -0.339 m at the 3.60 m corner and +0.094 m at the 1.90 m one. Nothing in
//! the family asserts a sign, `apply_travel_pose` simply negates it, and her
//! tyres are on the ground at every corner either way - which is what the
//! derivation was for. #1382 read this and did NOT assert a sign; anything
//! that revisits her drive has to read this paragraph first.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1378, #1359 rules 1, 12 and 14):
//! `target/dump/vehicles2026-09/rover/rover.py` is its python twin, and the
//! port was checked node for node against it.

mod deck;
mod dressing;
mod instruments;
mod running_gear;

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::livery::rover_colours;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{ParticleAura, RoverVariant, SkiffBlueprint};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim, quat_x, quat_z, superellipsoid};
use super::plan::{Axle, BodyPlan};
// The family's shape vocabulary, imported here so each part's
// `use super::{plate, ..}` resolves.
use super::shape::{line, plate, rim_profile, solid, tapered_plate, tyre_profile};
use super::{SkiffCraft, SkiffFeel, SkiffIdle, dim};

/// Her deck's half-width over the blueprint's body half-width: the deck
/// overhangs the coachwork a real car of this blueprint would carry, because
/// the deck IS the machine's whole plan form.
const DECK_W: f32 = 1.15;

/// The deck's floor over the PLAN's wheel radius - the second of the two
/// numbers the section is derived from (see the module docs). The first is
/// the blueprint's beltline, which the deck's top stands at unchanged. 2.30
/// puts the floor just over the tyres' crowns at every corner.
const BELLY: f32 = 2.30;

/// The wheels' radius, her track and her wheelbase over the blueprint's.
///
/// 0.78 rather than the blueprint's own is the proportion that makes her
/// read as six-wheeled: at 0.92 she has a buggy's wheels and the six-ness is
/// lost in the overlap, at 0.66 she is under-tyred. The wider track and the
/// longer wheelbase spread the six along and across her so the middle pair
/// are not swallowed.
const WHEEL: f32 = 0.78;
const TRACK: f32 = 1.20;
const WHEELBASE: f32 = 1.12;

/// A tyre's half-width over its radius: 0.50 is a DRUM rather than a tyre,
/// which is what a rover rolls on.
const TYRE_W: f32 = 0.50;

/// How far the rim stands proud of the tyre's end caps, over the tyre's
/// half-width - what she is widest at.
const RIM_REACH: f32 = 1.10;

/// A rocker's tube radius (of the length). The bogie is drawn at 0.92 of it
/// and every stub axle at 0.95, so the arms read as a linkage rather than as
/// one welded frame.
const ARM_R: f32 = 0.0150;

/// The camera head's top over the ground (of the length), or what rule 6's
/// cap leaves, whichever is lower.
const MAST: f32 = 0.62;

/// Rule 6's air draft and the margin held under it (m) - see [`cap_y`].
///
/// [`cap_y`]: RoverPlan::cap_y
const AIR_CAP: f32 = 2.80;
const AIR_MARGIN: f32 = 0.06;

/// The deck's plan form, `(z fraction of the length, half-width fraction)`,
/// tail to nose.
///
/// Parallel-sided, because a deck is a box: one half-width over the whole
/// run, so every mount reads the same number the plate is cut to. The deck
/// stops well short of the machine's own length at both ends - the six
/// wheels and their arms fill the rest of it.
const PLAN_BOX: &[(f32, f32)] = &[(-0.360, 1.00), (0.340, 1.00)];

/// SIX wheels on three paired axles, evenly spaced. See the module docs for
/// why the spacing is not free.
const AXLES: &[Axle] = &[
    Axle {
        at: 1.0,
        paired: true,
        radius: 1.0,
    },
    Axle {
        at: 0.0,
        paired: true,
        radius: 1.0,
    },
    Axle {
        at: -1.0,
        paired: true,
        radius: 1.0,
    },
];

/// The side the chase camera's usual quarter shows: her mast, her stern
/// pallet's taller box and a worn machine's odd rim are on it, and her dish
/// and her whip on the far one.
const NEAR: f32 = 1.0;

/// No rotation at all - the authoring frame's own.
const NO_TURN: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Which side of the centreline `x` is on.
fn side(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// A turned part laid with its axis outboard along x, on either side.
fn outboard(x: f32) -> [f32; 4] {
    quat_z(-side(x) * FRAC_PI_2)
}

/// One axle of the rover: its station, its wheels' centre over the datum,
/// their radius and their tyres' half-width (m).
#[derive(Clone, Copy, Debug)]
struct AxleLine {
    z: f32,
    y: f32,
    r: f32,
    w: f32,
}

/// The rover's plan: the family's [`BodyPlan`] over her own blueprint copy,
/// and the variant drawn on it. It derefs to the plan, so every shared read
/// looks the same as on a car, and it adds the reads her deck, her running
/// gear and her instruments share.
#[derive(Clone, Copy, Debug)]
pub(super) struct RoverPlan {
    plan: BodyPlan,
    variant: RoverVariant,
}

impl std::ops::Deref for RoverPlan {
    type Target = BodyPlan;
    fn deref(&self) -> &BodyPlan {
        &self.plan
    }
}

impl RoverPlan {
    /// A station along the machine (m), from a fraction of the length.
    fn at(&self, fraction: f32) -> f32 {
        fraction * self.length
    }

    /// The deck's TOP (m over the datum).
    ///
    /// The datum is mid-deck, so it is simply `depth()`, and everything that
    /// stands on her is bedded against it. Her FLOOR is `-depth()`, which is
    /// [`BodyPlan::sill_at`] at any station, because a deck is
    /// parallel-sided; the clearance guard reads it there.
    fn top(&self) -> f32 {
        self.depth()
    }

    /// The deck's `(tail z, nose z, centre z, length)` (m).
    ///
    /// Named for the twin's own `run`, and it deliberately hides
    /// [`BodyPlan::run`]: she sweeps no run at all, because a deck is a
    /// plate.
    fn deck_run(&self) -> (f32, f32, f32, f32) {
        let (z0, z1) = (self.tail_z(), self.nose_z());
        (z0, z1, (z0 + z1) * 0.5, z1 - z0)
    }

    /// Rule 6 in the datum's frame: nothing she draws stands over this, BY
    /// CONSTRUCTION. The camera head, the dish and the antenna whip's beacon
    /// are each clamped to it rather than checked against it.
    fn cap_y(&self) -> f32 {
        AIR_CAP - AIR_MARGIN - self.datum_height()
    }

    /// The axle at station `at` in half-wheelbases (`+1.0` front, `0.0`
    /// middle, `-1.0` rear).
    fn axle(&self, at: f32) -> AxleLine {
        let (z, r) = (at * self.wheelbase * 0.5, self.wheel_r);
        AxleLine {
            z,
            y: r - self.datum_height(),
            r,
            w: r * TYRE_W,
        }
    }

    /// The plane the rockers and the bogies lie in (m from the centreline):
    /// between the deck's flank and the tyres' inboard faces, held off the
    /// tyre by the arm's own diameter.
    ///
    /// Over all 54 seeds and the six corners it stands at 0.97 to 1.26 of
    /// the deck's half-width. At the narrow-track corner the rocker's pivot
    /// is buried 3 % inside the deck's flank; everywhere else it stands
    /// OUTBOARD of it, and the differential bar drawn through both pivots
    /// and the deck is what bridges the gap - which is why that bar is not
    /// ornament.
    fn arm_x(&self) -> f32 {
        let inner = self.track * 0.5 - self.axle(1.0).w;
        let x = inner - ARM_R * self.length * 2.2;
        debug_assert!(
            x > self.half_w() * 0.5,
            "the arm plane fell deep inside the deck"
        );
        x
    }

    /// The six tyres' outboard faces with the rim standing proud of them (m)
    /// - what the gateway mouth is measured against.
    fn overall_width(&self) -> f32 {
        self.track + 2.0 * self.axle(1.0).w * RIM_REACH
    }
}

/// The rover's plan for a blueprint, as `variant`: her deck half-width, her
/// wheels, her track and her wheelbase over the blueprint's, with the
/// section derived from the top and the floor her deck is to lie between.
pub(super) fn plan_of(bp: &SkiffBlueprint, variant: RoverVariant) -> RoverPlan {
    let wheel_r = bp.wheel_r * WHEEL;
    // The deck's top IS the blueprint's beltline, and the datum is mid-deck,
    // so the beltline the copy carries is the top.
    let top = bp.beltline;
    let floor = wheel_r * BELLY;
    let half_w = bp.body_w * DECK_W * 0.5;
    // The one derivation, and a NEGATIVE here is a bug rather than a thin
    // deck: `dim` would quietly floor it and draw her inside out. It comes
    // out at 0.088 and 0.091 of the length at the two corner proportion
    // sets, so the margin is real.
    debug_assert!(
        top - floor > 0.0,
        "the deck's thickness is {} m - negative or zero before the floor",
        top - floor
    );
    let bp = SkiffBlueprint {
        body_w: bp.body_w * DECK_W,
        wheel_r,
        track: bp.track * TRACK,
        wheelbase: bp.wheelbase * WHEELBASE,
        beltline: top,
        ..*bp
    };
    with_variant(
        BodyPlan::new(&bp, dim(top - floor) * 0.5 / half_w, PLAN_BOX, AXLES),
        variant,
    )
}

/// A plan the family built, with its variant laid back on it - the
/// [`SkiffCraft`] seam carries only the shared [`BodyPlan`].
pub(super) fn with_variant(plan: BodyPlan, variant: RoverVariant) -> RoverPlan {
    RoverPlan { plan, variant }
}

/// Where a seeded aura issues from (root-local, m) - see
/// [`Rover::fx_mount`].
pub(super) fn aura_mount(aura: ParticleAura, plan: &RoverPlan) -> [f32; 3] {
    instruments::aura_mount(aura, plan)
}

pub(super) struct Rover;

impl SkiffCraft for Rover {
    fn plan(&self, bp: &SkiffBlueprint, seed: u64) -> BodyPlan {
        plan_of(bp, RoverVariant::for_seed(seed)).plan
    }

    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator {
        build_dressed(ctx, &with_variant(*plan, RoverVariant::for_seed(ctx.seed)))
    }

    fn feel(&self) -> SkiffFeel {
        // #1381's sweep, agreed by the owner on 2026-09-20, and the most
        // opinionated row on the card. A servo rover drives each wheel:
        // slow, deliberate, and it PIVOTS - nothing about it should turn
        // like a lorry, and at 21.0 deg/s round 40.8 m she had both the
        // slowest turn and the widest circle in the whole fleet.
        //
        // The placeholder had her the SLOWEST-TURNING skiff, which was the
        // reading that most needed driving: turn_accel 1.2 -> 2.4 with
        // angular_damping 4.0 -> 3.0 gives 55.8 deg/s, and drive_accel
        // 6.0 -> 2.5 with linear_damping 0.8 -> 0.9 gives 10.0 km/h
        // (2.78 m/s). Together that is a circle of 5.5 m = 2.0 of her own
        // 2.72 m: a pivot, and the tightest thing on land or water. If she
        // should merely be tidy rather than a tank, turn_accel 1.8 gives
        // about four lengths.
        //
        // The mass factor is untouched, and it is not a feel knob (see
        // `SkiffFeel::mass_factor`). She is still the first type to reach
        // the family's 480 kg mass FLOOR, as the armoured car was the first
        // to reach the 1500 kg ceiling. Nine of the 54 seeds under 3000
        // clamp at it - every seed under 2.437 m - so a factor much under
        // this stops moving the short third of her population at all. 0.62
        // (the buggy's) is the smallest that clears the floor at her
        // shortest seed.
        SkiffFeel {
            mass_factor: 0.58,
            drive_accel: 2.5,
            turn_accel: 2.4,
            linear_damping: 0.9,
            angular_damping: 3.0,
        }
    }

    fn idle(&self) -> SkiffIdle {
        // `Propulsion::Servo`: each wheel is driven by its own motor and
        // there is no engine to idle, so she sits still. Whether a servo
        // rover should TICK rather than be silent is a taste call the owner
        // can take later; stillness is the honest floor.
        //
        // 3 degrees, the flattest lean in the fleet: the flattest box
        // measured, a width-to-height ratio of 7.77 against the roadster's
        // 2.32, on six wheels. She leans IN, barely.
        SkiffIdle {
            shiver: None,
            bank_degrees: 3.0,
        }
    }

    fn overall_width(&self, plan: &BodyPlan, seed: u64) -> f32 {
        let plan = with_variant(*plan, RoverVariant::for_seed(seed));
        plan.overall_width()
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan, seed: u64) -> [f32; 3] {
        // The seed's own flourish hovers over the deck, clear of the mast
        // and the dish and over whatever the variant stands on it. An
        // exhaust or a steam wisp never reaches this on a live seed - her
        // servo drops both in `fx::drawn_aura`, which is what empties the 25
        // SpaceOutpost seeds of any emitter at all - but the mount is total,
        // and where it is kept it leaves the ground between the rear tyres.
        aura_mount(aura, &with_variant(*plan, RoverVariant::for_seed(seed)))
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Servo
    }
}

/// The rover as `plan`'s variant, dressed for the tiers `ctx` carries: what
/// [`Rover::build`] draws with the seed's own variant, and what the guards
/// sweep every variant through.
///
/// The order is the agreed prototype's, part for part, which is what lets a
/// dump of this tree be checked node by node against it.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &RoverPlan) -> Generator {
    let c = rover_colours(ctx);
    // Root: a hidden hub on the datum, buried in the deck - the roadster's
    // idiom.
    let hub = dim(plan.length * 0.008);
    let mut root = prim(cuboid([hub; 3], c.arm.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    let ladder = dressing::ladder(plan, ctx.ornateness, ctx.wear);
    deck::plate_and_band(kids, plan, &c);
    running_gear::rocker_bogie(kids, plan, &c);
    running_gear::wheels(kids, plan, &c, ladder.odd_rim);
    instruments::mast(kids, plan, &c);
    instruments::variant_mass(kids, plan, &c, ladder.dead_cell);
    dressing::dress(kids, plan, &c, &ladder);
    root
}
