//! The two rotors: the main rotor on the mast over the engine - two long
//! blades and the stabiliser bar under them at right angles, weighted at
//! its ends - and the tail rotor on the boom's left, its blades on the outer
//! end of a shaft out of the gearbox.
//!
//! Each turns on a hidden hub cube at its centre with a Constant node spin
//! about its shaft, every part laid relative to the hub - the utility's
//! idiom (`utility::rotor`).

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z};
use super::super::{PodPlan, TrainerColours, dim};
use super::PROFILE;
use super::bay::ENGINE_Z;
use super::boom::gear_y;

/// The main rotor's turn (degrees a second): two and a half turns a
/// second, lazier than the utility's three (#1621 Q-G), and two blades make
/// the 5 Hz blade pass her voice slaps at ([`BLADE_PASS_HZ`]).
pub(crate) const MAIN_ROTOR_DEG_S: f32 = 900.0;

/// The main rotor's blades.
const MAIN_ROTOR_BLADES: f32 = 2.0;

/// How many times a second a main-rotor blade passes a point: the rate her
/// voice's slap is gated at (`fx::piston_chop`), so what is heard is what is
/// seen.
pub(crate) const BLADE_PASS_HZ: f32 = MAIN_ROTOR_BLADES * MAIN_ROTOR_DEG_S / 360.0;

/// The tail rotor's: four turns a second, at the sanitiser's ceiling.
pub(crate) const TAIL_ROTOR_DEG_S: f32 = 1440.0;

/// The rotor head's centre over the skid bottoms, and its height
/// (prototype frame). The mast's top stands at a height of its own rather
/// than over the bubble's crown: the mast rises out of the transmission
/// behind the bubble, not off it.
pub(super) const MAST_TOP: f32 = 1.13;
const HEAD_H: f32 = 0.05;

/// The main rotor's blade tip radius, and the stabiliser bar's reach - its
/// weights' outer ends - and how far under the blades it turns: 3 cm, so
/// the head's lower rim sits in it, where the prototype hung it 5 cm under
/// the blades and joined to nothing.
const ROTOR_R: f32 = 1.86;
const BAR_HALF: f32 = 0.50;
const BAR_DROP: f32 = 0.03;
const WEIGHT_R: f32 = 0.022;
const WEIGHT_LEN: f32 = 0.06;

/// The tail rotor's blade length, and where its hub stands: off the
/// gearbox to her right (`-X`), its blades a further 2.5 cm out on the
/// shaft's end: its swept disc clears the longerons by 3.3 cm at the
/// nominal machine, 2.8 at the shortest one on the longest boom
/// (`her_tail_rotor_sweeps_clear_of_the_rest_of_her`), and the stirrup by
/// more.
const TAIL_ROTOR_R: f32 = 0.27;
const TAIL_ROTOR_X: f32 = -0.06;
const TAIL_BLADE_OUT: f32 = -0.025;

/// The top of the rotor head over the skid bottoms (prototype frame) - the
/// top of her box.
pub(super) fn head_top() -> f32 {
    MAST_TOP + HEAD_H * 0.5
}

/// A rotor: `parts` laid relative to a hidden hub at `at`, turning about
/// `axis` at `rate` degrees a second.
fn rotor(
    plan: &PodPlan,
    at: [f32; 3],
    axis: [f32; 3],
    rate: f32,
    parts: Vec<Generator>,
    c: &TrainerColours,
) -> Generator {
    let mut hub = super::hub(plan, at, c);
    hub.children = parts;
    hub.spin = Some(Spin::about(axis, SpinTerm::Constant { rate: Fp(rate) }));
    hub
}

/// The tail rotor: a shaft reaching 5 mm into the gearbox's face, and two
/// blades along `Y` on its outer end, turning about `X`.
pub(super) fn tail_rotor(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let mut parts = vec![prim(
        cylinder(dim(plan.len(0.018)), plan.len(0.06), 10, c.metal.clone()),
        [0.0; 3],
        quat_xyzw(quat_z(FRAC_PI_2)),
    )];
    for side in [-1.0f32, 1.0] {
        parts.push(prim(
            cuboid(
                [
                    dim(plan.len(0.010)),
                    plan.len(TAIL_ROTOR_R),
                    plan.len(0.032),
                ],
                c.dark.clone(),
            ),
            [
                plan.len(TAIL_BLADE_OUT),
                plan.len(side * TAIL_ROTOR_R * 0.5),
                0.0,
            ],
            id_quat(),
        ));
    }
    kids.push(rotor(
        plan,
        plan.at(TAIL_ROTOR_X, gear_y(), PROFILE.boom_end),
        [1.0, 0.0, 0.0],
        TAIL_ROTOR_DEG_S,
        parts,
        c,
    ));
}

/// The main rotor: the head, its yoke and two blades along `X`, each
/// feathered four degrees, and the stabiliser bar under them along `Z`,
/// weighted at its ends - turning about `+Y` on the mast's top.
pub(super) fn main_rotor(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let mut parts = vec![
        prim(
            cylinder(plan.len(0.040), plan.len(HEAD_H), 16, c.metal.clone()),
            [0.0; 3],
            id_quat(),
        ),
        prim(
            cuboid(
                [plan.len(0.15), plan.len(0.024), plan.len(0.045)],
                c.metal.clone(),
            ),
            [0.0; 3],
            id_quat(),
        ),
    ];
    for side in [-1.0f32, 1.0] {
        parts.push(prim(
            cuboid(
                [
                    plan.len(ROTOR_R - 0.06),
                    dim(plan.len(0.012)),
                    plan.len(0.072),
                ],
                c.dark.clone(),
            ),
            [plan.len(side * (ROTOR_R + 0.06) * 0.5), 0.0, 0.0],
            quat_xyzw(quat_x((side * 4.0).to_radians())),
        ));
    }
    // The bar: a rod across under the blades, a weight on each end.
    let across = quat_xyzw(quat_x(FRAC_PI_2));
    parts.push(prim(
        cylinder(
            dim(plan.len(0.008)),
            plan.len(2.0 * BAR_HALF),
            8,
            c.metal.clone(),
        ),
        [0.0, -plan.len(BAR_DROP), 0.0],
        across,
    ));
    for side in [-1.0f32, 1.0] {
        parts.push(prim(
            cylinder(plan.len(WEIGHT_R), plan.len(WEIGHT_LEN), 12, c.dark.clone()),
            [0.0, -plan.len(BAR_DROP), plan.len(side * BAR_HALF)],
            across,
        ));
    }
    kids.push(rotor(
        plan,
        plan.at(0.0, MAST_TOP, ENGINE_Z),
        [0.0, 1.0, 0.0],
        MAIN_ROTOR_DEG_S,
        parts,
        c,
    ));
}
