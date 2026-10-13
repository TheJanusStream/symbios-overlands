//! The two rotors (#1630 Q-B, Q-C): three blades each, 2.9 m across at the
//! nominal machine, the front one over her cockpit and the rear one 28 cm
//! higher on her aft pylon, turning the other way at the same rate - a
//! tandem's whole point, the two torques cancelled with no tail rotor to pay
//! for it. Their discs overlap over the middle of her cabin, the rear's
//! blades passing over the front's.
//!
//! Each rotor turns on a hidden hub at its head, with a Constant node
//! [`Spin`] about its mast (#1613 recommendation 3: always turning). A spin
//! turns the node's children about the node's own origin and rebases
//! nothing, so every part is laid relative to its hub and the hub stands
//! unturned.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_xyzw, quat_y, quat_z,
};
use super::super::{PodPlan, TandemColours, dim};
use super::cabin_section;

/// Each rotor's turn (degrees a second), the front one way and the rear the
/// other: three turns a second, the coaxial's.
pub(crate) const ROTOR_DEG_S: f32 = 1080.0;

/// Each rotor's blades.
pub(crate) const BLADES: u32 = 3;

/// How many times a second a blade of either rotor passes a point: the
/// coaxial's, whose double chop she is heard by (#1630 Q-H), so what is
/// heard is what is seen.
#[cfg(test)]
pub(crate) const BLADE_PASS_HZ: f32 = BLADES as f32 * ROTOR_DEG_S / 360.0;

/// The front rotor's mast station, over the cockpit, and the rear rotor's,
/// on the aft pylon (prototype frame) - the rear's stretched.
pub(super) const FRONT_Z: f32 = 1.10;
pub(super) const REAR_Z: f32 = -1.20;

/// Each rotor's hub over the cabin's roof (prototype frame): the rear's 28
/// cm over the front's, so its blades pass over the front's where their
/// discs overlap.
const FRONT_RISE: f32 = 0.22;
const REAR_RISE: f32 = 0.50;

/// A rotor head's radius and height; each blade's tip radius, where it
/// starts - inside its head - its chord and thickness, and how far it is
/// raised off its head (degrees); and how far round the rear rotor's blades
/// are set from the front's (degrees) (prototype frame).
const HEAD_R: f32 = 0.075;
const HEAD_H: f32 = 0.06;
const ROTOR_R: f32 = 1.45;
const BLADE_ROOT: f32 = 0.05;
const BLADE_CHORD: f32 = 0.10;
const BLADE_THICK: f32 = 0.014;
const CONING_DEG: f32 = 3.0;
const REAR_PHASE_DEG: f32 = 60.0;

/// The rear rotor's cap, `(radius, height)` base to tip, and how far over
/// the hub its base stands - on the head's top (prototype frame).
const CAP: [(f32, f32); 4] = [(0.075, 0.0), (0.068, 0.03), (0.045, 0.07), (0.0, 0.10)];
const CAP_RISE: f32 = 0.03;

/// The cabin's roof (prototype frame).
fn roof(plan: &PodPlan) -> f32 {
    let (cy, _, h) = cabin_section(plan);
    cy + h
}

/// The front rotor's hub height over the ground (prototype frame).
pub(super) fn front_hub(plan: &PodPlan) -> f32 {
    roof(plan) + FRONT_RISE
}

/// The rear rotor's hub height over the ground (prototype frame).
pub(super) fn rear_hub(plan: &PodPlan) -> f32 {
    roof(plan) + REAR_RISE
}

/// The top of the rear rotor's cap over the ground (prototype frame): the
/// top of her box.
pub(super) fn head_top(plan: &PodPlan) -> f32 {
    rear_hub(plan) + CAP_RISE + CAP[CAP.len() - 1].1
}

/// Both rotors, the front first: each a head and its blades on a hidden hub
/// turning about `+Y`, the rear the other way, its blades set a half blade
/// round, and capped in the trim colour.
pub(super) fn rotors(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    kids.push(rotor(
        plan,
        plan.at(0.0, front_hub(plan), FRONT_Z),
        ROTOR_DEG_S,
        0.0,
        false,
        c,
    ));
    kids.push(rotor(
        plan,
        plan.at(0.0, rear_hub(plan), REAR_Z),
        -ROTOR_DEG_S,
        REAR_PHASE_DEG,
        true,
        c,
    ));
}

/// A rotor whose hub stands `at`, turning at `rate`: its head, its cap if
/// `capped`, and [`BLADES`] blades set `phase` degrees round, each raised
/// [`CONING_DEG`] off it.
fn rotor(
    plan: &PodPlan,
    at: [f32; 3],
    rate: f32,
    phase: f32,
    capped: bool,
    c: &TandemColours,
) -> Generator {
    let mut parts = vec![prim(
        cylinder(dim(plan.len(HEAD_R)), plan.len(HEAD_H), 16, c.metal.clone()),
        [0.0; 3],
        id_quat(),
    )];
    if capped {
        let cap: Vec<(f32, f32)> = CAP
            .iter()
            .map(|&(r, h)| (plan.len(r), plan.len(h)))
            .collect();
        parts.push(prim(
            lathe(&cap, 16, true, c.trim.clone()),
            [0.0, plan.len(CAP_RISE), 0.0],
            id_quat(),
        ));
    }
    let mid = (BLADE_ROOT + ROTOR_R) * 0.5;
    let coning = CONING_DEG.to_radians();
    // Raised about its middle, a blade's root stays on the head's plane.
    let lift = (mid - BLADE_ROOT) * coning.sin();
    for k in 0..BLADES {
        // A blade built along +X, raised about +Z, then turned about the
        // mast to its own place: a turn of `a - 90` degrees about +Y carries
        // +X to (sin a, 0, cos a).
        let a = (phase + 360.0 * k as f32 / BLADES as f32).to_radians();
        parts.push(prim(
            cuboid(
                [
                    plan.len(ROTOR_R - BLADE_ROOT),
                    dim(plan.len(BLADE_THICK)),
                    plan.len(BLADE_CHORD),
                ],
                c.dark.clone(),
            ),
            [
                plan.len(a.sin() * mid),
                plan.len(lift),
                plan.len(a.cos() * mid),
            ],
            quat_xyzw(quat_mul(quat_y(a - FRAC_PI_2), quat_z(coning))),
        ));
    }
    let mut hub = super::hub(plan, at, c);
    hub.children = parts;
    hub.spin = Some(Spin::about(
        [0.0, 1.0, 0.0],
        SpinTerm::Constant { rate: Fp(rate) },
    ));
    hub
}
