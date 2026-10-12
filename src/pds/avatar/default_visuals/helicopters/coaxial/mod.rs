//! The coaxial: two three-blade rotors turning opposite ways on one mast and
//! no tail rotor, a sleek fuselage tapering from a wrap-round canopy to a
//! pusher in a shroud behind an H-tail, on skids - the near-future compound
//! of the Sikorsky X2 and S-97 line (#1628), and the fan-out's third
//! rotorcraft (#1618), at home on the five speculative and alien themes.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1628, all eight recommendations
//! taken): `target/dump/aircraft-p7-coaxial/coaxial.py` is its Python twin,
//! written on [`PodPlan`]'s own scale and girth rules, and this port reads
//! its defaults - the compound, the shroud, three blades a rotor - number
//! for number, in the prototype's own frame ([`PodPlan::at`] maps it to the
//! drawn one). She is true metres at the nominal blueprint's 3.2 m, nose tip
//! to her boom's end in her shroud, 2 cm aft of her pusher's disc.
//!
//! [`fuselage`] draws the fuselage and what is laid on it - the canopy and
//! the chin windows, the cabin's lit panes and the cheat line - and the roof
//! fairing and its exhausts; [`rotor`] the mast and the two rotors; [`tail`]
//! the H-tail, the pusher, its shroud and stators, and the lights; [`skids`]
//! the landing gear; [`dressing`] the ladder of secondary masses by
//! ornateness; and [`measure`] reads her box off the machine she draws.
//!
//! # Her box is measured
//!
//! Her nose and her tail - the shroud's aft lip - and her width are read off
//! the machine she draws, blades pruned ([`measure::reach`],
//! [`measure::drawn`]), as the autogyro's are: her H-tail and its nav lights
//! stand wider than her skids on a slim cabin, and an ornate machine's stub
//! wings wider still. Her box is as tall as her upper rotor's cap.
//!
//! # The cabin's skids do not stretch
//!
//! The boom stretch starts at the cabin's rear, `0.15` ([`PROFILE`]), and
//! her skids and their rear cross tube run past it: they are drawn rigid off
//! that station ([`cabin`]), so a longer boom carries her tail aft and
//! leaves her skids where the prototype drew them.
//!
//! # Where the port leaves the prototype, and why
//!
//! Three changes, each a fleet rule's or a decision's:
//!
//! 1. The cheat line is the seeded ACCENT (the identity rule #1365) and the
//!    cabin's panes the fleet's lit window - the tertiary's light, held off
//!    her body's colour ([`CoaxialColours`]) - where the prototype drew the
//!    line in the trim colour and picked one light per scheme by eye. Her
//!    schemes' finish is her theme's
//!    [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s, as every
//!    helicopter's is.
//! 2. The pusher gains 63 degrees a second for each metre a second, not the
//!    prototype's 60: so it reaches the spin ceiling at 14.3 m/s, under her
//!    top speed, as the proposal said it would (#1628 Q-G).
//! 3. Her thin parts are floored at [`MIN_DIM`](super::MIN_DIM) where her
//!    blueprint draws her smaller than the prototype: her blades, her
//!    pusher's blades, her steps, her sensor's lens and her lamp pods' ends,
//!    each 1.2 cm in prototype metres.

mod dressing;
mod fuselage;
mod measure;
mod rotor;
mod skids;
mod tail;

pub(crate) use rotor::BLADE_PASS_HZ;
#[cfg(test)]
pub(super) use rotor::{BLADES, ROTOR_DEG_S};
#[cfg(test)]
pub(super) use tail::{PROP_GAIN_DEG_PER_M, PROP_IDLE_DEG_S};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{HelicopterBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    CoaxialColours, HelicopterCraft, HelicopterFeel, HelicopterIdle, PodPlan, PodProfile,
    coaxial_colours, dim,
};

/// The fuselage, `(z, centreline height, radius)` nose tip to the tail
/// cone's end in the stators, in the prototype's frame at girth 1 - the
/// prototype's `FUSE_BASE`. One sweep, the cabin flowing into the tapering
/// tail as a compound's does; adjacent gaps stay within twice each other (a
/// sweep is a uniform Catmull-Rom, #1616).
const FUSE: &[(f32, f32, f32)] = &[
    (1.450, 0.640, 0.020),
    (1.400, 0.632, 0.105),
    (1.300, 0.624, 0.175),
    (1.150, 0.620, 0.222),
    (0.950, 0.622, 0.248),
    (0.700, 0.630, 0.255),
    (0.420, 0.645, 0.245),
    (0.150, 0.665, 0.212),
    (-0.200, 0.695, 0.150),
    (-0.600, 0.720, 0.102),
    (-1.000, 0.738, 0.076),
    (-1.400, 0.752, 0.062),
    (-1.680, 0.760, 0.054),
];

/// Her fuselage: the stations above, drawn 1.20 times taller than wide; the
/// boom stretch aft of the cabin's rear at `0.15`, and her length to her
/// boom's end at `-1.75`, in her shroud 2 cm aft of her pusher's disc, so the
/// prototype is true metres at the nominal blueprint.
///
/// The datum is where [`PodPlan`] hangs the drawn frame's origin, and so
/// where the root's hidden hub sits: fore and aft at her box's middle, which
/// falls in her tail cone a third of a metre aft of the cabin's rear - there
/// the fuselage's centreline stands at `0.713` at the nominal machine, and
/// the hub hides in it wherever the girth and the boom move it. Her nose and
/// tail are measured ([`measure::reach`]), so the profile's reach is zero.
pub(super) const PROFILE: PodProfile = PodProfile {
    stations: FUSE,
    section: 1.20,
    datum: 0.713,
    boom_root: 0.15,
    boom_end: -1.75,
    tail_reach: 0.0,
};

/// The side of every hidden hub cube (m, prototype frame): the root at her
/// box's middle, the two rotors' pivots and the pusher's.
const HUB_CUBE: f32 = 0.02;

/// The station under her cabin a belly aura issues from (prototype frame):
/// between the skids' cross tubes, under the mast.
const BELLY_FX_Z: f32 = 0.45;

pub(super) struct Coaxial;

impl HelicopterCraft for Coaxial {
    fn plan(&self, bp: &HelicopterBlueprint) -> PodPlan {
        // Her box's two ends are read off the machine she draws: her nose's
        // tip and her shroud's aft lip.
        let plan = PodPlan::new(bp, PROFILE);
        let (nose, tail) = measure::reach(&plan);
        plan.with_nose_reach(nose).with_tail_reach(tail)
    }

    fn build(&self, ctx: &PartCtx, plan: &PodPlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> HelicopterFeel {
        // #1628 Q-G, agreed: the fastest and the quickest of the four. Her
        // cyclic 11.5 against the utility's 10 tops out at about 14.4 m/s
        // against 12.5, her pusher shoving her along; her yaw 13 against 9 -
        // with no tail rotor she pirouettes; a tenth heavier, for her second
        // rotor and her pusher. The dampings are the preset's, as every
        // type's are.
        HelicopterFeel {
            mass_factor: 1.1,
            cyclic_accel: 11.5,
            yaw_accel: 13.0,
            linear_damping: 0.8,
            angular_damping: 4.0,
        }
    }

    fn idle(&self) -> HelicopterIdle {
        // #1628 Q-G: ten degrees, against the utility's 15 - her pusher
        // shoves her along, so she flies flatter.
        HelicopterIdle { lean_degrees: 10.0 }
    }

    fn overall_width(&self, plan: &PodPlan, ornateness: OrnatenessTier) -> f32 {
        // Read off the machine she draws at `ornateness`: her skids, or her
        // H-tail's nav lights outboard of them on a slim cabin, or an ornate
        // machine's stub wings outboard of both.
        let drawn = measure::drawn(plan, ornateness);
        drawn.max.x - drawn.min.x
    }

    fn height(&self, plan: &PodPlan) -> f32 {
        plan.len(rotor::head_top(plan))
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &PodPlan) -> [f32; 3] {
        match aura {
            // A turbine's exhaust, steam or sparks leave the port exhaust's
            // mouth on her fairing - where anything burns on her.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                fuselage::exhaust_mouth(plan)
            }
            // Everything else issues under her cabin, between the skids: a
            // wash blows down from the belly, and a flourish rises round her
            // from under it - never into her rotors over it.
            _ => plan.at(0.0, plan.sill(BELLY_FX_Z) - 0.08, BELLY_FX_Z),
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::CoaxialTurbine
    }
}

/// The coaxial dressed for the tiers `ctx` carries - what [`Coaxial::build`]
/// draws, and what the guards sweep every tier through.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &PodPlan) -> Generator {
    let c = coaxial_colours(ctx);
    // Root: a hidden hub at her box's middle, buried in her tail cone.
    let side = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
    draw(&mut root.children, plan, &c, ctx.ornateness);
    root
}

/// Every part she is drawn with.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it.
fn draw(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours, ornateness: OrnatenessTier) {
    fuselage::fuselage(kids, plan, c);
    fuselage::skins(kids, plan, c);
    fuselage::fairing(kids, plan, c);
    fuselage::exhausts(kids, plan, c);
    rotor::mast(kids, plan, c);
    rotor::rotors(kids, plan, c);
    tail::h_tail(kids, plan, c);
    tail::pusher(kids, plan, c);
    tail::shroud(kids, plan, c);
    tail::lights(kids, plan, c);
    skids::skids(kids, plan, c);
    dressing::dress(kids, plan, c, ornateness);
}

/// A hidden hub cube at `at` - a rotor's or the pusher's pivot - for the
/// parts it turns, which are laid relative to it.
fn hub(plan: &PodPlan, at: [f32; 3], c: &CoaxialColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}

/// A prototype point drawn RIGID off the cabin's rear, where the boom
/// stretch starts: the same as [`PodPlan::at`] ahead of it, and unstretched
/// aft of it - what her skids and their rear cross tube stand on.
fn cabin(plan: &PodPlan, x: f32, y: f32, z: f32) -> [f32; 3] {
    let root = PROFILE.boom_root;
    plan.at_off(x, y, root, z - root)
}
