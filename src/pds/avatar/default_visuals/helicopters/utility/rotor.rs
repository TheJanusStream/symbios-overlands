//! The two rotors: the mast and the main rotor on the roof, the tail rotor
//! on the fin's right-hand face.
//!
//! Each rotor turns on a hidden hub cube at its centre, with a Constant node
//! [`Spin`] about its shaft (#1613 recommendation 3: always turning). A
//! spin turns the node's children about the node's own origin and rebases
//! nothing, so every part is laid relative to its hub and the hub stands
//! unturned - the skiffs' road-wheel idiom (#1604).

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z};
use super::super::{HelicopterColours, PodPlan, dim};
use super::tail::FIN;

/// The main rotor's turn (degrees a second): three turns a second, and two
/// blades make the 6 Hz blade pass her voice chops at ([`BLADE_PASS_HZ`]).
pub(crate) const MAIN_ROTOR_DEG_S: f32 = 1080.0;

/// The main rotor's blades.
const MAIN_ROTOR_BLADES: f32 = 2.0;

/// How many times a second a main-rotor blade passes a point: the rate her
/// voice's chop is gated at (`fx::turbine_chop`), so what is heard is what
/// is seen.
pub(crate) const BLADE_PASS_HZ: f32 = MAIN_ROTOR_BLADES * MAIN_ROTOR_DEG_S / 360.0;

/// The tail rotor's: four turns a second, at the sanitiser's ceiling - the
/// fastest a turn may strobe at 60 frames a second.
pub(crate) const TAIL_ROTOR_DEG_S: f32 = 1440.0;

/// The mast's station, how far the rotor head stands over the crown there,
/// and the head's height.
const MAST_Z: f32 = 0.05;
const MAST_RISE: f32 = 0.20;
const HEAD_H: f32 = 0.045;

/// The main rotor's hub height over the skid bottoms (prototype frame).
pub(super) fn mast_top(plan: &PodPlan) -> f32 {
    plan.crown(MAST_Z) + MAST_RISE
}

/// The top of the rotor head over the skid bottoms (prototype frame) - the
/// top of her box.
pub(super) fn head_top(plan: &PodPlan) -> f32 {
    mast_top(plan) + HEAD_H * 0.5
}

/// A rotor: `parts` laid relative to a hidden hub at `at`, turning about
/// `axis` at `rate` degrees a second.
fn rotor(
    plan: &PodPlan,
    at: [f32; 3],
    axis: [f32; 3],
    rate: f32,
    parts: Vec<Generator>,
    c: &HelicopterColours,
) -> Generator {
    let mut hub = super::hub(plan, at, c);
    hub.children = parts;
    hub.spin = Some(Spin::about(axis, SpinTerm::Constant { rate: Fp(rate) }));
    hub
}

/// The mast, from inside the roof fairing up into the rotor head, and the
/// main rotor on it: the head, its yoke and two blades, turning about `+Y`.
pub(super) fn main_rotor(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let (foot, top) = (plan.crown(MAST_Z) - 0.04, mast_top(plan));
    kids.push(prim(
        cylinder(
            dim(plan.len(0.024)),
            plan.len(top - foot),
            14,
            c.metal.clone(),
        ),
        plan.at(0.0, (foot + top) * 0.5, MAST_Z),
        id_quat(),
    ));
    let mut parts = vec![
        prim(
            cylinder(dim(plan.len(0.045)), plan.len(HEAD_H), 16, c.metal.clone()),
            [0.0; 3],
            id_quat(),
        ),
        prim(
            cuboid(
                [plan.len(0.16), plan.len(0.025), plan.len(0.05)],
                c.metal.clone(),
            ),
            [0.0; 3],
            id_quat(),
        ),
    ];
    // Two blades along X ([`MAIN_ROTOR_BLADES`]), each feathered four
    // degrees.
    for side in [-1.0f32, 1.0] {
        parts.push(prim(
            cuboid(
                [plan.len(1.48), dim(plan.len(0.012)), plan.len(0.075)],
                c.dark.clone(),
            ),
            [plan.len(side * 0.77), 0.0, 0.0],
            quat_xyzw(quat_x((side * 4.0).to_radians())),
        ));
    }
    kids.push(rotor(
        plan,
        plan.at(0.0, top, MAST_Z),
        [0.0, 1.0, 0.0],
        MAIN_ROTOR_DEG_S,
        parts,
        c,
    ));
}

/// Where the tail rotor's hub stands: off the fin's centreline to her right
/// (`-X`), and a little under the fin's middle.
///
/// Both moved from the prototype's `(-0.035, 0.86)`, and both for a guard.
/// OUTBOARD, because there the lower blade cut a centimetre into the boom on
/// every half turn: the disc reaches forward to where the boom has widened
/// to about `0.049`, and its stripe stands `0.003` proud of that, so the
/// blades' inner face has to stand outboard of `0.052` - at `-0.065` it is
/// `0.0595` (`her_tail_rotor_sweeps_clear_of_her_boom`). LOWER, because at
/// the shortest boom the disc's top rose over the main rotor's plane and
/// passed within 4 mm of its blade tips, and through them at the slimmest
/// cabin; at `0.82` the two discs clear at every blueprint corner
/// (`her_two_rotors_never_cross`). Neither move reads at play distance,
/// and a tail rotor near the boom's height is the JetRanger's own.
const TAIL_ROTOR_X: f32 = -0.065;
const TAIL_ROTOR_Y: f32 = 0.82;

/// The tail rotor's gearbox cap: its length along the shaft, and how far
/// its centre stands inboard of the hub - so it runs from a centimetre
/// outboard of the blades into the fin's face.
const TAIL_CAP_LEN: f32 = 0.08;
const TAIL_CAP_IN: f32 = 0.025;

/// The tail rotor on the fin's right-hand face: a gearbox cap along its
/// shaft and two blades along `Y`, turning about `X`.
pub(super) fn tail_rotor(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let (z, _) = FIN;
    // The cap runs from outboard of the blades into the fin, so the rotor is
    // carried by a shaft rather than hung in the air beside it.
    let mut parts = vec![prim(
        cylinder(
            dim(plan.len(0.022)),
            plan.len(TAIL_CAP_LEN),
            12,
            c.metal.clone(),
        ),
        [plan.len(TAIL_CAP_IN), 0.0, 0.0],
        quat_xyzw(quat_z(FRAC_PI_2)),
    )];
    for side in [-1.0f32, 1.0] {
        parts.push(prim(
            cuboid(
                [dim(plan.len(0.011)), plan.len(0.26), plan.len(0.035)],
                c.dark.clone(),
            ),
            [0.0, plan.len(side * 0.13), 0.0],
            id_quat(),
        ));
    }
    kids.push(rotor(
        plan,
        plan.at(TAIL_ROTOR_X, TAIL_ROTOR_Y, z),
        [1.0, 0.0, 0.0],
        TAIL_ROTOR_DEG_S,
        parts,
        c,
    ));
}
