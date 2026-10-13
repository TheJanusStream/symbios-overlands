//! The tandem-rotor transport: a long box cabin under two big three-blade
//! rotors turning opposite ways - one over her cockpit, one on a tall aft
//! pylon with her two engines on its flanks - on four wheels in low
//! sponsons: the heavy lifter's layout of the CH-47 Chinook (#1630), and the
//! fan-out's fourth rotorcraft (#1618), at home on the works, the north and
//! the wasteland.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1630, all eight recommendations
//! taken): `target/dump/aircraft-p7-tandem/tandem.py` is its Python twin,
//! written on [`PodPlan`]'s own scale and girth rules, and this port reads
//! its defaults - the Chinook's layout, three blades a rotor, the full
//! rotors - number for number, in the prototype's own frame ([`PodPlan::at`]
//! maps it to the drawn one). She is true metres at the nominal blueprint's
//! 3.2 m, nose tip to the prototype's ramp's end under her aft pylon.
//!
//! [`cabin`] draws the cabin and the nose and what is laid on them - the
//! windscreen and the chin windows, the cabin's lit windows and the cheat
//! line; [`gear`] the sponsons, the wheels and the nav lights on the
//! sponsons' noses; [`pylons`] the hump and the mast over her cockpit, the
//! aft pylon with its engines and its mast, and the tail light; [`rotor`]
//! the two rotors; [`dressing`] the ladder of secondary masses by
//! ornateness; and [`measure`] reads her box off the machine she draws.
//!
//! # One profile, a cabin and a nose
//!
//! Her pod ([`PROFILE`]) is her NOSE, a sweep from its tip into the cabin's
//! front, whose last two stations are the cabin's section. The cabin is a
//! superellipsoid, a rounded box, which a sweep's round section cannot
//! draw: that section carried aft unchanged to her ramp ([`cabin_section`]).
//! The girth swells both by [`PodPlan`]'s one rule, so the nose meets the
//! cabin at every girth. The boom stretch starts aft of the front rotor, at
//! `0.70`: a longer boom lengthens the cabin between her rotors and carries
//! her aft pylon, everything on it and the rear rotor with it.
//!
//! # Her box is measured
//!
//! Her nose - the nose's tip - and her tail - the white light on her aft
//! pylon's tail - and her width are read off the machine she draws, blades
//! pruned ([`measure::reach`], [`measure::drawn`]), as the coaxial's are.
//! Her tail light stands 5 to 13 cm short of the station her length runs to,
//! so her tail reach is negative and her box a little shorter than her
//! blueprint's length. An adorned machine's rescue hoist stands out over her
//! starboard door alone, and her box, which is symmetric, is as wide as it
//! on both sides. Her box is as tall as her rear rotor's cap.
//!
//! An ornate machine's refuelling probe reaches 35 cm out past her nose and
//! stands outside her box on purpose, as the utility's pitot does past
//! hers: a thin rod, like a rotor's blades, that a collider would only make
//! clumsy.
//!
//! # Where the port leaves the prototype, and why
//!
//! Four changes: two a fleet rule's, and two fixes to the agreed prototype
//! itself, which `tandem.py` now carries too (marked `PORT FIX`):
//!
//! 1. The cheat line is the seeded ACCENT (the identity rule #1365) and the
//!    cabin's windows the fleet's lit window - the tertiary's light, held
//!    off her body's colour ([`TandemColours`]) - where the prototype drew
//!    the line in the trim colour and picked one light per scheme by eye.
//!    Her schemes' finish is her theme's
//!    [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s, as every
//!    helicopter's is.
//! 2. Her thin parts are floored at [`MIN_DIM`](super::MIN_DIM) where her
//!    blueprint draws her smaller than the prototype: her cheat line, her
//!    step and her hoist's cable, 1.2, 1.2 and 1.1 cm in prototype metres.
//! 3. An ornate machine's beacon stands on her aft pylon's back, at the
//!    height the prototype drew it ([`dressing`]): where the prototype had
//!    it, 0.55 of the pylon's half-length aft of its middle, it stood 10 cm
//!    inside the pylon, and none of it showed on the agreed sheet.
//! 4. Her engines carry no intake discs. The prototype drew a dark disc in
//!    each engine pod 4 cm behind its nose, of a 4.1 cm radius where the
//!    pod's is 4.5 to 5.2 cm: wholly inside it, so it showed on no sheet,
//!    and dropping it changes nothing the owner saw.

mod cabin;
mod dressing;
mod gear;
mod measure;
mod pylons;
mod rotor;

#[cfg(test)]
pub(super) use rotor::{BLADE_PASS_HZ, BLADES, ROTOR_DEG_S};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{HelicopterBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{
    HelicopterCraft, HelicopterFeel, HelicopterIdle, PodPlan, PodProfile, TandemColours, dim,
    tandem_colours,
};

/// Her nose, `(z, centreline height, radius)` tip to inside the cabin's
/// front, in the prototype's frame at girth 1 - the prototype's
/// `NOSE_BASE`. Its last two stations are as wide and as tall as the cabin,
/// whose section they are ([`cabin_section`]).
const NOSE: &[(f32, f32, f32)] = &[
    (1.600, 0.380, 0.020),
    (1.560, 0.385, 0.095),
    (1.480, 0.392, 0.150),
    (1.360, 0.400, 0.188),
    (1.200, 0.410, 0.210),
    (1.020, 0.420, 0.220),
    (0.860, 0.420, 0.220),
];

/// Her pod: the nose above, drawn 1.15 times taller than wide, which the
/// cabin carries aft; the boom stretch aft of the front rotor at `0.70`, and
/// her length to the prototype's ramp's end at `-1.60`, under her aft pylon,
/// so the prototype is true metres at the nominal blueprint.
///
/// The datum is where [`PodPlan`] hangs the drawn frame's origin, and so
/// where the root's hidden hub sits: fore and aft at her box's middle, which
/// falls in her cabin a little ahead of its middle - there her cabin's
/// centreline stands at `0.448` at the nominal machine's girth, and the hub
/// hides deep in the cabin wherever the girth moves it. Her nose and tail
/// are measured ([`measure::reach`]), so the profile's reach is zero.
pub(super) const PROFILE: PodProfile = PodProfile {
    stations: NOSE,
    section: 1.15,
    datum: 0.448,
    boom_root: 0.70,
    boom_end: -1.60,
    tail_reach: 0.0,
};

/// The cabin's front and rear (prototype frame): the front fixed, inside
/// the nose's last stations; the rear on a stretched station, at her ramp.
pub(super) const CABIN_FRONT: f32 = 0.95;
pub(super) const CABIN_REAR: f32 = -1.25;

/// The side of every hidden hub cube (m, prototype frame): the root at her
/// box's middle and the two rotors' pivots.
const HUB_CUBE: f32 = 0.02;

/// The station under her cabin a belly aura issues from (prototype frame):
/// the sponsons' middle, between her wheels.
const BELLY_FX_Z: f32 = -0.10;

pub(super) struct Tandem;

impl HelicopterCraft for Tandem {
    fn plan(&self, bp: &HelicopterBlueprint) -> PodPlan {
        // Her box's two ends are read off the machine she draws: her nose's
        // tip and her tail light.
        let plan = PodPlan::new(bp, PROFILE);
        let (nose, tail) = measure::reach(&plan);
        plan.with_nose_reach(nose).with_tail_reach(tail)
    }

    fn build(&self, ctx: &PartCtx, plan: &PodPlan) -> Generator {
        build_dressed(ctx, plan)
    }

    fn feel(&self) -> HelicopterFeel {
        // #1630 Q-G, agreed: the heaviest and the steadiest of the five. Her
        // cyclic 7.5 against the utility's 10 tops out at about 9.4 m/s
        // against 12.5; her yaw 7 against 9 - a tandem turns by tilting her
        // two rotors against each other, slowly; and 1.8 times as heavy. The
        // dampings are the preset's, as every type's are.
        HelicopterFeel {
            mass_factor: 1.8,
            cyclic_accel: 7.5,
            yaw_accel: 7.0,
            linear_damping: 0.8,
            angular_damping: 4.0,
        }
    }

    fn idle(&self) -> HelicopterIdle {
        // #1630 Q-G: six degrees, against the utility's 15 - she flies flat.
        HelicopterIdle { lean_degrees: 6.0 }
    }

    fn overall_width(&self, plan: &PodPlan, ornateness: OrnatenessTier) -> f32 {
        // Read off the machine she draws at `ornateness`: her sponsons, or an
        // adorned machine's hoist over her starboard door - on that side
        // alone, so her box, which is symmetric, is twice its reach.
        let drawn = measure::drawn(plan, ornateness);
        2.0 * drawn.max.x.max(-drawn.min.x)
    }

    fn height(&self, plan: &PodPlan) -> f32 {
        plan.len(rotor::head_top(plan))
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &PodPlan) -> [f32; 3] {
        match aura {
            // A turbine's exhaust, steam or sparks leave her port engine's
            // tail - where anything burns on her.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                pylons::exhaust_mouth(plan)
            }
            // Everything else issues under her cabin, between her sponsons:
            // a wash blows down from the belly, and a flourish rises round
            // her from under it - never into her rotors over it.
            _ => plan.at(0.0, plan.sill(BELLY_FX_Z) - 0.08, BELLY_FX_Z),
        }
    }

    fn propulsion(&self) -> Propulsion {
        // #1630 Q-H: the coaxial's double chop. Her two three-blade rotors
        // turn opposite ways at the coaxial's three turns a second, so their
        // blades cross as the coaxial's do and slap in pairs at 9 Hz.
        Propulsion::CoaxialTurbine
    }
}

/// The tandem dressed for the tiers `ctx` carries - what [`Tandem::build`]
/// draws, and what the guards sweep every tier through.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &PodPlan) -> Generator {
    let c = tandem_colours(ctx);
    // Root: a hidden hub at her box's middle, buried in her cabin.
    let side = dim(plan.len(HUB_CUBE));
    let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
    draw(&mut root.children, plan, &c, ctx.ornateness);
    root
}

/// Every part she is drawn with.
///
/// The order is the prototype's, part for part, which is what lets a dump
/// of this tree be checked node by node against it.
fn draw(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours, ornateness: OrnatenessTier) {
    cabin::cabin(kids, plan, c);
    cabin::nose(kids, plan, c);
    cabin::skins(kids, plan, c);
    cabin::windows(kids, plan, c);
    cabin::cheat_line(kids, plan, c);
    gear::sponsons(kids, plan, c);
    gear::wheels(kids, plan, c);
    pylons::front(kids, plan, c);
    pylons::aft(kids, plan, c);
    rotor::rotors(kids, plan, c);
    gear::nav_lights(kids, plan, c);
    pylons::tail_light(kids, plan, c);
    dressing::dress(kids, plan, c, ornateness);
}

/// A hidden hub cube at `at` - a rotor's pivot - for the parts it turns,
/// which are laid relative to it.
fn hub(plan: &PodPlan, at: [f32; 3], c: &TandemColours) -> Generator {
    let side = dim(plan.len(HUB_CUBE));
    prim(cuboid([side; 3], c.dark.clone()), at, id_quat())
}

/// The cabin's `(centreline y, half-width, half-height)` (prototype frame,
/// at her girth): the nose's last section, carried aft unchanged -
/// [`PodPlan::pod_at`] holds it past the last station.
pub(super) fn cabin_section(plan: &PodPlan) -> (f32, f32, f32) {
    plan.pod_at(CABIN_REAR)
}

/// A prototype point at a station ALREADY stretched (prototype frame): the
/// middle of a part whose two ends stretch differently - the cabin's, the
/// cheat line's.
fn at_drawn(plan: &PodPlan, x: f32, y: f32, stretched: f32) -> [f32; 3] {
    plan.at(x, y, plan.unstretch(stretched))
}
