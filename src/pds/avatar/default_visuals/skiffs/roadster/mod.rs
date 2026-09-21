//! The roadster: an open two-seat sports car on a boat-tailed body, and the
//! skiff family's universal floor.
//!
//! Every dimension here is a fraction of the seeded machine, read off
//! [`BodyPlan`], so a 1.9 m seed and a 3.6 m one are the same car at two sizes
//! rather than two different mistakes. The one exception is the one that
//! *cannot* scale: the sanitiser's dimension floor ([`super::MIN_DIM`]).
//!
//! The shape was agreed by render before any of this was written (#1359 rules
//! 1, 12 and 14). #1364 agreed the hero - `target/dump/vehicles2026-09/
//! roadster2.py` - and #1367 agreed what finishes it, prototyped over a python
//! twin of this module verified node for node against the live build on seven
//! seeds (`roadster2/roadster3.py`, `variants.py`):
//!
//! - three bodies, each nothing but a plan station list and a [`Layout`],
//!   picked per seed by [`RoadsterBody`];
//! - [`coachwork`]: the three swept runs of one plan and the trim along them,
//!   the radiator, the wings, the lamps, the bumper and the exhaust;
//! - [`running_gear`]: the axles, the track rod and the torque tube;
//! - [`wheels`]: pressed discs, balloon tyres on every Heavy machine, or wire
//!   wheels, behind the [`Wheels`](wheels::Wheels) trait, picked by
//!   [`RoadsterWheels`];
//! - [`cockpit`] for an open car and [`hardtop`] for a closed one, picked by
//!   [`RoadsterTop`];
//! - [`dressing`]: the ladder of secondary masses by ornateness and wear.
//!
//! # Why it is built the way it is
//!
//! Three swept runs of ONE plan share one section scale, so their sections
//! meet flush: a full barrel for the bonnet, a **bored lower half-pipe** for
//! the tub - whose cut rim is the cockpit coaming and whose bore is the
//! footwell - and upper half-pipes for the scuttle and the tail deck, which
//! stop at the cockpit's ends so the opening between them is simply the gap.
//! "Land-skiff" is the right word for it: boat-built torpedo bodies were a
//! real coachbuilding style, and this is one.
//!
//! Nothing here is a BlobGroup (owner decision 4): a blob reads as organic and
//! this is a machine. The hardtop is the one Superellipsoid, which the
//! redesign's rules allow for a pressed panel.

mod coachwork;
mod cockpit;
mod dressing;
mod hardtop;
mod running_gear;
mod wheels;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::seeded_defaults::{
    ParticleAura, RoadsterBody, RoadsterTop, RoadsterWheels, SkiffBlueprint,
};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, lathe, prim, quat_xyzw, spine};
use super::plan::{Axle, BodyPlan};
// The family's scaled sweep (#1374), imported here so the coachwork's and
// the hardtop's `use super::{.., sweep}` still resolve.
use super::shape::sweep;
use super::{Shiver, SkiffCraft, SkiffFeel, SkiffIdle, dim, skiff_colours};

/// Section depth per unit half-width - the ONE node scale every body sweep
/// shares, and the knob that turns a plan form below into a body. 0.80 makes
/// a section wider than it is deep, which is what a car's is.
const SECTION: f32 = 0.80;

/// How deep the tub is bored.
///
/// `hollow` leaves a wall of `(1 - hollow)` of the radius, so this is the
/// cockpit floor as a fraction of the section: 0.74 gives a coaming rim a hand
/// wide and a footwell a hand deep. The feasibility probe's 0.14 left the wall
/// 0.86 of the radius - a solid tub with a drain hole through it, and a seat
/// standing on a lid rather than sitting in a car (#1364).
const TUB_HOLLOW: f32 = 0.74;

/// The wings' node scale on x. A flattened tube is what makes a mudguard; 3.4
/// is the brief's, and with no root scale on a craft authored in true metres
/// the scale PRODUCT down this path is 3.4 against the sanitiser's 4.0.
const WING_SCALE_X: f32 = 3.4;
/// Guard tube radius, as a fraction of the length - so the guard is
/// `WING_SCALE_X` times this across and twice it thick.
const WING_R: f32 = 0.0113;
/// The guard's arc radius over the tyre's.
const WING_ARC: f32 = 1.16;

/// Two axles, both paired - four wheels.
const AXLES: &[Axle] = &[
    Axle {
        at: 1.0,
        paired: true,
        radius: 1.0,
    },
    Axle {
        at: -1.0,
        paired: true,
        radius: 1.0,
    },
];

/// The BOAT-TAIL: `(z fraction of the overall length, half-width fraction)`
/// from tail to nose. Nine stations, well inside the sanitiser's sixteen.
/// Maximum beam just abaft the cockpit, a tail running out to a point, and a
/// bonnet that narrows steadily to the radiator. The body the roadster was
/// agreed on (#1364).
const BOAT_TAIL: &[(f32, f32)] = &[
    (-0.500, 0.09),
    (-0.452, 0.34),
    (-0.352, 0.70),
    (-0.230, 0.94),
    (-0.074, 1.00),
    (0.082, 0.97),
    (0.204, 0.91),
    (0.333, 0.80),
    (0.422, 0.70),
];

/// The BOBTAIL: the tail cut off just behind the rear wheels and rounded over
/// in a twentieth of the length - the stubby racer's back.
///
/// It still ENDS on a small radius, because the tub is bored: a sweep ending
/// wide would show the cockpit's bore as a hole in the tail. Everything
/// forward of the cockpit is the boat-tail's.
const BOBTAIL: &[(f32, f32)] = &[
    (-0.440, 0.10),
    (-0.428, 0.52),
    (-0.395, 0.80),
    (-0.330, 0.93),
    (-0.230, 0.98),
    (-0.074, 1.00),
    (0.082, 0.97),
    (0.204, 0.91),
    (0.333, 0.80),
    (0.422, 0.70),
];

/// The TOURER: full beam carried aft over a second row of seats to a blunt
/// rounded back.
const TOURER: &[(f32, f32)] = &[
    (-0.500, 0.10),
    (-0.490, 0.56),
    (-0.466, 0.84),
    (-0.410, 0.96),
    (-0.300, 1.00),
    (-0.074, 1.00),
    (0.082, 0.97),
    (0.204, 0.91),
    (0.333, 0.80),
    (0.422, 0.70),
];

/// What a body offers its tail's one mount: the spare wheel every roadster
/// carries, or the trunk that takes its place on an Ornate one (#1367).
///
/// Published by the body because the car as built restated it - a spare at
/// `-0.462` of the length on the crown there - and on a shorter tail that
/// fraction is past the end: the wheel stood 34 mm clear of a bobtail's back,
/// touching it with nothing but its lowest rim.
#[derive(Clone, Copy, Debug)]
pub(super) enum TailMount {
    /// On the tail deck near the tip, leaning back - a boat-tail, which has no
    /// back to stand a wheel against.
    Deck {
        /// Station, as a z fraction of the overall length.
        at: f32,
        /// How far over the deck's crown a mounted wheel's centre rides, as a
        /// fraction of the length.
        lift: f32,
        /// How far the wheel leans back from upright (rad).
        lean: f32,
    },
    /// Against the blunt back panel of a bobtail or a tourer.
    Back {
        /// How far a mounted wheel is pressed into the panel, as a fraction of
        /// its own half-width: contact by CONSTRUCTION, because the touch
        /// guard cannot see a gap at a blunt end - its tube is a chain of
        /// capsules, and where a sweep closes fast they bulge out past the
        /// drawn end cap (#1367).
        sink: f32,
        /// How far the wheel leans back from upright (rad).
        lean: f32,
    },
}

/// Where the roadster puts the stations along its body - its own, not the
/// family's: a wagon has no cowl and no radiator (#1377), so these moved here
/// from the shared plan rather than every type filling in fields that mean
/// nothing to it.
///
/// Separate from the plan-form table because the two answer different
/// questions - the table is the body's *shape*, this is what is arranged along
/// it - and because the roadster builds several bodies over one layout idiom
/// (its boat-tail, bobtail and tourer, #1367).
#[derive(Debug)]
pub(super) struct Layout {
    /// Where the bonnet gives way to the scuttle, as a z fraction of the
    /// overall length.
    cowl: f32,
    /// The cockpit opening's after and forward ends, likewise. Everything
    /// between them is open: the tub's bored shell is the footwell and the
    /// scuttle and tail deck stop at these two stations.
    cockpit: (f32, f32),
    /// The seat back's station.
    seat: f32,
    /// The radiator shell's face.
    radiator: f32,
    /// The bumper bar, which is what actually sets the overall length.
    bumper: f32,
    /// The tail lamps' station, where this body's quarter still has the
    /// width to carry them. Restated as `-0.448` of the length the lamps stood
    /// 32 mm behind a bobtail's back (#1367).
    tail_lamps: f32,
    /// A second row's seat station, or `None` for a two-seater.
    bench: Option<f32>,
    /// The tail's one mount.
    tail: TailMount,
}

/// The roadster's plan: the family's [`BodyPlan`] and the roadster's own
/// [`Layout`] along it. Every roadster part reads this; it derefs to the
/// plan, so the shared surface (`crown_at`, `side_at`, the wheels) reads
/// exactly as before.
#[derive(Clone, Copy, Debug)]
pub(super) struct RoadsterPlan {
    plan: BodyPlan,
    layout: &'static Layout,
}

impl std::ops::Deref for RoadsterPlan {
    type Target = BodyPlan;
    fn deref(&self) -> &BodyPlan {
        &self.plan
    }
}

impl RoadsterPlan {
    /// The scuttle break - the bonnet's after end, and the screen's foot.
    pub(super) fn cowl_z(&self) -> f32 {
        self.layout.cowl * self.length
    }

    /// The cockpit opening's after and forward ends (m).
    pub(super) fn cockpit_z(&self) -> (f32, f32) {
        (
            self.layout.cockpit.0 * self.length,
            self.layout.cockpit.1 * self.length,
        )
    }

    /// The seat back's station (m).
    pub(super) fn seat_z(&self) -> f32 {
        self.layout.seat * self.length
    }

    /// The radiator shell's face, and the bumper bar (m).
    pub(super) fn radiator_z(&self) -> f32 {
        self.layout.radiator * self.length
    }
    pub(super) fn bumper_z(&self) -> f32 {
        self.layout.bumper * self.length
    }

    /// Where a hood, a tonneau or a hardtop seats: the coaming plane, centred
    /// on the cockpit.
    ///
    /// **Published by the body** (#1364 item 1), which is the whole of that
    /// item. The legacy canopy chose its own height and hovered over three of
    /// the four chassis it could land on; a canopy that asks cannot. The
    /// hardtop is its first reader (#1367): the cabin stands on this plane,
    /// centred on this station, and the folded hood lies on its plane at the
    /// cockpit's after rim.
    pub(super) fn canopy_seat(&self) -> [f32; 3] {
        let (aft, fwd) = self.cockpit_z();
        [0.0, 0.0, (aft + fwd) * 0.5]
    }

    /// The tail lamps' station (m).
    pub(super) fn tail_lamps_z(&self) -> f32 {
        self.layout.tail_lamps * self.length
    }

    /// The second row's seat station (m), if the body has one.
    pub(super) fn bench_z(&self) -> Option<f32> {
        self.layout.bench.map(|b| b * self.length)
    }

    /// The tail's one mount.
    pub(super) fn tail_mount(&self) -> TailMount {
        self.layout.tail
    }
}

/// The boat-tail's stations. Its tail lamps and spare are where the car as
/// built put them - now read off here rather than restated where they are
/// drawn.
const BOAT_TAIL_LAYOUT: Layout = Layout {
    cowl: 0.082,
    cockpit: (-0.320, -0.040),
    seat: -0.250,
    radiator: 0.437,
    bumper: 0.492,
    tail_lamps: -0.448,
    bench: None,
    tail: TailMount::Deck {
        at: -0.462,
        lift: 0.0445,
        lean: 0.35,
    },
};

/// How far a wheel on a blunt back is pressed into it, and how far it leans:
/// nearly upright, bedded a third of its own half-width deep.
const BACK: TailMount = TailMount::Back {
    sink: 0.30,
    lean: 0.10,
};

/// The bobtail's stations: the tail lamps on the rounded quarter where it is
/// still half a beam wide, and the spare against the back.
const BOBTAIL_LAYOUT: Layout = Layout {
    tail_lamps: -0.428,
    tail: BACK,
    ..BOAT_TAIL_LAYOUT
};

/// The tourer's stations: the cockpit opened back to the second row, whose
/// bench is the only part this body adds. The front seat, the screen and the
/// steering stay where the roadster has them.
const TOURER_LAYOUT: Layout = Layout {
    cockpit: (-0.470, -0.040),
    tail_lamps: -0.488,
    bench: Some(-0.425),
    tail: BACK,
    ..BOAT_TAIL_LAYOUT
};

/// The plan form and layout of a body - the ONLY thing a body is. The deck,
/// the cockpit, every trim line and every mount follow them without being
/// told.
fn form(body: RoadsterBody) -> (&'static [(f32, f32)], &'static Layout) {
    match body {
        RoadsterBody::BoatTail => (BOAT_TAIL, &BOAT_TAIL_LAYOUT),
        RoadsterBody::Bobtail => (BOBTAIL, &BOBTAIL_LAYOUT),
        RoadsterBody::Tourer => (TOURER, &TOURER_LAYOUT),
    }
}

pub(super) struct Roadster;

impl SkiffCraft for Roadster {
    fn plan(&self, bp: &SkiffBlueprint, seed: u64) -> BodyPlan {
        plan_of(
            bp,
            RoadsterBody::for_seed(seed),
            RoadsterWheels::for_seed(seed),
        )
        .plan
    }

    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator {
        build_dressed(
            ctx,
            &with_layout(*plan, RoadsterBody::for_seed(ctx.seed)),
            RoadsterTop::for_seed(ctx.seed),
            RoadsterWheels::for_seed(ctx.seed),
        )
    }

    fn feel(&self) -> SkiffFeel {
        // The retired default chassis's numbers exactly. The chassis *classes*
        // carried the feel before #1364 and the roadster is what the default
        // chassis was, so the drive the owner validated with the scale bridge
        // (#1361) is the drive that ships.
        //
        // #1381's sweep DROVE her and kept her, and she is the yardstick the
        // other five skiffs were set against: 40.1 km/h, 90% of it in 2.89 s,
        // 39.6 deg/s, round in 31.6 m = 11.8 of her own 2.67 m. The two
        // damping numbers are new LITERALS and not a new value: they are
        // `CarParams`' own defaults, which is what every skiff silently
        // shared before the struct grew them, so her published record is
        // bit-identical to the one #1361 signed off.
        //
        // One thing the sweep noticed and did not change: 11.8 lengths is a
        // very wide circle for a car, and a real roadster comes round in
        // three or four. If the whole land fleet ever reads as understeering,
        // the answer is a family-wide turn_accel lift and not six separate
        // ones.
        SkiffFeel {
            mass_factor: 1.0,
            drive_accel: 8.9,
            turn_accel: 2.0,
            linear_damping: 0.8,
            angular_damping: 4.0,
        }
    }

    fn idle(&self) -> SkiffIdle {
        // THE BASELINE. `Propulsion::Engine`, so she idles, at 3.75 Hz.
        // Until #1400 that was 9 Hz, the family-wide constant, and her bank
        // clamp was 17.19 degrees, the `SKIFF_BANK_MAX` constant (0.3 rad)
        // it replaced. The owner turned the whole family down in #1400 - the
        // sway frequency into 0.2-0.45 and the bank into 0-8 degrees - and
        // every type's pace and lean were scaled together, so she keeps her
        // place among them: 0.4 of the old clamp is 6.88 degrees (0.12 rad).
        SkiffIdle {
            shiver: Some(Shiver {
                amplitude: 1.0,
                hz: 3.75,
            }),
            bank_degrees: 6.88,
        }
    }

    fn overall_width(&self, plan: &BodyPlan, _seed: u64) -> f32 {
        plan.drawn_width(WING_R * WING_SCALE_X * plan.length)
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan, seed: u64) -> [f32; 3] {
        let plan = &with_layout(*plan, RoadsterBody::for_seed(seed));
        match aura {
            // Exhaust and steam leave the pipe mouth, read off the body's own
            // flank - so the wisp leaves the pipe that is actually drawn
            // rather than a fraction of a nominal machine (#1364 item 8).
            ParticleAura::Exhaust | ParticleAura::Steam => {
                let p = coachwork::exhaust_path(plan);
                p[p.len() - 1].0
            }
            // Anything else is a flourish, and it belongs over the COCKPIT -
            // the seat the body publishes for a hood or a hardtop (#1364 item
            // 1) is exactly the station a decorative aura should hover on,
            // because it is where a person would be. Over a closed car it
            // hovers over the ROOF: at the open car's height it would be
            // inside the cabin, and the neon themes that close their cars are
            // the ones that trail a haze (#1367).
            _ => {
                let [x, _, z] = plan.canopy_seat();
                let y = match RoadsterTop::for_seed(seed) {
                    RoadsterTop::Open => plan.crown_at(z) * 1.5,
                    RoadsterTop::Hardtop => hardtop::roof_y(plan) + plan.length * 0.04,
                };
                [x, y, z]
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Engine
    }
}

/// The roadster's plan for a blueprint on a named body and wheel.
///
/// The wheel is part of the plan because a balloon tyre is a different outer
/// radius, and the axle line is one wheel radius over the ground by
/// construction ([`BodyPlan::on_wheels`]): the axle, the guards, the boards
/// and the beams all follow the tyre the machine actually rolls on.
pub(super) fn plan_of(
    bp: &SkiffBlueprint,
    body: RoadsterBody,
    rolls: RoadsterWheels,
) -> RoadsterPlan {
    let (stations, layout) = form(body);
    let plan = BodyPlan::new(bp, SECTION, stations, AXLES);
    RoadsterPlan {
        plan: plan.on_wheels(plan.wheel_r * wheels::wheels(rolls).radius_factor()),
        layout,
    }
}

/// A plan the family built for this seed, with its body's layout laid back
/// along it - what [`Roadster::build`] and [`Roadster::fx_mount`] draw with,
/// since the [`SkiffCraft`] seam carries only the shared [`BodyPlan`].
fn with_layout(plan: BodyPlan, body: RoadsterBody) -> RoadsterPlan {
    RoadsterPlan {
        plan,
        layout: form(body).1,
    }
}

/// The roadster with a named top on named wheels, dressed for the tiers `ctx`
/// carries - what [`Roadster::build`] draws with the seed's own picks, and
/// what the guards sweep every combination through.
///
/// The order is the agreed prototype's, part for part, which is what lets a
/// dump of this tree be checked node by node against it.
pub(super) fn build_dressed(
    ctx: &PartCtx,
    plan: &RoadsterPlan,
    top: RoadsterTop,
    rolls: RoadsterWheels,
) -> Generator {
    let c = skiff_colours(ctx);
    // Root: a hidden hub on the datum amidships, buried under the scuttle.
    // The body cannot BE the structural root - an elliptical section needs a
    // per-axis node scale and a structural root may not carry one (#798) -
    // and the datum is the one plane where a hub is inside the bodywork
    // whatever the seed, which is where the legacy skiff's exposed core
    // becomes impossible rather than merely fixed.
    let hub = dim(plan.length * 0.008);
    let mut root = prim(
        cuboid([hub; 3], c.machinery.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let kids = &mut root.children;
    let wheel = wheels::wheels(rolls);
    coachwork::body(kids, plan, &c);
    coachwork::radiator(kids, plan, &c);
    coachwork::wings(kids, plan, &c, dressing::wears_a_primer_wing(ctx.wear));
    running_gear::build(kids, plan, &c);
    wheel.build(kids, plan, &c);
    coachwork::lamps_and_bumper(kids, plan, &c);
    match top {
        RoadsterTop::Open => cockpit::build(kids, plan, &c),
        RoadsterTop::Hardtop => hardtop::build(kids, plan, &c),
    }
    dressing::dress(kids, plan, &c, top, wheel, ctx.ornateness, ctx.wear);
    root
}

// ---------------------------------------------------------------------------
// The helpers every part is built with - the scaled sweep is the family's,
// in `super::shape` (#1374); these two take their material by value
// ---------------------------------------------------------------------------

/// A turned part laid on an axis: the family's only other idiom.
fn turned(
    profile: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    material: SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
) -> Generator {
    prim(
        lathe(profile, resolution, smooth, material),
        at,
        quat_xyzw(rotation),
    )
}

/// A thin swept line - a bead, a rubbing strip, a rod.
fn line(points: &[([f32; 3], f32)], resolution: u32, m: SovereignMaterialSettings) -> Generator {
    let pts: Vec<([f32; 3], f32)> = points.iter().map(|&(p, r)| (p, dim(r))).collect();
    prim(spine(&pts, resolution, m), [0.0; 3], id_quat())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::VehicleStance;

    fn hero(length: f32) -> SkiffBlueprint {
        SkiffBlueprint {
            stance: VehicleStance::Sleek,
            length,
            body_w: length * 0.272,
            wheelbase: length * 0.64,
            track: length * 0.42,
            wheel_r: length * 0.115,
            beltline: length * 0.300,
            height: length * 0.372,
        }
    }

    /// Every body's tail lamps and tail mount stand ON that body (#1367
    /// defects 3 and 4). Written as fractions restated where they were
    /// drawn, the lamps stood 32 mm behind a bobtail's back and the spare
    /// 34 mm clear of it.
    ///
    /// A binary test rather than trusting the connectedness guard, because the
    /// guard cannot see this: its tube is a chain of capsules, and at a blunt
    /// end they bulge past the drawn end cap - by up to 128 mm on a bobtail -
    /// so a wheel standing clear of the back still reads as touching. What is
    /// checked here is the DRAWN end: a back-mounted wheel's inboard face has
    /// to be past the tail station, where the body is.
    #[test]
    fn every_bodys_tail_mounts_stand_on_the_body() {
        for length in [1.90f32, 2.65, 3.60] {
            for body in RoadsterBody::ALL {
                for rolls in RoadsterWheels::ALL {
                    let plan = plan_of(&hero(length), body, rolls);
                    let (tail, aft) = (plan.tail_z(), plan.cockpit_z().0);
                    let lamps = plan.tail_lamps_z();
                    assert!(
                        lamps > tail && lamps < aft,
                        "{body:?}: tail lamps at {lamps} off a body from {tail} to its cockpit at {aft}"
                    );
                    let spare = dressing::tail_spare_face_z(&plan, wheels::wheels(rolls));
                    match plan.tail_mount() {
                        TailMount::Back { .. } => assert!(
                            spare > tail,
                            "{body:?} on {rolls:?}: the spare's face is at {spare}, behind the \
                             tail at {tail}"
                        ),
                        TailMount::Deck { at, .. } => assert!(
                            at * plan.length > tail && at * plan.length < aft,
                            "{body:?}: the deck mount is off the deck"
                        ),
                    }
                }
            }
        }
    }

    /// A balloon tyre changes the PLAN's wheel radius and nothing else: the
    /// axle rises with it, the body stays where the beltline puts it, and the
    /// ground is where it was.
    #[test]
    fn balloons_raise_the_axle_and_leave_the_body() {
        let bp = hero(2.65);
        for body in RoadsterBody::ALL {
            let disc = plan_of(&bp, body, RoadsterWheels::Disc);
            let balloon = plan_of(&bp, body, RoadsterWheels::Balloon);
            assert!(balloon.wheel_r > disc.wheel_r);
            assert!((balloon.datum_height() - disc.datum_height()).abs() < 1e-6);
            assert!(
                (balloon.axle_y() - disc.axle_y() - (balloon.wheel_r - disc.wheel_r)).abs() < 1e-6
            );
        }
    }
}
