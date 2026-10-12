//! The high-wing light monoplane taildragger: a round-cowled cabin under a
//! strut-braced wing, on fat tyres and a tail wheel - the Cub's layout, and
//! the family's hero and universal floor (#1617).
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1616, #1359 rules 1, 12 and 14):
//! `target/dump/aircraft-p5/plane.py` is its Python twin, and this port reads
//! it number for number, in the prototype's own frame ([`FusePlan::at`] maps
//! it to the drawn one). It is drawn at the proportion the owner chose - C,
//! the fuselage 18 % fuller than a true one's at the true one's size - which
//! is the nominal blueprint.
//!
//! [`fuselage`] draws the fuselage, the skins laid on it, the cowl's dressing
//! and the airscrew; [`wing`] the wing, its struts and its lights; [`tail`]
//! the stabiliser, the fin and the tail light; [`gear`] the mains and the
//! tail wheel; and [`dressing`] the ladder of secondary masses by
//! ornateness.
//!
//! # Where the port leaves the prototype, and why
//!
//! Five small changes, each one either a fleet rule or a guard's:
//!
//! 1. The cheat line down each flank is the seeded ACCENT, where the
//!    prototype drew it in the scheme's second hue - the fleet's identity
//!    rule (#1365), as the helicopter's boom stripes are.
//! 2. The cabin windows are the fleet's lit window - the tertiary's light -
//!    held off her body's own colour (`livery::standing_off`), where the
//!    prototype picked one light per livery by eye.
//! 3. The spinner stands 5 mm INTO the cowl's face, where the prototype hung
//!    it 5 mm off it; and the tail wheel's leaf, the struts' feet and the
//!    gear legs are floored at [`MIN_DIM`](super::MIN_DIM) - both for
//!    `touch::assert_one_machine` and the sanitiser's floor.
//! 4. Each airscrew blade narrows toward its OWN tip: the prototype tapered
//!    both toward `+Y`, which drew the lower blade fat at its tip.
//! 5. The airscrew spins up to the sanitiser's ceiling at her cruise (540
//!    degrees a second idling and 55 more a metre a second), where the
//!    prototype's 70 asked past it - the runtime caps a speed term's rate
//!    at four turns a second whatever its gain.

mod dressing;
mod fuselage;
mod gear;
mod tail;
mod wing;

pub(crate) use fuselage::BLADE_PASS_HZ;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirplaneBlueprint, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim, rod, sweep};
use super::{
    AirplaneColours, AirplaneCraft, AirplaneFeel, AirplaneLayout, FusePlan, FuseProfile,
    airplane_colours, dim,
};

/// The fuselage, `(z, centreline height, radius)` spinner to tail post, in
/// the prototype's frame at girth 1 - the prototype's `FUSE_BASE`.
const FUSE: &[(f32, f32, f32)] = &[
    (1.470, 0.735, 0.105),
    (1.400, 0.730, 0.168),
    (1.260, 0.722, 0.205),
    (1.080, 0.716, 0.222),
    (0.800, 0.714, 0.230),
    (0.400, 0.714, 0.232),
    (0.080, 0.722, 0.218),
    (-0.300, 0.742, 0.180),
    (-0.800, 0.772, 0.130),
    (-1.250, 0.800, 0.085),
    (-1.500, 0.818, 0.050),
];

/// Her fuselage: the stations above, drawn 1.35 times taller than wide; the
/// cabin's run from the firewall at `1.08` to the cabin's rear at `0.08`;
/// the airframe from the spinner's tip - the cowl's face at `1.47`, the hub
/// 5 mm into it and the spinner `0.14` long - to the rudder's trailing edge,
/// `0.075` aft of the tail post at its widest - read back off the drawn tree
/// by `the_box_holds_the_drawn_airframe`; and her box over the cabin's
/// deepest belly at `0.40`, its floor 8 cm up.
pub(super) const PROFILE: FuseProfile = FuseProfile {
    stations: FUSE,
    section: 1.35,
    firewall: 1.08,
    cabin_rear: 0.08,
    nose: 1.47 - fuselage::HUB_INSET + fuselage::SPINNER_LEN,
    tail: -1.575,
    box_station: 0.40,
    box_floor_rise: 0.08,
};

/// The wing (prototype frame): its leading edge's station, chord, span,
/// thickness over chord and dihedral (degrees).
const WING_LE: f32 = 0.82;
const CHORD: f32 = 0.70;
const SPAN: f32 = 4.70;
const THICK: f32 = 0.13;
const DIHEDRAL_DEG: f32 = 1.5;

/// The main wheels' axle station, half their track and their tyres' outer
/// radius; the tail wheel's axle station and its tyre's; and the attitude
/// she stands at on all three (prototype frame, degrees).
const MAIN_Z: f32 = 0.90;
const TRACK: f32 = 0.42;
const WHEEL_R: f32 = 0.155;
const TAIL_Z: f32 = -1.50;
const TAIL_R: f32 = 0.055;
const GROUND_DEG: f32 = 10.0;

/// The side of every hidden hub cube (m, prototype frame): the root at the
/// box's middle, the airscrew's pivot and the wheels'.
const HUB_CUBE: f32 = 0.02;

/// The wing's centre height at span station `x` (prototype frame): on the
/// cabin roof under its mid-chord, sunk 1.2 cm into it, and rising with
/// the dihedral.
fn wing_y(plan: &FusePlan, x: f32) -> f32 {
    let roof = plan.crown(WING_LE - CHORD * 0.5);
    roof + THICK * CHORD * 0.5 - 0.012 + x.abs() * DIHEDRAL_DEG.to_radians().tan()
}

/// The wing's underside at span station `x` (prototype frame).
fn wing_under(plan: &FusePlan, x: f32) -> f32 {
    wing_y(plan, x) - THICK * CHORD * 0.5
}

/// The tail wheel's axle height (prototype frame): set so the line from the
/// mains' contact to the tail wheel's stands [`GROUND_DEG`] off level,
/// over the stretched distance between them.
fn tail_wheel_y(plan: &FusePlan) -> f32 {
    (plan.stretch(MAIN_Z) - plan.stretch(TAIL_Z)) * GROUND_DEG.to_radians().tan() + TAIL_R
}

pub(super) struct HighWing;

impl AirplaneCraft for HighWing {
    fn plan(&self, bp: &AirplaneBlueprint) -> FusePlan {
        FusePlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &FusePlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> AirplaneFeel {
        // #1616 recommendation 9: the preset's model as the owner validated
        // it (#1615), her mass and thrust in proportion to her length.
        AirplaneFeel { mass_factor: 1.0 }
    }

    fn layout(&self, plan: &FusePlan) -> AirplaneLayout {
        let half = plan.half_extents();
        let [hx, _, hz] = half;
        // Everything in the drawn frame: her nose at +Z, the box's middle
        // at the origin, so `+z` is AHEAD - the travel pose turns her round.
        let floor = plan.at(0.0, plan.box_floor(), 0.0)[1];
        let mains = plan.at(TRACK, WHEEL_R, MAIN_Z);
        let tail = plan.at(0.0, tail_wheel_y(plan), TAIL_Z);
        let stance = GROUND_DEG.to_radians();
        // Her floor over a wheel's lowest point, plumb: the hub hangs under
        // the floor along her own down, which leans with her, and the
        // tyre's lowest point is a radius under the hub at any pitch.
        let over = |hub: [f32; 3], radius: f32| (floor - hub[1]) * stance.cos() + plan.len(radius);
        // Her centre of mass over her wing's mid-chord, as a real one is.
        let wing = plan.at_off(0.0, 0.0, WING_LE, -CHORD * 0.5)[2];
        AirplaneLayout {
            half_extents: half,
            stance,
            floor_over_mains: over(mains, WHEEL_R),
            floor_over_tail: over(tail, TAIL_R),
            gear_track: mains[0] / hx,
            gear_ahead: mains[2] / hz,
            tail_wheel_aft: -tail[2] / hz,
            center_of_mass_ahead: wing / hz,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &FusePlan) -> [f32; 3] {
        match aura {
            // An engine's exhaust, steam or sparks leave the stack's mouth
            // under her cowl - the one place anything burns.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                let (x, y, z) = fuselage::exhaust_mouth(plan);
                plan.at(x, y, z)
            }
            // Everything else issues under her cabin, between her mains: a
            // wash blows down from the belly, and a flourish rises round the
            // cabin from under it - never into the airscrew's disc.
            _ => {
                let z = BELLY_FX_Z;
                plan.at(0.0, plan.sill(z) - 0.08, z)
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Propeller
    }
}

/// The station under the cabin a belly aura issues from (prototype frame):
/// under the wing, behind the mains.
const BELLY_FX_Z: f32 = 0.40;

/// The high-wing monoplane dressed for the tiers `ctx` carries - what
/// [`HighWing::build`] draws, and what the guards sweep every tier through.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it; the tier ladder comes
/// after what the prototype drew.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &FusePlan) -> Generator {
    let c = airplane_colours(ctx);
    // Root: a hidden hub at the box's middle, buried in the cabin.
    let hub = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([hub; 3], c.dark.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    fuselage::fuselage(kids, plan, &c);
    fuselage::skins(kids, plan, &c);
    fuselage::airscrew(kids, plan, &c);
    fuselage::cowl_dressing(kids, plan, &c);
    wing::wing(kids, plan, &c);
    wing::struts(kids, plan, &c);
    gear::mains(kids, plan, &c);
    gear::tail_wheel(kids, plan, &c);
    tail::stabiliser(kids, plan, &c);
    tail::fin(kids, plan, &c);
    wing::lights(kids, plan, &c);
    tail::tail_light(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness);
    root
}

/// A hidden hub cube at `at` - an airscrew's or a wheel's pivot - for the
/// parts it turns, which are laid relative to it.
fn hub(plan: &FusePlan, at: [f32; 3], c: &AirplaneColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}
