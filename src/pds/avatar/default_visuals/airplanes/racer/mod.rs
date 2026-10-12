//! The air racer: a single-seat racing taildragger - a slim fuselage from a
//! pointed electric cowl behind a spinner to a swept fin, a dark bubble
//! canopy with its lower sides lit, a short low wing, spatted mains and a
//! tail wheel - the Formula One pylon racer's layout, drawn for the near
//! future (#1629). The airplane fan-out's third type (#1618), at home on the
//! five speculative and alien themes.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1629, all nine recommendations
//! taken): `target/dump/aircraft-p7-racer/racer.py` is its Python twin, and
//! this port reads its defaults - the electric nose, the bubble canopy, the
//! spats, the full proportion - number for number, in the prototype's own
//! frame ([`FusePlan::at`] maps it to the drawn one).
//!
//! [`fuselage`] draws the fuselage and what is laid on it - the racing
//! stripe and the canopy; [`nose`] the cooling scoop and the airscrew;
//! [`wing`] the wing and its nav lights; [`gear`] the mains in their spats
//! and the tail wheel; [`tail`] the stabiliser, the fin and the tail light;
//! and [`dressing`] the ladder of secondary masses by ornateness.
//!
//! # Her proportion is drawn into her profile
//!
//! The owner took the FULL proportion (#1629 Q-I): her fuselage 12 percent
//! fuller than a true racer's, on the family's own girth rule - every
//! station's radius grown, its centreline raised by its half-height's growth,
//! the belly where it was - and her wing, tail, canopy, spats and airscrew
//! grown to suit, so she holds her own beside the family at play distance.
//! [`FUSE`] is the true racer's stations with that fullness drawn in
//! ([`full`]); the blueprint's girth grows her from there.
//!
//! # The wing does not stretch with the cockpit
//!
//! Her cockpit's run - the firewall at `0.95` to the cockpit's rear at
//! `0.12` - lies over her wing's root chord, and a longer cockpit carries her
//! tail aft and leaves her wing where it is, as the prototype drew it: the
//! wing and everything on it is hung RIGIDLY off the firewall's station
//! ([`wing::on_wing`]), which no stretch moves. Her mains, her canopy and her
//! cooling scoop ride the cockpit's stations, as the prototype's do.
//!
//! # Where the port leaves the prototype, and why
//!
//! 1. The racing stripe is the seeded ACCENT (the identity rule #1365) and
//!    the canopy's lit sides the fleet's lit window - the tertiary's light,
//!    held off her body's colour ([`RacerColours`]) - where the prototype
//!    drew the stripe in the trim colour and picked one light per scheme by
//!    eye; and her schemes' finish is her theme's
//!    [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s, as every
//!    airplane's is, where the prototype gave each scheme a sheen of its
//!    own.
//! 2. Her thin parts are floored at [`MIN_DIM`](super::MIN_DIM) where her
//!    blueprint draws her smaller than the prototype: her blades, her
//!    cooling scoop's mouth, the tail wheel's fork and the pitot.
//!
//! # One fix to the agreed prototype
//!
//! Her airscrew's hub stands 4.5 cm ahead of her cowl's face, where the
//! prototype drew it 3 cm ahead: on her fattest fuselage (girth 1.28) the
//! face's ring, square to a centreline that climbs aft steeply off the small
//! face, leant its rim 8 mm into the disc her blades sweep
//! (`a_racers_airscrew_turns_clear_of_her_cowl_and_the_ground`). Her
//! spinner stands where it was - its base on the face, its tip her nose - so
//! only her blades moved, and `racer.py` carries the fix.

mod dressing;
mod fuselage;
mod gear;
mod nose;
mod tail;
mod wing;

pub(crate) use nose::BLADE_PASS_HZ;
#[cfg(test)]
pub(super) use nose::{PROP_GAIN_DEG_PER_M, PROP_IDLE_DEG_S};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirplaneBlueprint, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    AirplaneCraft, AirplaneFeel, AirplaneLayout, FusePlan, FuseProfile, Handling, RacerColours,
    dim, racer_colours,
};

/// How much taller her section is than it is wide.
const SECTION: f32 = 1.30;

/// A true racer's fuselage, `(z, centreline height, radius)` cowl face to
/// tail post, in the prototype's frame at girth 1 - the prototype's electric
/// `NOSES` and its `FUSE_TAIL`. Adjacent gaps stay within twice each other
/// (a sweep is a uniform Catmull-Rom, #1616).
const TRUE_FUSE: [(f32, f32, f32); 13] = [
    (1.500, 0.470, 0.058),
    (1.430, 0.473, 0.100),
    (1.320, 0.477, 0.135),
    (1.160, 0.481, 0.160),
    (0.950, 0.485, 0.174),
    (0.700, 0.490, 0.180),
    (0.420, 0.496, 0.176),
    (0.120, 0.505, 0.160),
    (-0.220, 0.518, 0.132),
    (-0.580, 0.534, 0.100),
    (-0.940, 0.552, 0.072),
    (-1.250, 0.568, 0.052),
    (-1.480, 0.580, 0.036),
];

/// Her fullness over a true racer's (#1629 Q-I): an extra girth.
const FULLNESS: f32 = 1.12;

/// `base` with her fullness drawn in, on the family's girth rule - each
/// radius grown by [`FULLNESS`], each centreline raised by its
/// half-height's growth - so the belly stays where the wing passes under it.
const fn full(base: [(f32, f32, f32); 13]) -> [(f32, f32, f32); 13] {
    let mut out = base;
    let mut i = 0;
    while i < base.len() {
        let (z, y, r) = base[i];
        out[i] = (z, y + (FULLNESS - 1.0) * r * SECTION, r * FULLNESS);
        i += 1;
    }
    out
}

/// Her fuselage at girth 1: the true racer's, full.
const FUSE: [(f32, f32, f32); 13] = full(TRUE_FUSE);

/// Her fuselage: the stations above, drawn 1.30 times taller than wide; the
/// cockpit's run from the firewall at `0.95` to its rear at `0.12`; the
/// airframe from the spinner's tip to the rudder's trailing edge - its trim
/// cap's, read back off the drawn tree by `the_box_holds_the_drawn_airframe`;
/// and her box over the cockpit's deepest section at `0.70`, its floor 22 cm
/// up - so that, standing ten degrees tail-down with her tail running 32 cm
/// past her tail wheel, the box's tail corner clears the ground by 2 cm at
/// her longest cockpit; at 20 cm it cleared by 4 mm.
pub(super) const PROFILE: FuseProfile = FuseProfile {
    stations: &FUSE,
    section: SECTION,
    firewall: 0.95,
    cabin_rear: 0.12,
    nose: nose::SPINNER_TIP,
    tail: tail::TAIL_REACH,
    box_station: 0.70,
    box_floor_rise: 0.22,
    cowl_ring: 0.0,
};

/// The mains' `x`, the main axles' station and the tyres' outer radius; the
/// tail wheel's station and its tyre's; and the attitude she stands at on
/// all three (prototype frame, degrees).
const MAIN_X: f32 = 0.48;
const MAIN_Z: f32 = 0.90;
const WHEEL_R: f32 = 0.11;
const TAIL_Z: f32 = -1.25;
const TAIL_R: f32 = 0.035;
const GROUND_DEG: f32 = 10.0;

/// The side of every hidden hub cube (m, prototype frame): the root at the
/// box's middle, the airscrew's pivot and the wheels'.
const HUB_CUBE: f32 = 0.02;

/// The station under her fuselage a belly aura issues from (prototype
/// frame): behind her wing's root, ahead of her tail wheel.
const BELLY_FX_Z: f32 = -0.10;

/// The tail wheel's axle height (prototype frame): set so the line from the
/// mains' contact to the tail wheel's stands [`GROUND_DEG`] off level, over
/// the stretched distance between them.
fn tail_wheel_y(plan: &FusePlan) -> f32 {
    (plan.stretch(MAIN_Z) - plan.stretch(TAIL_Z)) * GROUND_DEG.to_radians().tan() + TAIL_R
}

pub(super) struct Racer;

impl AirplaneCraft for Racer {
    fn plan(&self, bp: &AirplaneBlueprint) -> FusePlan {
        FusePlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &FusePlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> AirplaneFeel {
        // #1629 Q-G: the fastest and the quickest-rolling of the four. She
        // stalls at 13 m/s against the preset's 11 - 14 stretched her
        // take-off past the 40 m the agent's pilot sweeps clear - flies
        // level with her nose on her path at 26 against 20 and tops out
        // about 31.6 against 25, on 0.6 times the drag at the same thrust per
        // kilo; she rolls at 150 degrees a second against 90, pitches at 60
        // against 45 and yaws at 30 against 25. A fifth lighter for her
        // length: a racer.
        AirplaneFeel {
            mass_factor: 0.8,
            handling: Some(Handling {
                stall: 13.0,
                cruise: 26.0,
                drag_factor: 0.6,
                roll_rate: 150.0,
                pitch_rate: 60.0,
                yaw_rate: 30.0,
            }),
        }
    }

    fn layout(&self, plan: &FusePlan) -> AirplaneLayout {
        let half = plan.half_extents();
        let [hx, _, hz] = half;
        // Everything in the drawn frame: her nose at +Z, the box's middle
        // at the origin, so `+z` is AHEAD - the travel pose turns her round.
        let floor = plan.at(0.0, plan.box_floor(), 0.0)[1];
        let mains = plan.at(MAIN_X, WHEEL_R, MAIN_Z);
        let tail = plan.at(0.0, tail_wheel_y(plan), TAIL_Z);
        let stance = GROUND_DEG.to_radians();
        // Her floor over a wheel's lowest point, plumb: the hub hangs under
        // the floor along her own down, which leans with her, and the
        // tyre's lowest point is a radius under the hub at any pitch.
        let over = |hub: [f32; 3], radius: f32| (floor - hub[1]) * stance.cos() + plan.len(radius);
        // Her centre of mass over her wing's quarter chord at its root,
        // where a straight wing's lift is centred - 28 cm behind her mains.
        let lift = wing::quarter_chord(plan);
        AirplaneLayout {
            half_extents: half,
            stance,
            floor_over_mains: over(mains, WHEEL_R),
            floor_over_tail: over(tail, TAIL_R),
            gear_track: mains[0] / hx,
            gear_ahead: mains[2] / hz,
            tail_wheel_aft: -tail[2] / hz,
            center_of_mass_ahead: lift / hz,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &FusePlan) -> [f32; 3] {
        match aura {
            // An electric racer has no stack: a picked exhaust or steam is
            // dropped (`fx::drawn_aura`) - her Solarpunk and SpaceOutpost
            // seeds' exhaust floor among them - and embers, the one burning
            // aura that stands, would leave the tail of her cooling scoop
            // under her nose, where her motor's heat does. None of her homes
            // picks them: defensive.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                nose::scoop_tail(plan)
            }
            // Everything else issues under her fuselage, behind her wing's
            // root: a wash blows down from the belly, and a flourish rises
            // round her from under it - never into her airscrew's disc.
            _ => plan.at(0.0, plan.sill(BELLY_FX_Z) - 0.08, BELLY_FX_Z),
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::ElectricProp
    }
}

/// The racer dressed for the tiers `ctx` carries - what [`Racer::build`]
/// draws, and what the guards sweep every tier through.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it; the tier ladder comes
/// after what the prototype drew.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &FusePlan) -> Generator {
    let c = racer_colours(ctx);
    // Root: a hidden hub at the box's middle, buried in the cockpit.
    let side = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    fuselage::fuselage(kids, plan, &c);
    fuselage::stripe(kids, plan, &c);
    fuselage::canopy(kids, plan, &c);
    wing::wing(kids, plan, &c);
    nose::scoop(kids, plan, &c);
    nose::airscrew(kids, plan, &c);
    gear::mains(kids, plan, &c);
    gear::tail_wheel(kids, plan, &c);
    tail::stabiliser(kids, plan, &c);
    tail::fin(kids, plan, &c);
    wing::lights(kids, plan, &c, ctx.ornateness);
    tail::tail_light(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness);
    root
}

/// A hidden hub cube at `at` - the airscrew's or a wheel's pivot - for the
/// parts it turns, which are laid relative to it.
fn hub(plan: &FusePlan, at: [f32; 3], c: &RacerColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}
