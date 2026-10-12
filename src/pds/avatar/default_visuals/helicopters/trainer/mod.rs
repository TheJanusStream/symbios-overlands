//! The bubble-canopy trainer: a glazed bubble over an open engine bay, twin
//! saddle tanks strapped high beside the mast, an open truss boom to a small
//! tail rotor with a guard stirrup under it, a two-blade main rotor with its
//! stabiliser bar, and tall skids - the light helicopter of the 1950s, and
//! the fan-out's first type (#1621, umbrella #1618).
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1621, all eight recommendations
//! taken): `target/dump/aircraft-p7-trainer/trainer.py` is its Python twin,
//! written on [`PodPlan`]'s own scale and girth rules, and this port reads it
//! number for number, in the prototype's own frame ([`PodPlan::at`] maps it
//! to the drawn one). It is drawn at the proportion the owner chose - B, the
//! bubble a ninth larger than first drawn about its own floor - which is the
//! profile, at the nominal blueprint's 3.2 m.
//!
//! [`bubble`] draws the bubble and the skins laid on it; [`bay`] the engine
//! bay behind it - its frame, the engine and its stack, the transmission and
//! the mast, the saddle tanks; [`boom`] the truss, its tail and its lights;
//! [`rotor`] both rotors; [`skids`] the landing gear; and [`dressing`] the
//! ladder of secondary masses by ornateness.
//!
//! # Where the port leaves the prototype, and why
//!
//! Five small changes, each one a fleet rule or a guard's:
//!
//! 1. The door hoop round the bubble is the seeded ACCENT, where the
//!    prototype drew it in the scheme's second hue - the fleet's identity
//!    rule (#1365), as the utility's boom stripes are; and it is held clear
//!    of the dark glass it frames rather than of her body
//!    ([`TrainerColours::stripe`]).
//! 2. The skid tubes run on evened stations and their toes end at
//!    `(1.58, 0.11)`, 6 cm aft of and 1 cm under the prototype's
//!    `(1.64, 0.12)`: one long straight run beside the toe's short segments
//!    dipped the tube under the ground ahead of the bend (a Spine is a
//!    uniform Catmull-Rom, #1616). And her skid track grows with her bubble,
//!    as the utility's does with her cabin; the prototype held it at the
//!    nominal machine's.
//! 3. The searchlight is the utility's: aimed forward and 40 degrees DOWN,
//!    its housing half buried in the bubble's floor 10 cm further aft, its
//!    lens the fleet's lit window light and its housing dark - where the
//!    prototype aimed it 20 degrees UP, hung it under the chin, where the
//!    fullest bubble's floor stands off its sill, and lit it white in metal.
//! 4. Every part that was drawn touching another face to face, or not at
//!    all, now overlaps it (`touch::assert_one_machine`): the transmission
//!    sits a centimetre into the engine and the mast rises out of its
//!    middle, where each stood on the other's top face; the stabiliser bar
//!    turns 3 cm under the blades, where the head's rim sits in it, rather
//!    than 5 cm and joined to nothing; the exhaust stack's upper end sits in
//!    the starboard bank rather than 4 cm under it; the skid steps sit on
//!    the front cross tube, 5 cm aft of where the prototype stood them on
//!    its edge; and the bulkhead is a short sweep under the bubble's own
//!    section scale rather than a turned cylinder.
//! 5. Every part the prototype drew under the sanitiser's 1 cm floor is
//!    drawn at [`MIN_DIM`](super::MIN_DIM): the truss's struts (8 mm), the
//!    tank straps (7), the cradles (9), the elevator's end plates (10), the
//!    searchlight's lens (6) and the litter pods' struts (8); and the guard
//!    stirrup at 12.5 mm (10), over the floor at every blueprint, since it
//!    sets her box's tail. And each face's zig-zag runs one corner past each
//!    end and is trimmed back (`boom`), which straightens the first and last
//!    strut of each face - they bowed 4 cm.

mod bay;
mod boom;
mod bubble;
mod dressing;
mod rotor;
mod skids;

pub(crate) use rotor::BLADE_PASS_HZ;
#[cfg(test)]
pub(super) use rotor::{MAIN_ROTOR_DEG_S, TAIL_ROTOR_DEG_S};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{HelicopterBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    HelicopterCraft, HelicopterFeel, HelicopterIdle, PodPlan, PodProfile, TrainerColours, dim,
    trainer_colours,
};

/// The bubble, `(z, centreline height, radius)` nose to its rear bulkhead,
/// in the prototype's frame at girth 1 - the prototype's `BUBBLE_B`, its
/// `BUBBLE_A` grown by a ninth about its own floor (each centreline raised
/// `0.11 r` times the section, each radius `1.11` times).
const BUBBLE: &[(f32, f32, f32)] = &[
    (1.620, 0.5024, 0.0222),
    (1.575, 0.5216, 0.1554),
    (1.490, 0.5405, 0.2387),
    (1.380, 0.5557, 0.2864),
    (1.250, 0.5621, 0.2997),
    (1.110, 0.5607, 0.2864),
    (1.000, 0.5537, 0.2498),
    (0.935, 0.5434, 0.1998),
];

/// Her bubble: the stations above, drawn 1.08 times taller than wide; the
/// truss leaving the engine bay at `0.30` and ending at the tail rotor's
/// gearbox at `-1.58`, so the nose stands 3.2 m ahead of it and the
/// prototype is true metres at the nominal blueprint; and the guard stirrup
/// reaching `0.3178` aft of that - its curve's aftmost sample, `0.3053`, and
/// its tube's radius - read back off the drawn tree by
/// `the_box_holds_the_drawn_fuselage`.
///
/// The datum is where [`PodPlan`] hangs the drawn frame's origin, and so
/// where the root's hidden hub sits: fore and aft at the craft's middle,
/// which falls in the OPEN truss (the utility's falls inside her boom). So
/// it is the upper longeron's height there at the nominal machine, and the
/// hub is small enough to hide inside the tube wherever the boom, and her
/// bubble's measured nose ([`bubble::nose_reach`]), move that middle
/// (`her_root_hub_hides_in_the_upper_longeron`).
pub(super) const PROFILE: PodProfile = PodProfile {
    stations: BUBBLE,
    section: 1.08,
    datum: 0.6271,
    boom_root: 0.30,
    boom_end: -1.58,
    tail_reach: 0.3178,
};

/// The root's hidden hub cube (m, prototype frame): inside the 13 mm upper
/// longeron at any blueprint - its half-diagonal, 8.5 mm.
const ROOT_CUBE: f32 = 0.012;

/// The side of the rotors' hidden pivot cubes (m, prototype frame).
const HUB_CUBE: f32 = 0.02;

/// The station under the bubble a belly aura issues from (prototype frame):
/// between the skids' front cross tube and the chin.
const BELLY_FX_Z: f32 = 1.10;

pub(super) struct Trainer;

impl HelicopterCraft for Trainer {
    fn plan(&self, bp: &HelicopterBlueprint) -> PodPlan {
        // Her blunt nose is drawn past its first station: her box's nose is
        // read off the bubble she draws, so her collider holds her glass.
        let plan = PodPlan::new(bp, PROFILE);
        plan.with_nose_reach(bubble::nose_reach(&plan))
    }

    fn build(&self, ctx: &PartCtx, plan: &PodPlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> HelicopterFeel {
        // #1621 Q-G, agreed: lighter and lazier than the utility. Her cyclic
        // 8.5 against the utility's 10 tops out at about 10.6 m/s against
        // 12.5; her yaw is a little quicker; the dampings are the preset's,
        // as the utility's are.
        HelicopterFeel {
            mass_factor: 0.75,
            cyclic_accel: 8.5,
            yaw_accel: 10.0,
            linear_damping: 0.8,
            angular_damping: 4.0,
        }
    }

    fn idle(&self) -> HelicopterIdle {
        // #1621 Q-G: 18 degrees, three more than the utility's 15 - a light
        // machine swinging under her rotor.
        HelicopterIdle { lean_degrees: 18.0 }
    }

    fn overall_width(&self, plan: &PodPlan, ornateness: OrnatenessTier) -> f32 {
        // The skids' outboard faces - outboard of her bubble at every girth,
        // her track growing with it - or, on an ornate machine, her litter
        // pods' outboard faces, read off the pods she draws: they hang 12 to
        // 14 cm past the skids, and her box holds what she is drawn as.
        if ornateness == OrnatenessTier::Ornate {
            2.0 * dressing::pods_half_width(plan)
        } else {
            2.0 * plan.len(skids::track(plan) + skids::SKID_R)
        }
    }

    fn height(&self, plan: &PodPlan) -> f32 {
        plan.len(rotor::head_top())
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &PodPlan) -> [f32; 3] {
        match aura {
            // An engine's exhaust, steam or sparks leave the stack's mouth
            // under the engine - the one place anything burns.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                let (x, y, z) = bay::exhaust_mouth();
                plan.at(x, y, z)
            }
            // Everything else issues under her bubble, between the skids: a
            // wash blows down from the belly, and a flourish rises round the
            // bubble from under it - never into the rotor over it.
            _ => plan.at(0.0, plan.sill(BELLY_FX_Z) - 0.08, BELLY_FX_Z),
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::PistonRotor
    }
}

/// The trainer dressed for the tiers `ctx` carries - what [`Trainer::build`]
/// draws, and what the guards sweep every tier through.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &PodPlan) -> Generator {
    let c = trainer_colours(ctx);
    // Root: a hidden hub on the datum, inside the upper longeron.
    let side = dim(plan.len(ROOT_CUBE));
    let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    bubble::bubble(kids, plan, &c);
    bubble::skins(kids, plan, &c);
    bubble::bulkhead(kids, plan, &c);
    bay::frame(kids, plan, &c);
    bay::engine(kids, plan, &c);
    bay::tanks(kids, plan, &c);
    boom::truss(kids, plan, &c);
    boom::gearbox(kids, plan, &c);
    rotor::tail_rotor(kids, plan, &c);
    boom::stirrup(kids, plan, &c);
    boom::elevator(kids, plan, &c);
    boom::lights(kids, plan, &c);
    rotor::main_rotor(kids, plan, &c);
    skids::skids(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness);
    root
}

/// A hidden hub cube at `at` - a rotor's pivot - for the parts the rotor
/// turns, which are laid relative to it.
fn hub(plan: &PodPlan, at: [f32; 3], c: &TrainerColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}
