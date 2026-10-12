//! The biplane: a 1930s radial-engined biplane taildragger - two staggered
//! wings joined by an N interplane strut and one X of wires a side, the
//! upper wing carried over two open cockpits on N cabane struts, a radial in
//! an open cowl ring, a two-blade wooden airscrew and fixed taildragger
//! gear (#1623). The airplane fan-out's first type (#1618), at home on the
//! old worlds, the frontier and the boiler age.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1623, all eight
//! recommendations): `target/dump/aircraft-p7-biplane/biplane.py` is its
//! Python twin, and this port reads its defaults - the recommendation: open
//! cockpits with lit wells, one X of wires a side, the radial - number for
//! number, in the prototype's own frame ([`FusePlan::at`] maps it to the
//! drawn one). Drawn at the blueprint's length, airscrew tip to rudder, her
//! parts are 3.7 % larger at the nominal 3.19 m than the prototype drew
//! them: a 4.31 m upper span.
//!
//! [`fuselage`] draws the fuselage, the skins laid on it - the cheat line
//! and the two cockpits - the radial in its cowl ring, the exhaust stack and
//! the airscrew; [`wings`] the two wings, their struts, the wires and the
//! lights; [`gear`] the mains and the tail wheel; [`tail`] the stabiliser,
//! the fin and the tail light; and [`dressing`] the ladder of secondary
//! masses by ornateness.
//!
//! # Where the port leaves the prototype, and why
//!
//! Each change is a fleet rule's or a guard's:
//!
//! 1. The cheat line is the seeded ACCENT (the identity rule #1365) and the
//!    cockpit wells the fleet's lit window - the tertiary's light, held off
//!    her fuselage's colour (`livery::standing_off`) - as the high-wing's,
//!    where the prototype drew the line in the trim colour and picked one
//!    light per scheme by eye.
//! 2. The airscrew turns 3 cm AHEAD of the cowl ring's lip, on a shaft out
//!    of the crankcase: the prototype hung it inside the ring's mouth, where
//!    its blades swept through the ring's wall. The 6.5 cm it moved is
//!    length her blueprint's airframe includes, so every other part of her
//!    is drawn 2 % smaller for it.
//! 3. Each blade narrows its CHORD toward its own tip, as the high-wing's
//!    does: the prototype tapered its thickness.
//! 4. Her box holds her cowl ring ([`FuseProfile::cowl_ring`]): a radial's
//!    ring stands 6 cm proud of her widest section each side.
//!
//! What a part fixed rigidly off a wing reads the fuselage at - a cabane
//! strut's foot, a wing walk's root - is read where the part is drawn
//! ([`FusePlan::unstretch`]), as the prototype read it: the port's frame
//! keeps stations unstretched, the prototype's kept them stretched.

mod dressing;
mod fuselage;
mod gear;
mod tail;
mod wings;

pub(crate) use fuselage::BLADE_PASS_HZ;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirplaneBlueprint, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    AirplaneCraft, AirplaneFeel, AirplaneLayout, BiplaneColours, FusePlan, FuseProfile, Handling,
    biplane_colours, dim,
};

/// The fuselage, `(z, centreline height, radius)` crankcase to tail post, in
/// the prototype's frame at girth 1 - the prototype's `FUSE_OPEN`. The nose
/// is the radial's crankcase inside its cowl; the fuselage proper starts at
/// the firewall, `1.12`. Adjacent gaps stay within twice each other (a sweep
/// is a uniform Catmull-Rom, #1616).
const FUSE: &[(f32, f32, f32)] = &[
    (1.450, 0.700, 0.090),
    (1.380, 0.700, 0.150),
    (1.270, 0.700, 0.178),
    (1.120, 0.700, 0.185),
    (0.920, 0.702, 0.186),
    (0.620, 0.708, 0.182),
    (0.270, 0.716, 0.168),
    (-0.150, 0.730, 0.138),
    (-0.600, 0.748, 0.102),
    (-1.050, 0.766, 0.068),
    (-1.400, 0.778, 0.042),
];

/// Her fuselage: the stations above, drawn 1.35 times taller than wide; the
/// cockpits' run from the firewall at `1.12` to the rear cockpit at `0.27`;
/// the airframe from the hubcap's tip, ahead of the cowl ring, to the
/// rudder's trailing edge - read back off the drawn tree by
/// `the_box_holds_the_drawn_airframe`; her box over her deepest section at
/// `0.92`, its floor 12 cm up, so that the box's tail corner clears the
/// ground standing twelve degrees tail-down at the longest cabin; and her
/// cowl ring's radius, which the box's width holds.
pub(super) const PROFILE: FuseProfile = FuseProfile {
    stations: FUSE,
    section: 1.35,
    firewall: 1.12,
    cabin_rear: 0.27,
    nose: fuselage::PROP_Z + fuselage::HUBCAP_LEN,
    tail: -1.465,
    box_station: 0.92,
    box_floor_rise: 0.12,
    cowl_ring: fuselage::COWL_R,
};

/// The wings (prototype frame): each one's leading edge station - the upper
/// staggered 25 cm ahead - their shared chord, their spans, their thickness
/// over chord, the lower wing's dihedral (degrees; the upper is flat), how
/// far the upper wing's underside stands over the crown and the lower wing's
/// over the belly.
const LOWER_LE: f32 = 1.02;
const UPPER_LE: f32 = 1.27;
const CHORD: f32 = 0.59;
const LOWER_SPAN: f32 = 3.90;
const UPPER_SPAN: f32 = 4.15;
const THICK: f32 = 0.12;
const LOWER_DIHEDRAL_DEG: f32 = 3.0;
const GAP: f32 = 0.22;
const LOWER_RISE: f32 = 0.055;

/// The main wheels' axle station, half their track and their tyres' outer
/// radius; the tail wheel's axle station and its tyre's; and the attitude
/// she stands at on all three (prototype frame, degrees).
const MAIN_Z: f32 = 1.00;
const TRACK: f32 = 0.42;
const WHEEL_R: f32 = 0.165;
const TAIL_Z: f32 = -1.32;
const TAIL_R: f32 = 0.05;
const GROUND_DEG: f32 = 12.0;

/// The side of every hidden hub cube (m, prototype frame): the root at the
/// box's middle, the airscrew's pivot and the wheels'.
const HUB_CUBE: f32 = 0.02;

/// Half a wing's thickness at its mid-chord (prototype frame).
fn half_thick() -> f32 {
    THICK * CHORD * 0.5
}

/// The lower wing's centre height at span station `x` (prototype frame):
/// through the fuselage under its lower longerons, rising with its
/// dihedral.
fn lower_y(plan: &FusePlan, x: f32) -> f32 {
    plan.sill(LOWER_LE - CHORD * 0.5)
        + LOWER_RISE
        + half_thick()
        + x.abs() * LOWER_DIHEDRAL_DEG.to_radians().tan()
}

/// The upper wing's centre height (prototype frame): its underside
/// [`GAP`] over the crown under its mid-chord, flat across.
fn upper_y(plan: &FusePlan) -> f32 {
    plan.crown(UPPER_LE - CHORD * 0.5) + GAP + half_thick()
}

/// The tail wheel's axle height (prototype frame): set so the line from the
/// mains' contact to the tail wheel's stands [`GROUND_DEG`] off level,
/// over the stretched distance between them.
fn tail_wheel_y(plan: &FusePlan) -> f32 {
    (plan.stretch(MAIN_Z) - plan.stretch(TAIL_Z)) * GROUND_DEG.to_radians().tan() + TAIL_R
}

pub(super) struct Biplane;

impl AirplaneCraft for Biplane {
    fn plan(&self, bp: &AirplaneBlueprint) -> FusePlan {
        FusePlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &FusePlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> AirplaneFeel {
        // #1623 Q-G: slower and nimbler than the high-wing - a big wing and
        // a lot of drag. She stalls at 9 m/s against the preset's 11, flies
        // level with her nose on her path at 17 against 20 and tops out
        // about 21 against 25, on 1.4 times the drag at the same thrust per
        // kilo; she rolls at 120 degrees a second against 90 and pitches at
        // 50 against 45, and yaws as the preset does. A tenth lighter for her
        // length: fabric over a frame.
        AirplaneFeel {
            mass_factor: 0.9,
            handling: Some(Handling {
                stall: 9.0,
                cruise: 17.0,
                drag_factor: 1.4,
                roll_rate: 120.0,
                pitch_rate: 50.0,
                yaw_rate: 25.0,
            }),
        }
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
        // Her centre of mass between her two wings' mid-chords, where a
        // staggered pair's lift is centred - 15 cm behind her mains at the
        // nominal cabin.
        let lower = plan.at_off(0.0, 0.0, LOWER_LE, -CHORD * 0.5)[2];
        let upper = plan.at_off(0.0, 0.0, UPPER_LE, -CHORD * 0.5)[2];
        AirplaneLayout {
            half_extents: half,
            stance,
            floor_over_mains: over(mains, WHEEL_R),
            floor_over_tail: over(tail, TAIL_R),
            gear_track: mains[0] / hx,
            gear_ahead: mains[2] / hz,
            tail_wheel_aft: -tail[2] / hz,
            center_of_mass_ahead: (lower + upper) * 0.5 / hz,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &FusePlan) -> [f32; 3] {
        match aura {
            // An engine's exhaust, steam or sparks leave the stack's mouth
            // behind her cowl - the one place anything burns.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                let (x, y, z) = fuselage::exhaust_mouth(plan);
                plan.at(x, y, z)
            }
            // Everything else issues under her front cockpit, between her
            // mains and her tail: a wash blows down from the belly, and a
            // flourish rises round her from under it - never into the
            // airscrew's disc.
            _ => {
                let z = BELLY_FX_Z;
                plan.at(0.0, plan.sill(z) - 0.08, z)
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Radial
    }
}

/// The station under the front cockpit a belly aura issues from (prototype
/// frame): behind the mains, under the lower wing.
const BELLY_FX_Z: f32 = 0.62;

/// The biplane dressed for the tiers `ctx` carries - what [`Biplane::build`]
/// draws, and what the guards sweep every tier through.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it; the tier ladder comes
/// after what the prototype drew.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &FusePlan) -> Generator {
    let c = biplane_colours(ctx);
    // Root: a hidden hub at the box's middle, buried in the fuselage.
    let hub = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([hub; 3], c.dark.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    fuselage::fuselage(kids, plan, &c);
    fuselage::skins(kids, plan, &c);
    fuselage::cockpits(kids, plan, &c);
    fuselage::radial(kids, plan, &c);
    fuselage::exhaust(kids, plan, &c);
    fuselage::airscrew(kids, plan, &c);
    wings::wings(kids, plan, &c);
    wings::interplane_struts(kids, plan, &c);
    wings::cabane_struts(kids, plan, &c);
    wings::wires(kids, plan, &c);
    gear::mains(kids, plan, &c);
    gear::tail_wheel(kids, plan, &c);
    tail::stabiliser(kids, plan, &c);
    tail::fin(kids, plan, &c);
    wings::lights(kids, plan, &c);
    tail::tail_light(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness);
    root
}

/// A hidden hub cube at `at` - an airscrew's or a wheel's pivot - for the
/// parts it turns, which are laid relative to it.
fn hub(plan: &FusePlan, at: [f32; 3], c: &BiplaneColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}
