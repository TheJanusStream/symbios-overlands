//! The engine bay behind the bubble: the open tube frame from the bulkhead
//! to the truss's root, the upright engine with a finned bank of cylinders
//! each side and its exhaust stack, the transmission and the mast rising out
//! of it, and the twin saddle tanks strapped high either side of the mast.
//!
//! All of it stands forward of the boom's root, so no stretch moves it.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;

use super::super::super::common::{cylinder, id_quat, prim, quat_x, quat_xyzw, rod, sweep, torus};
use super::super::{PodPlan, TrainerColours, dim};
use super::boom::frame_at;
use super::bubble::BULKHEAD_Z;
use super::rotor::MAST_TOP;

/// The engine's station, and the mast's over it (prototype frame).
pub(super) const ENGINE_Z: f32 = 0.66;

/// The engine block: its centre's height, its radius and its height.
const ENGINE_Y: f32 = 0.36;
const ENGINE_R: f32 = 0.095;
const ENGINE_H: f32 = 0.20;

/// Where the skids' aft cross tube meets the engine block's foot.
pub(super) const ENGINE_FOOT: f32 = ENGINE_Y - ENGINE_H * 0.5 - 0.01;

/// The exhaust stack's middle - under the starboard bank, `-X` - its rake
/// (degrees about `+X`: a turn of -150 carries its `+Y` down and aft) and
/// its length. Its upper end sits in the bank, where the prototype hung it
/// 4 cm clear of anything (`touch::assert_one_machine`).
const EXHAUST_AT: (f32, f32, f32) = (-0.11, 0.293, ENGINE_Z - 0.09);
const EXHAUST_RAKE_DEG: f32 = -150.0;
const EXHAUST_LEN: f32 = 0.12;

/// The transmission over the engine: its centre's height, radius and
/// height. It sits a centimetre down into the block, where the prototype
/// stood it on the block's top face (touching in a plane, which is no join).
const GEARBOX_Y: f32 = 0.50;
const GEARBOX_R: f32 = 0.058;
const GEARBOX_H: f32 = 0.10;

/// The saddle tanks, fore and aft, their height, how far out and their
/// radius (prototype frame).
const TANK_Z0: f32 = 0.86;
const TANK_Z1: f32 = 0.46;
const TANK_Y: f32 = 0.66;
const TANK_X: f32 = 0.175;
const TANK_R: f32 = 0.072;

/// The tube frame: four rods from the bulkhead to the truss's root, the
/// lower pair off the bubble's floor to the lower longerons, the upper pair
/// off its shoulders to the upper.
pub(super) fn frame(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let (fy, fr, fh) = plan.pod_at(BULKHEAD_Z);
    let floor = plan.sill(BULKHEAD_Z) + 0.03;
    let root = frame_at(0.0);
    let z = BULKHEAD_Z - 0.02;
    for side in [-1.0f32, 1.0] {
        kids.push(rod(
            plan.at(side * fr * 0.75, floor, z),
            plan.at(side * root.lower_x, root.lower_y, root.z),
            dim(plan.len(0.012)),
            c.body.clone(),
        ));
        kids.push(rod(
            plan.at(side * fr * 0.55, fy + fh * 0.55, z),
            plan.at(0.0, root.upper_y, root.z),
            dim(plan.len(0.011)),
            c.body.clone(),
        ));
    }
}

/// The engine and what stands on it: the block, a bank of cylinders each
/// side, the exhaust stack, the transmission and the mast up to the rotor
/// head.
pub(super) fn engine(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    kids.push(prim(
        cylinder(plan.len(ENGINE_R), plan.len(ENGINE_H), 16, c.dark.clone()),
        plan.at(0.0, ENGINE_Y, ENGINE_Z),
        id_quat(),
    ));
    // Its cylinder heads: one finned bank a side, turned along the engine.
    for side in [-1.0f32, 1.0] {
        kids.push(prim(
            cylinder(plan.len(0.030), plan.len(0.18), 10, c.metal.clone()),
            plan.at(side * 0.11, ENGINE_Y, ENGINE_Z),
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    // Its stack, down and aft off the starboard bank - where an exhaust,
    // steam or ember aura leaves her.
    let (x, y, z) = EXHAUST_AT;
    kids.push(prim(
        cylinder(
            dim(plan.len(0.016)),
            plan.len(EXHAUST_LEN),
            10,
            c.dark.clone(),
        ),
        plan.at(x, y, z),
        quat_xyzw(quat_x(EXHAUST_RAKE_DEG.to_radians())),
    ));
    // The transmission over it, the mast rising out of its middle into the
    // rotor head - the prototype stood it on the box's top face, touching
    // in a plane, and it came away whole with the rotor
    // (`touch::assert_one_machine`).
    kids.push(prim(
        cylinder(
            plan.len(GEARBOX_R),
            plan.len(GEARBOX_H),
            16,
            c.metal.clone(),
        ),
        plan.at(0.0, GEARBOX_Y, ENGINE_Z),
        id_quat(),
    ));
    let foot = GEARBOX_Y;
    kids.push(prim(
        cylinder(
            dim(plan.len(0.021)),
            plan.len(MAST_TOP - foot),
            12,
            c.metal.clone(),
        ),
        plan.at(0.0, (MAST_TOP + foot) * 0.5, ENGINE_Z),
        id_quat(),
    ));
}

/// The stack's mouth, in the prototype frame: its lower end, half the
/// stack's length along its raked axis from its centre - where a burning
/// aura leaves her (`Trainer::fx_mount`).
pub(super) fn exhaust_mouth() -> (f32, f32, f32) {
    let rake = EXHAUST_RAKE_DEG.to_radians();
    let half = EXHAUST_LEN * 0.5;
    let (x, y, z) = EXHAUST_AT;
    // A turn of `rake` about +X carries the stack's +Y to (cos, sin) in
    // (y, z): down and aft for her -150 degrees.
    (x, y + half * rake.cos(), z + half * rake.sin())
}

/// The saddle tanks: a capsule each side, high beside the mast, two straps
/// round each in the scheme's second hue, and a cradle down to the engine.
pub(super) fn tanks(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let mid = (TANK_Z0 + TANK_Z1) * 0.5;
    for side in [-1.0f32, 1.0] {
        let x = side * TANK_X;
        let pts = [
            (TANK_Z0, 0.035),
            (TANK_Z0 - 0.04, TANK_R),
            (mid, TANK_R),
            (TANK_Z1 + 0.04, TANK_R),
            (TANK_Z1, 0.035),
        ]
        .map(|(z, r)| (plan.at(x, TANK_Y, z), plan.len(r)));
        kids.push(prim(
            sweep(&pts, 20, 6, c.body.clone()),
            [0.0; 3],
            id_quat(),
        ));
        // The straps: a ring round the tank, turned to stand across it.
        for z in [TANK_Z0 - 0.09, TANK_Z1 + 0.09] {
            kids.push(prim(
                torus(
                    dim(plan.len(0.007)),
                    plan.len(TANK_R + 0.002),
                    c.straps.clone(),
                ),
                plan.at(x, TANK_Y, z),
                quat_xyzw(quat_x(FRAC_PI_2)),
            ));
        }
        kids.push(rod(
            plan.at(x, TANK_Y - TANK_R, mid),
            plan.at(side * 0.10, 0.40, mid),
            dim(plan.len(0.009)),
            c.metal.clone(),
        ));
    }
}
