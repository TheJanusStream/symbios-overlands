//! The light utility helicopter: a glazed pod on skids, a two-blade main
//! rotor on a roof mast and a tail rotor on a slim boom - the JetRanger's
//! layout, and the family's hero and universal floor (#1614).
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1613, #1359 rules 1, 12 and 14):
//! `target/dump/aircraft-p2/heli.py` is its Python twin, and this port reads
//! it number for number, in the prototype's own frame ([`PodPlan::at`] maps
//! it to the drawn one). It is drawn at the proportion the owner chose - B,
//! the cabin 22 % fuller and the whole 12 % larger than the first draft -
//! which is the nominal blueprint.
//!
//! [`pod`] draws the cabin and everything laid on it or stood on its roof;
//! [`tail`] the boom, its stripes, the fins, the stabiliser and the lights;
//! [`rotor`] the mast and both rotors; [`skids`] the landing gear; and
//! [`dressing`] the ladder of secondary masses by ornateness.
//!
//! # Where the port leaves the prototype, and why
//!
//! Five small changes, each one either a fleet rule or a guard's:
//!
//! 1. The boom stripe is the seeded ACCENT, where the prototype drew it in
//!    the livery's second hue - the fleet's identity rule (#1365): every
//!    craft carries its owner's colour in a few square inches of trim. And
//!    there is one on EACH flank, where the prototype had one: a second sweep
//!    over the same stations, drawn tail to nose, which mirrors the sweep's
//!    angular frame and so its band.
//! 2. The nav lights are on the aviation sides: red to port (her left,
//!    `+X`), green to starboard. The prototype had them swapped.
//! 3. The mast reaches into the rotor head, where the prototype left 18 mm
//!    of air between them; the tail rotor's gearbox cap reaches into the
//!    fin, where it hung 11 mm off its face; and the beacon sits down into
//!    the fin's top on the fin's own axis, where the prototype balanced it
//!    on a station of its own that the boom stretch drew away. All three
//!    broke `touch::assert_one_machine`. And the tail rotor stands 3 cm
//!    further out and 4 cm lower on the fin, where its blades cut through
//!    the boom and, on a short boom, came within millimetres of the main
//!    rotor's tips (`rotor`'s `TAIL_ROTOR_X`). None reads at play
//!    distance.
//! 4. The door panes are the fleet's lit window - the tertiary's own light -
//!    where the prototype had one fixed amber.
//! 5. The tier ladder is new ([`dressing`]), as #1613 recommendation 7 set
//!    it out: an adorned machine carries a pitot and skid steps, an ornate
//!    one a searchlight and a cargo hook as well.

mod dressing;
mod pod;
mod rotor;
mod skids;
mod tail;

pub(crate) use rotor::BLADE_PASS_HZ;
#[cfg(test)]
pub(super) use rotor::{MAIN_ROTOR_DEG_S, TAIL_ROTOR_DEG_S};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{HelicopterBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    HelicopterColours, HelicopterCraft, HelicopterFeel, HelicopterIdle, PodPlan, PodProfile, dim,
    helicopter_colours,
};

/// The pod, `(z, centreline height, radius)` nose to rear, in the
/// prototype's frame at girth 1 - the prototype's `POD_BASE`.
const POD: &[(f32, f32, f32)] = &[
    (1.20, 0.47, 0.03),
    (1.13, 0.465, 0.13),
    (0.98, 0.49, 0.205),
    (0.74, 0.525, 0.240),
    (0.40, 0.545, 0.248),
    (0.06, 0.555, 0.238),
    (-0.22, 0.575, 0.200),
    (-0.46, 0.615, 0.128),
    (-0.62, 0.645, 0.085),
];

/// Her pod: the stations above, drawn 1.15 times taller than wide; the
/// datum at the cabin's centreline height; the boom leaving the pod at
/// `-0.50` and ending, with the fin on it, at `-1.66`.
///
/// `tail_reach` is the fin's raked top: a `0.16` chord tapered to 55 % and
/// raked 18 degrees aft over a `0.34` span puts its trailing corner
/// `0.17 sin 18 + 0.044 cos 18 = 0.0944` aft of the boom's end - read back
/// off the drawn tree by `the_box_holds_the_drawn_fuselage`. The beacon
/// seated on the fin's top stands inside it.
pub(super) const PROFILE: PodProfile = PodProfile {
    stations: POD,
    section: 1.15,
    datum: 0.55,
    boom_root: -0.50,
    boom_end: -1.66,
    tail_reach: 0.0944,
};

/// The side of every hidden hub cube (m, prototype frame): the root on the
/// datum and the two rotors' pivots.
const HUB_CUBE: f32 = 0.02;

pub(super) struct Utility;

impl HelicopterCraft for Utility {
    fn plan(&self, bp: &HelicopterBlueprint) -> PodPlan {
        PodPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &PodPlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> HelicopterFeel {
        // #1613 recommendation 8, agreed: nimbler than an airship. The
        // zeppelin's cyclic is 8 m/s^2 and its yaw 7; hers are 10 and 9,
        // which tops out at 12.5 m/s (45 km/h) against the airship's 10.
        // The dampings are the preset's own, so she gathers way and settles
        // a turn as the default body does.
        HelicopterFeel {
            mass_factor: 1.0,
            cyclic_accel: 10.0,
            yaw_accel: 9.0,
            linear_damping: 0.8,
            angular_damping: 4.0,
        }
    }

    fn idle(&self) -> HelicopterIdle {
        // #1613 recommendation 8: a 15 degree lean, a light machine tipping
        // her disc where she is going.
        HelicopterIdle { lean_degrees: 15.0 }
    }

    fn overall_width(&self, plan: &PodPlan, _ornateness: OrnatenessTier) -> f32 {
        // The skids, outboard faces: the stabiliser's end plates and their
        // nav lights reach 0.249 at the slimmest girth, the skid tubes 0.365.
        2.0 * plan.len(skids::track(plan) + skids::SKID_R)
    }

    fn height(&self, plan: &PodPlan) -> f32 {
        plan.len(rotor::head_top(plan))
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &PodPlan) -> [f32; 3] {
        match aura {
            // A turbine's exhaust, steam or sparks leave the stack's mouth
            // on her roof - the one place anything burns.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                let (x, y, z) = pod::exhaust_mouth(plan);
                plan.at(x, y, z)
            }
            // Everything else issues under her cabin, between the skids: a
            // thruster's wash blows down from the belly, and a flourish rises
            // round the cabin from under it - never into the rotor over it.
            _ => {
                let z = BELLY_FX_Z;
                plan.at(0.0, plan.sill(z) - 0.08, z)
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Turbine
    }
}

/// The station under the cabin a belly aura issues from (prototype frame):
/// under the mast, between the skids' cross tubes.
const BELLY_FX_Z: f32 = 0.15;

/// The utility helicopter dressed for the tiers `ctx` carries - what
/// [`Utility::build`] draws, and what the guards sweep every tier through.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it; the boom's second
/// stripe and the tier ladder come after what the prototype drew.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &PodPlan) -> Generator {
    let c = helicopter_colours(ctx);
    // Root: a hidden hub on the datum, buried in the cabin.
    let hub = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([hub; 3], c.dark.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    pod::pod(kids, plan, &c);
    pod::skins(kids, plan, &c);
    pod::fairing(kids, plan, &c);
    pod::exhaust(kids, plan, &c);
    tail::boom(kids, plan, &c);
    tail::fins(kids, plan, &c);
    tail::stabiliser(kids, plan, &c);
    tail::beacon(kids, plan, &c);
    rotor::tail_rotor(kids, plan, &c);
    rotor::main_rotor(kids, plan, &c);
    skids::skids(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness);
    root
}

/// A hidden hub cube at `at` - a rotor's pivot - for the parts the rotor
/// turns, which are laid relative to it.
fn hub(plan: &PodPlan, at: [f32; 3], c: &HelicopterColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}
