//! The autogyro: a slim fuselage from a radial's crankcase to the tail post,
//! two open cockpits in tandem, the radial in an open cowl ring turning a
//! two-blade wooden airscrew, a free three-blade rotor on a tripod pylon over
//! the front cockpit, a tailplane with upturned end plates, wide main gear on
//! V legs and tall shock legs, and a tail wheel she stands tail-down on - the
//! Cierva C.30's layout (#1624), and the fan-out's second rotorcraft
//! (#1618), at home on the old worlds and in the boiler age.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1624, all nine recommendations
//! taken): `target/dump/aircraft-p7-autogyro/autogyro.py` is its Python
//! twin, written on [`PodPlan`]'s own scale and girth rules, and this port
//! reads its defaults - the tractor, standing tail-down, three blades, the
//! cowl ring - number for number, in the prototype's own frame
//! ([`PodPlan::at`] maps it to the drawn one). She is true metres at the
//! nominal blueprint, crankcase nose to tail post.
//!
//! [`fuselage`] draws the fuselage and the skins laid on it - the cheat line
//! and the two cockpits; [`engine`] the radial in its cowl ring, the exhaust
//! stack and the airscrew; [`rotor`] the rotor's housing, the pylon and the
//! rotor; [`gear`] the mains and the tail wheel; [`tail`] the stabiliser,
//! its end plates, the fin and the lights; [`dressing`] the ladder of
//! secondary masses by ornateness; and [`stance`] stands her on her three
//! wheels and measures the box she stands in.
//!
//! # She stands tail-down in a level box
//!
//! The Helicopter preset rests a LEVEL box on its floor (#1612), and she
//! sits on her tail wheel as a C.30 does (#1624 Q-B): so she is drawn
//! standing, every part turned twelve degrees nose-up about her main axle
//! ([`stance::stand`]) - her mains' tyres on the box's floor and her tail
//! wheel's beside them - and her lean levels her flying forward. Turned,
//! her ends are not where her stations put them, so her box's nose and tail,
//! and with them her centring, are measured off the machine she draws,
//! standing ([`stance::reach`]), as the trainer's blunt nose is measured
//! off her bubble; and so is her width, which her tailplane's nav lights
//! set, or an ornate machine's spats.
//!
//! # Where the port leaves the prototype, and why
//!
//! Five changes, each a fleet rule's, a guard's or an owner decision's:
//!
//! 1. The cheat line is the seeded ACCENT (the identity rule #1365) and the
//!    cockpit wells the fleet's lit window - the tertiary's light, held off
//!    her fuselage's colour - as the biplane's are
//!    ([`super::AutogyroColours`]), where the prototype drew the line in the
//!    trim colour and picked one light per scheme by eye.
//! 2. The airscrew gains 76 degrees a second for each metre a second, not
//!    the biplane's 55 the prototype drew (#1624 Q-I): so it reaches the
//!    spin ceiling, and her voice's thrum, at her own top speed.
//! 3. The tail wheel's axle stands 2.2 mm lower: the prototype set it so the
//!    line from the mains' contact to the tail tyre's stood twelve degrees
//!    off level BEFORE the turn, which left the tail tyre that far off the
//!    ground after it. Here it stands on the ground ([`gear::tail_wheel_y`]).
//! 4. Her mains keep their turn: the stance turns every other part about
//!    her main axle, but a wheel turned about its own axle is the same
//!    wheel, and unturned each main tyre's lowest vertex is on the ground,
//!    where turned it stood up to 0.9 mm off it.
//! 5. The root's hidden hub stands in her fuselage at her box's middle, as
//!    she stands ([`PROFILE`]'s datum), where the prototype hung it on the
//!    LEVEL fuselage's centreline before the turn: 2 cm under the crown.

mod dressing;
mod engine;
mod fuselage;
mod gear;
mod rotor;
mod stance;
mod tail;

#[cfg(test)]
pub(super) use engine::{PROP_GAIN_DEG_PER_M, PROP_IDLE_DEG_S};
#[cfg(test)]
pub(super) use gear::{TAIL_STEER_GAIN, TAIL_STEER_LIMIT};
#[cfg(test)]
pub(super) use rotor::{BLADES, ROTOR_GAIN_DEG_PER_M, ROTOR_IDLE_DEG_S};
#[cfg(test)]
pub(super) use stance::{GROUND_DEG, draw_level};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{HelicopterBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    AutogyroColours, HelicopterCraft, HelicopterFeel, HelicopterIdle, PodPlan, PodProfile,
    autogyro_colours, dim,
};

/// The fuselage, `(z, centreline height, radius)` crankcase nose to tail
/// post, in the prototype's frame at girth 1 - the prototype's `FUSE_BASE`.
/// The nose is the radial's crankcase inside its cowl ring; the fuselage
/// proper starts at the firewall, `1.12`. It sits high on her long gear, as
/// a C.30's did, so that standing tail-down her tail clears her tail wheel;
/// adjacent gaps stay within twice each other (a sweep is a uniform
/// Catmull-Rom, #1616).
const FUSE: &[(f32, f32, f32)] = &[
    (1.450, 0.720, 0.074),
    (1.380, 0.720, 0.123),
    (1.270, 0.720, 0.146),
    (1.120, 0.720, 0.152),
    (0.900, 0.722, 0.153),
    (0.600, 0.726, 0.149),
    (0.250, 0.732, 0.138),
    (-0.200, 0.742, 0.112),
    (-0.700, 0.755, 0.084),
    (-1.250, 0.768, 0.056),
    (-1.750, 0.780, 0.034),
];

/// Her fuselage: the stations above, drawn 1.30 times taller than wide; the
/// boom stretch aft of `-0.30` and her length to the tail post at `-1.75`,
/// so the prototype is true metres at the nominal blueprint.
///
/// The datum is where [`PodPlan`] hangs the drawn frame's origin, and so
/// where the root's hidden hub sits: fore and aft at her box's middle,
/// which standing tail-down falls in her fuselage about 1.2 m behind her
/// mains (1.0 to 1.3 over her blueprints), where it has dropped 23 cm with
/// the turn. So it is the standing
/// centreline's height there at the nominal machine, and the hub hides in
/// the fuselage wherever the girth and the boom move it - within 2.2 cm of
/// that centreline at every corner (`her_root_hub_hides_in_her_fuselage`).
/// Her tail reach is measured as she stands ([`stance::reach`]), so the
/// profile's is zero.
pub(super) const PROFILE: PodProfile = PodProfile {
    stations: FUSE,
    section: 1.30,
    datum: 0.524,
    boom_root: -0.30,
    boom_end: -1.75,
    tail_reach: 0.0,
};

/// The main axle's station, each main wheel's `x` and its tyre's outer
/// radius; the tail wheel's station and its tyre's (prototype frame).
const MAIN_Z: f32 = 0.98;
const TRACK: f32 = 0.50;
const WHEEL_R: f32 = 0.15;
const TAIL_Z: f32 = -1.62;
const TAIL_R: f32 = 0.05;

/// The rotor head's station: over the front cockpit (prototype frame).
const PYLON_Z: f32 = 0.84;

/// Where the front pylon legs' feet and the shock legs' tops are read off
/// the fuselage (prototype frame): the crown's shoulders at the firewall,
/// and the flank just aft of it.
const PYLON_FOOT_Z: f32 = 1.10;
const SHOCK_TOP_Z: f32 = 1.06;

/// The side of every hidden hub cube (m, prototype frame): the root at her
/// box's middle, the airscrew's pivot, the rotor's and the wheels'.
const HUB_CUBE: f32 = 0.02;

/// The station between her cockpits a belly aura issues from (prototype
/// frame): behind the mains' legs, ahead of the tail wheel.
const BELLY_FX_Z: f32 = 0.62;

pub(super) struct Autogyro;

impl HelicopterCraft for Autogyro {
    fn plan(&self, bp: &HelicopterBlueprint) -> PodPlan {
        // Her box's two ends are read off the machine she draws, standing:
        // turned about her main axle, her hubcap and her tail post are not
        // where her stations put them.
        let plan = PodPlan::new(bp, PROFILE);
        let (nose, tail) = stance::reach(&plan);
        plan.with_nose_reach(nose).with_tail_reach(tail)
    }

    fn build(&self, ctx: &PartCtx, plan: &PodPlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> HelicopterFeel {
        // #1624 Q-H, agreed: the Helicopter preset, as every helicopter type
        // flies it - she hovers, which a real autogyro cannot: the game's
        // licence. Her cyclic 9.5 tops out at about 11.9 m/s, between the
        // trainer's 10.6 and the utility's 12.5; her yaw is a slower rudder
        // than either; the dampings are the preset's, as theirs are.
        HelicopterFeel {
            mass_factor: 0.85,
            cyclic_accel: 9.5,
            yaw_accel: 8.0,
            linear_damping: 0.8,
            angular_damping: 4.0,
        }
    }

    fn idle(&self) -> HelicopterIdle {
        // #1624 Q-H: twelve degrees, her stance's own - nose down flying
        // forward, it levels her tail-down stance.
        HelicopterIdle { lean_degrees: 12.0 }
    }

    fn overall_width(&self, plan: &PodPlan, ornateness: OrnatenessTier) -> f32 {
        // Read off the machine she draws at `ornateness`: her tailplane's
        // nav lights stand a little outboard of her tyres, and an ornate
        // machine's spats outboard of both.
        let drawn = stance::drawn(plan, ornateness);
        drawn.max.x - drawn.min.x
    }

    fn height(&self, plan: &PodPlan) -> f32 {
        rotor::head_top(plan) + plan.datum_height()
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &PodPlan) -> [f32; 3] {
        let level = match aura {
            // An engine's exhaust, steam or sparks leave the stack's mouth
            // under her cowl - the one place anything burns.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                let (x, y, z) = engine::exhaust_mouth(plan);
                plan.at(x, y, z)
            }
            // Everything else issues under her, between her cockpits and
            // between her mains and her tail wheel: a wash blows down from
            // the belly, and a flourish rises round her from under it -
            // never into her airscrew's disc or her rotor's.
            _ => plan.at(0.0, plan.sill(BELLY_FX_Z) - 0.08, BELLY_FX_Z),
        };
        // Where that point stands, with her.
        stance::stood(plan, level)
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Radial
    }
}

/// The autogyro dressed for the tiers `ctx` carries - what
/// [`Autogyro::build`] draws, and what the guards sweep every tier through:
/// drawn level, then stood on her three wheels.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &PodPlan) -> Generator {
    let c = autogyro_colours(ctx);
    // Root: a hidden hub at her box's middle, buried in her fuselage.
    let side = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
    draw(&mut root.children, plan, &c, ctx.ornateness);
    stance::stand(&mut root.children, plan);
    root
}

/// Every part she is drawn with, LEVEL - before her stance turns her.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it; the tier ladder comes
/// after what the prototype drew.
fn draw(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    c: &AutogyroColours,
    ornateness: OrnatenessTier,
) {
    fuselage::fuselage(kids, plan, c);
    fuselage::skins(kids, plan, c);
    fuselage::cockpits(kids, plan, c);
    engine::radial(kids, plan, c);
    engine::exhaust(kids, plan, c);
    engine::airscrew(kids, plan, c);
    rotor::housing(kids, plan, c);
    rotor::pylon(kids, plan, c);
    rotor::rotor(kids, plan, c);
    gear::mains(kids, plan, c);
    gear::tail_wheel(kids, plan, c);
    tail::stabiliser(kids, plan, c);
    tail::end_plates(kids, plan, c);
    tail::fin(kids, plan, c);
    tail::lights(kids, plan, c);
    dressing::dress(kids, plan, c, ornateness);
}

/// A hidden hub cube at `at` - an airscrew's, a rotor's or a wheel's pivot -
/// for the parts it turns, which are laid relative to it.
fn hub(plan: &PodPlan, at: [f32; 3], c: &AutogyroColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}
