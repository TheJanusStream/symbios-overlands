//! The mast and the two rotors (#1628 Q-C): three blades each, the upper
//! [`ROTOR_GAP`] over the lower, turning the other way at the same rate - a
//! coaxial's whole point, the torque cancelled with no tail rotor to pay for
//! it.
//!
//! Each rotor turns on a hidden hub cube at its head, with a Constant node
//! [`Spin`] about the mast (#1613 recommendation 3: always turning). A spin
//! turns the node's children about the node's own origin and rebases
//! nothing, so every part is laid relative to its hub and the hub stands
//! unturned.

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_xyzw, quat_y, quat_z,
};
use super::super::{CoaxialColours, PodPlan, dim};
use super::fuselage::{FAIRING_SINK, FAIRING_SQUASH};

/// Each rotor's turn (degrees a second), the lower one way and the upper the
/// other: three turns a second, the utility's.
pub(crate) const ROTOR_DEG_S: f32 = 1080.0;

/// Each rotor's blades.
pub(crate) const BLADES: u32 = 3;

/// How many times a second a blade of either rotor passes a point: what her
/// voice's double chop gallops at (`fx::coaxial_chop`), so what is heard is
/// what is seen.
pub(crate) const BLADE_PASS_HZ: f32 = BLADES as f32 * ROTOR_DEG_S / 360.0;

/// The mast's station (prototype frame): over the cabin's middle.
const MAST_Z: f32 = 0.52;

/// The fairing's radius at the mast - its station at `0.50` - which with its
/// squash sets the fairing's top the mast rises out of (prototype frame).
const FAIRING_TOP_R: f32 = 0.155;

/// The lower rotor's head over the fairing's top, and the upper's over the
/// lower's (prototype frame).
const LOWER_RISE: f32 = 0.07;
const ROTOR_GAP: f32 = 0.30;

/// The mast's radius, and how far down into the fairing it stands
/// (prototype frame).
const MAST_R: f32 = 0.030;
const MAST_FOOT: f32 = 0.06;

/// A rotor head's radius and height; each blade's tip radius, where it
/// starts - inside its head - its chord and thickness, and how far it is
/// raised off its head (degrees) (prototype frame).
const HEAD_R: f32 = 0.065;
const HEAD_H: f32 = 0.05;
const ROTOR_R: f32 = 1.55;
const BLADE_ROOT: f32 = 0.04;
const BLADE_CHORD: f32 = 0.075;
const BLADE_THICK: f32 = 0.012;
const CONING_DEG: f32 = 3.0;

/// The upper rotor's cap, `(radius, height)` base to tip, and how far over
/// the hub its base stands (prototype frame).
const CAP: [(f32, f32); 4] = [(0.065, 0.0), (0.060, 0.03), (0.040, 0.07), (0.0, 0.10)];
const CAP_RISE: f32 = 0.02;

/// The fairing's top at the mast (prototype frame).
fn fairing_top(plan: &PodPlan) -> f32 {
    plan.crown(MAST_Z) - FAIRING_SINK + FAIRING_TOP_R * FAIRING_SQUASH
}

/// The lower rotor's hub height over the skid bottoms (prototype frame).
fn lower(plan: &PodPlan) -> f32 {
    fairing_top(plan) + LOWER_RISE
}

/// The upper rotor's hub height over the skid bottoms (prototype frame).
fn upper(plan: &PodPlan) -> f32 {
    lower(plan) + ROTOR_GAP
}

/// The top of the upper rotor's cap over the skid bottoms (prototype
/// frame): the top of her box.
pub(super) fn head_top(plan: &PodPlan) -> f32 {
    upper(plan) + CAP_RISE + CAP[CAP.len() - 1].1
}

/// The mast, from inside the fairing up into the upper rotor's head.
pub(super) fn mast(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let (top, upper) = (fairing_top(plan), upper(plan));
    kids.push(prim(
        cylinder(
            dim(plan.len(MAST_R)),
            plan.len(upper - top + MAST_FOOT),
            14,
            c.metal.clone(),
        ),
        plan.at(0.0, (upper + top - MAST_FOOT) * 0.5, MAST_Z),
        id_quat(),
    ));
}

/// Both rotors, the lower first: each a head and its blades on a hidden hub
/// turning about `+Y`, the upper the other way and capped.
pub(super) fn rotors(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    kids.push(rotor(plan, lower(plan), ROTOR_DEG_S, false, c));
    kids.push(rotor(plan, upper(plan), -ROTOR_DEG_S, true, c));
}

/// A rotor whose hub stands `y` over the skids, turning at `rate`: its head
/// and [`BLADES`] blades, each raised [`CONING_DEG`] off it - the upper's
/// set half a blade round from the lower's - and, `capped`, the cap over it.
fn rotor(plan: &PodPlan, y: f32, rate: f32, capped: bool, c: &CoaxialColours) -> Generator {
    let mut parts = vec![prim(
        cylinder(dim(plan.len(HEAD_R)), plan.len(HEAD_H), 18, c.metal.clone()),
        [0.0; 3],
        id_quat(),
    )];
    let (span, mid) = (ROTOR_R - BLADE_ROOT, (BLADE_ROOT + ROTOR_R) * 0.5);
    let coning = CONING_DEG.to_radians();
    let offset = if rate > 0.0 {
        0.0
    } else {
        180.0 / BLADES as f32
    };
    for k in 0..BLADES {
        // A blade built along +X, raised about +Z, then turned about the mast
        // to its own place: a turn about +Y carries +X to (cos, 0, -sin).
        let a = (360.0 * k as f32 / BLADES as f32 + offset).to_radians();
        parts.push(prim(
            cuboid(
                [
                    plan.len(span),
                    dim(plan.len(BLADE_THICK)),
                    plan.len(BLADE_CHORD),
                ],
                c.dark.clone(),
            ),
            [
                plan.len(a.cos() * mid),
                plan.len(mid * coning.sin()),
                plan.len(-a.sin() * mid),
            ],
            quat_xyzw(quat_mul(quat_y(a), quat_z(coning))),
        ));
    }
    if capped {
        let cap: Vec<(f32, f32)> = CAP
            .iter()
            .map(|&(r, h)| (plan.len(r), plan.len(h)))
            .collect();
        parts.push(prim(
            lathe(&cap, 18, true, c.trim.clone()),
            [0.0, plan.len(CAP_RISE), 0.0],
            id_quat(),
        ));
    }
    let mut hub = super::hub(plan, plan.at(0.0, y, MAST_Z), c);
    hub.children = parts;
    hub.spin = Some(Spin::about(
        [0.0, 1.0, 0.0],
        SpinTerm::Constant { rate: Fp(rate) },
    ));
    hub
}
