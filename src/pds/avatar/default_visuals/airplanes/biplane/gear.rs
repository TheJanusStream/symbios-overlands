//! Her landing gear: a V of legs each side from her belly to the axle of a
//! fat main tyre, and a tail wheel on a leaf spring under her tail cone -
//! the high-wing's gear, stood where the prototype stood hers.
//!
//! Where they stand is where the Airplane preset's raycast wheels stand
//! (`Biplane::layout`, #1617): the mains ahead of her centre of mass, the
//! tail wheel near her tail post and short enough that she sits twelve
//! degrees tail-down on all three. The tyres roll with her (a Roll spin)
//! and the tail wheel turns with her turning (a Steer spin), each on a
//! hidden hub at its axle.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::Fp3;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{cylinder, id_quat, prim, quat_xyzw, quat_z, rod, sweep, torus};
use super::super::{BiplaneColours, FusePlan, MIN_DIM, dim};
use super::{MAIN_Z, TAIL_R, TAIL_Z, TRACK, WHEEL_R, hub, tail_wheel_y};

/// A main tyre's tube radius and its hub's radius and width (prototype
/// frame).
const TYRE_TUBE: f32 = 0.055;
const MAIN_HUB_R: f32 = 0.065;
const MAIN_HUB_W: f32 = 0.08;

/// Each main leg's foot, `(dz, x)` from the axle's station - `x` at girth 1,
/// growing with her - how far over the belly it stands, and where it meets
/// the axle: a V, one leg forward and one aft (prototype frame).
const LEGS: [(f32, f32); 2] = [(0.16, 0.10), (-0.18, 0.10)];
const LEG_RISE: f32 = 0.06;
const LEG_R: f32 = 0.018;
const LEG_REACH: f32 = 0.90;

/// The tail wheel's tyre tube, hub and leaf (prototype frame): the leaf's
/// root station under the tail cone, drawn 2.6 times as wide as it is
/// deep, at the sanitiser's floor.
const TAIL_TUBE: f32 = 0.02;
const TAIL_HUB_R: f32 = 0.022;
const TAIL_HUB_W: f32 = 0.03;
const LEAF_ROOT: f32 = -1.10;
const LEAF_WIDTH: f32 = 2.6;

/// The tail wheel's steering: two fifths of a degree for each degree a
/// second she turns, held to 25 degrees either side - the high-wing's.
const TAIL_STEER_GAIN: f32 = 0.4;
const TAIL_STEER_LIMIT: f32 = 25.0;

/// A tyre and its hub on a hidden hub at `at`, rolling with her.
fn wheel(
    plan: &FusePlan,
    at: [f32; 3],
    (outer, tube): (f32, f32),
    (hub_r, hub_w, hub_res): (f32, f32, u32),
    (minor, major): (u32, u32),
    c: &BiplaneColours,
) -> Generator {
    let mut tyre = torus(dim(plan.len(tube)), plan.len(outer - tube), c.dark.clone());
    if let GeneratorKind::Torus {
        minor_resolution,
        major_resolution,
        ..
    } = &mut tyre
    {
        (*minor_resolution, *major_resolution) = (minor, major);
    }
    // A torus rings its +Y and a cylinder stands on it; a quarter turn about
    // +Z lays both across her, on the axle.
    let across = quat_xyzw(quat_z(FRAC_PI_2));
    let mut roller = hub(plan, at, c);
    roller.children = vec![
        prim(tyre, [0.0; 3], across),
        prim(
            cylinder(
                dim(plan.len(hub_r)),
                plan.len(hub_w),
                hub_res,
                c.metal.clone(),
            ),
            [0.0; 3],
            across,
        ),
    ];
    roller.spin = Some(Spin::about(
        [1.0, 0.0, 0.0],
        SpinTerm::Roll {
            radius: Fp(plan.len(outer)),
        },
    ));
    roller
}

/// The mains: a V of legs each side from the belly to the axle, and a fat
/// tyre on a hub that rolls with her.
pub(super) fn mains(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    for side in [-1.0f32, 1.0] {
        let axle_x = side * TRACK;
        for (dz, x) in LEGS {
            let z = MAIN_Z + dz;
            kids.push(rod(
                plan.at(side * x * plan.girth(), plan.sill(z) + LEG_RISE, z),
                plan.at(axle_x * LEG_REACH, WHEEL_R + 0.01, MAIN_Z),
                dim(plan.len(LEG_R)),
                c.metal.clone(),
            ));
        }
        kids.push(wheel(
            plan,
            plan.at(axle_x, WHEEL_R, MAIN_Z),
            (WHEEL_R, TYRE_TUBE),
            (MAIN_HUB_R, MAIN_HUB_W, 14),
            (10, 20),
            c,
        ));
    }
}

/// The tail wheel: a leaf spring running aft and down from the tail cone's
/// belly, and a small wheel at its end that steers with her and rolls.
pub(super) fn tail_wheel(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let axle_y = tail_wheel_y(plan);
    let leaf = [
        plan.at(0.0, plan.sill(LEAF_ROOT) + 0.012, LEAF_ROOT),
        plan.at(0.0, axle_y + 0.03, TAIL_Z + 0.015),
    ];
    // Drawn wider than deep by its node's x scale: the x is divided out.
    let leaf: Vec<([f32; 3], f32)> = leaf
        .iter()
        .map(|&[x, y, z]| ([x / LEAF_WIDTH, y, z], dim(plan.len(MIN_DIM))))
        .collect();
    let mut spring = prim(sweep(&leaf, 8, 2, c.metal.clone()), [0.0; 3], id_quat());
    spring.transform.scale = Fp3([LEAF_WIDTH, 1.0, 1.0]);
    kids.push(spring);
    let rolling = wheel(
        plan,
        [0.0; 3],
        (TAIL_R, TAIL_TUBE),
        (TAIL_HUB_R, TAIL_HUB_W, 10),
        (8, 14),
        c,
    );
    let mut steering = hub(plan, plan.at(0.0, axle_y, TAIL_Z), c);
    steering.children = vec![rolling];
    steering.spin = Some(Spin::about(
        [0.0, 1.0, 0.0],
        SpinTerm::Steer {
            gain: Fp(TAIL_STEER_GAIN),
            limit: Fp(TAIL_STEER_LIMIT),
            follow_yaw: false,
        },
    ));
    kids.push(steering);
}
