//! The twin-engine transport: a light twin taildragger - a cabin with a
//! glazed nose and a row of lit windows, a low tapered wing, a radial in a
//! closed cowl on a nacelle either side turning a two-blade metal airscrew,
//! the main wheels under the nacelles, a tail wheel and a twin-fin tail -
//! the Beech 18's layout (#1625). The airplane fan-out's second type
//! (#1618), at home on the city, the works and the campus.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1625, all eight
//! recommendations): `target/dump/aircraft-p7-twin/twin.py` is its Python
//! twin, and this port reads its defaults - twin fins, a row of five cabin
//! windows a side, two-blade airscrews - number for number, in the
//! prototype's own frame ([`FusePlan::at`] maps it to the drawn one). She is
//! true metres at the nominal 3.19 m airframe, nose tip to the fins'
//! trailing edges.
//!
//! [`fuselage`] draws the fuselage and what is laid on it - the cheat line,
//! the windscreen and the cockpit's windows, the cabin's lit windows;
//! [`wing`] the wing and its nav lights; [`engines`] the nacelles, the
//! cowls and the airscrews; [`gear`] the mains and the tail wheel; [`tail`]
//! the stabiliser, the fins and the tail light; and [`dressing`] the ladder
//! of secondary masses by ornateness.
//!
//! # The wing does not stretch with the cabin
//!
//! Her cabin's run - the cockpit's back wall at `0.78` to the cabin's rear
//! at `0.05` - lies over her wing's root chord, and a longer cabin carries
//! her tail aft and leaves her wing, her nacelles and her mains where they
//! are, as the prototype drew them: each is hung RIGIDLY off a station
//! ahead of the cockpit's back wall ([`FusePlan::at_off`]), which no stretch
//! moves.
//!
//! # Where the port leaves the prototype, and why
//!
//! Two of the fleet's rules, as every airplane's port takes them:
//!
//! 1. The cheat line is the seeded ACCENT (the identity rule #1365) and the
//!    cabin windows the fleet's lit window - the tertiary's light, held off
//!    her body's colour ([`TwinColours`]) - where the
//!    prototype drew the line in the trim colour and picked one light per
//!    scheme by eye.
//! 2. Her airframe's rigid parts are floored at
//!    [`MIN_DIM`](super::MIN_DIM) where her blueprint draws her smaller
//!    than the prototype: the tail wheel's fork, the pitot and the aerial's
//!    wires, at their sanitiser's floor in prototype metres.

mod dressing;
mod engines;
mod fuselage;
mod gear;
mod tail;
mod wing;

pub(crate) use engines::BLADE_PASS_HZ;
#[cfg(test)]
pub(super) use engines::{PROP_GAIN_DEG_PER_M, PROP_IDLE_DEG_S};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirplaneBlueprint, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    AirplaneCraft, AirplaneFeel, AirplaneLayout, FusePlan, FuseProfile, Handling, TwinColours, dim,
    twin_colours,
};

/// The fuselage, `(z, centreline height, radius)` nose tip to tail post, in
/// the prototype's frame at girth 1 - the prototype's `FUSE_BASE`. Adjacent
/// gaps stay within twice each other (a sweep is a uniform Catmull-Rom,
/// #1616).
const FUSE: &[(f32, f32, f32)] = &[
    (1.550, 0.505, 0.030),
    (1.480, 0.512, 0.100),
    (1.360, 0.522, 0.152),
    (1.200, 0.534, 0.184),
    (1.000, 0.545, 0.204),
    (0.780, 0.550, 0.214),
    (0.420, 0.551, 0.215),
    (0.050, 0.556, 0.204),
    (-0.350, 0.574, 0.172),
    (-0.800, 0.604, 0.128),
    (-1.200, 0.634, 0.084),
    (-1.500, 0.656, 0.046),
];

/// Her fuselage: the stations above, drawn 1.15 times taller than wide; the
/// cabin's run from the cockpit's back wall at `0.78` to the cabin's rear at
/// `0.05`; the airframe from the nose's tip to the fins' trailing edges,
/// their trim caps' - read back off the drawn tree by
/// `the_box_holds_the_drawn_airframe`; and her box over the cabin's deepest
/// section at `0.42`, its floor 22.5 cm up - so that, standing eleven
/// degrees tail-down with her tail running 44 cm past her tail wheel, the
/// box's tail corner clears the ground at her longest cabin.
pub(super) const PROFILE: FuseProfile = FuseProfile {
    stations: FUSE,
    section: 1.15,
    firewall: 0.78,
    cabin_rear: 0.05,
    nose: 1.55,
    tail: tail::TAIL_REACH,
    box_station: 0.42,
    box_floor_rise: 0.225,
    cowl_ring: 0.0,
};

/// The nacelles' (and the main wheels') `x`, the engines' thrust line, the
/// main axles' station and the tyres' outer radius; the tail wheel's
/// station and its tyre's; and the attitude she stands at on all three
/// (prototype frame, degrees).
const NACELLE_X: f32 = 0.68;
const NACELLE_Y: f32 = 0.47;
const MAIN_Z: f32 = 0.88;
const WHEEL_R: f32 = 0.15;
const TAIL_Z: f32 = -1.20;
const TAIL_R: f32 = 0.05;
const GROUND_DEG: f32 = 11.0;

/// The side of every hidden hub cube (m, prototype frame): the root at the
/// box's middle, the airscrews' pivots and the wheels'.
const HUB_CUBE: f32 = 0.02;

/// The station under the cabin a belly aura issues from (prototype frame):
/// behind the wing's spar, ahead of the tail wheel.
const BELLY_FX_Z: f32 = 0.40;

/// The tail wheel's axle height (prototype frame): set so the line from the
/// mains' contact to the tail wheel's stands [`GROUND_DEG`] off level,
/// over the stretched distance between them.
fn tail_wheel_y(plan: &FusePlan) -> f32 {
    (plan.stretch(MAIN_Z) - plan.stretch(TAIL_Z)) * GROUND_DEG.to_radians().tan() + TAIL_R
}

pub(super) struct Twin;

impl AirplaneCraft for Twin {
    fn plan(&self, bp: &AirplaneBlueprint) -> FusePlan {
        FusePlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &FusePlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> AirplaneFeel {
        // #1625 Q-G: a transport - heavier, faster and steadier than the
        // high-wing. She stalls at 13 m/s against the preset's 11, flies
        // level with her nose on her path at 24 against 20 and tops out
        // about 27 against 25, on 0.85 times the drag at the same thrust per
        // kilo; she rolls at 70 degrees a second against 90, pitches at 40
        // against 45 and yaws at 22 against 25. Three tenths heavier for her
        // length: two engines and a cabin.
        AirplaneFeel {
            mass_factor: 1.3,
            handling: Some(Handling {
                stall: 13.0,
                cruise: 24.0,
                drag_factor: 0.85,
                roll_rate: 70.0,
                pitch_rate: 40.0,
                yaw_rate: 22.0,
            }),
        }
    }

    fn layout(&self, plan: &FusePlan) -> AirplaneLayout {
        let half = plan.half_extents();
        let [hx, _, hz] = half;
        // Everything in the drawn frame: her nose at +Z, the box's middle
        // at the origin, so `+z` is AHEAD - the travel pose turns her round.
        let floor = plan.at(0.0, plan.box_floor(), 0.0)[1];
        let mains = plan.at(NACELLE_X, WHEEL_R, MAIN_Z);
        let tail = plan.at(0.0, tail_wheel_y(plan), TAIL_Z);
        let stance = GROUND_DEG.to_radians();
        // Her floor over a wheel's lowest point, plumb: the hub hangs under
        // the floor along her own down, which leans with her, and the
        // tyre's lowest point is a radius under the hub at any pitch.
        let over = |hub: [f32; 3], radius: f32| (floor - hub[1]) * stance.cos() + plan.len(radius);
        // Her centre of mass over her wing's quarter chord at its root,
        // where a straight wing's lift is centred - 13 cm behind her mains.
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
            // An engine's exhaust, steam or sparks leave the port engine's
            // stack - where anything burns on her.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                let (x, y, z) = engines::exhaust_mouth(1.0);
                plan.at(x, y, z)
            }
            // Everything else issues under her cabin, aft of her wing's
            // spar: a wash blows down from the belly, and a flourish rises
            // round her from under it - never into her airscrews' discs.
            _ => {
                let z = BELLY_FX_Z;
                plan.at(0.0, plan.sill(z) - 0.08, z)
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::TwinRadial
    }
}

/// The twin dressed for the tiers `ctx` carries - what [`Twin::build`]
/// draws, and what the guards sweep every tier through.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it; the tier ladder comes
/// after what the prototype drew.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &FusePlan) -> Generator {
    let c = twin_colours(ctx);
    // Root: a hidden hub at the box's middle, buried in the cabin.
    let side = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    fuselage::fuselage(kids, plan, &c);
    fuselage::skins(kids, plan, &c);
    fuselage::windows(kids, plan, &c);
    wing::wing(kids, plan, &c);
    engines::engines(kids, plan, &c);
    gear::mains(kids, plan, &c);
    gear::tail_wheel(kids, plan, &c);
    tail::stabiliser(kids, plan, &c);
    tail::fins(kids, plan, &c);
    wing::lights(kids, plan, &c, ctx.ornateness);
    tail::tail_light(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness);
    root
}

/// A hidden hub cube at `at` - an airscrew's or a wheel's pivot - for the
/// parts it turns, which are laid relative to it.
fn hub(plan: &FusePlan, at: [f32; 3], c: &TwinColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}
