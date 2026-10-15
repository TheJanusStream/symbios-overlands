//! Her landing gear: a main wheel under each nacelle on an oleo leg down
//! its inboard side and a drag brace aft, and a tail wheel on a fork under
//! the tail cone - where the Airplane preset's raycast wheels stand
//! (`Twin::layout`, #1617): the mains ahead of her centre of mass, the tail
//! wheel short enough that she sits eleven degrees tail-down on all three.
//!
//! The tyres roll with her (a Roll spin) and the tail wheel turns with her
//! turning (a Steer spin), each on a hidden hub at its axle. The mains stand
//! ahead of the cockpit's back wall, the brace's top hung rigidly off the
//! axle's station, so the cabin's stretch moves none of them.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::Fp3;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{cylinder, id_quat, prim, quat_xyzw, quat_z, rod, sweep, torus};
use super::super::{FusePlan, MIN_DIM, TwinColours, dim};
use super::{MAIN_Z, NACELLE_X, NACELLE_Y, TAIL_R, TAIL_Z, WHEEL_R, hub, tail_wheel_y};

/// A main tyre's tube radius and its hub's radius and width (prototype
/// frame).
const TYRE_TUBE: f32 = 0.05;
const MAIN_HUB_R: f32 = 0.06;
const MAIN_HUB_W: f32 = 0.10;

/// Where a main's legs meet its hub - this far inboard of the wheel's
/// middle and over its axle - and where their tops stand in the nacelle:
/// the oleo's inboard of the thrust line, under it and a little ahead of
/// the axle, the brace's further under it and aft; and each one's radius
/// (prototype frame).
const LEG_IN: f32 = 0.05;
const LEG_RISE: f32 = 0.005;
const TOP_IN: f32 = 0.06;
const OLEO_DOWN: f32 = 0.08;
const OLEO_AHEAD: f32 = 0.02;
const OLEO_R: f32 = 0.020;
const BRACE_DOWN: f32 = 0.10;
const BRACE_AFT: f32 = 0.20;
const BRACE_R: f32 = 0.014;

/// The tail wheel's tyre tube, hub and fork (prototype frame): the fork's
/// root ahead of the tail wheel's station and over the belly there, its end
/// over the axle and ahead of it, drawn 2.6 times as wide as it is deep, at
/// the sanitiser's floor.
const TAIL_TUBE: f32 = 0.02;
const TAIL_HUB_R: f32 = 0.022;
const TAIL_HUB_W: f32 = 0.03;
const FORK_ROOT_AHEAD: f32 = 0.10;
const FORK_ROOT_RISE: f32 = 0.015;
const FORK_END_AHEAD: f32 = 0.01;
const FORK_END_RISE: f32 = 0.035;
const FORK_WIDTH: f32 = 2.6;

/// The tail wheel's steering: two fifths of a degree for each degree a
/// second she turns, held to 25 degrees either side - the family's.
const TAIL_STEER_GAIN: f32 = 0.4;
const TAIL_STEER_LIMIT: f32 = 25.0;

/// A tyre and its hub on a hidden hub at `at`, rolling with her.
fn wheel(
    plan: &FusePlan,
    at: [f32; 3],
    (outer, tube): (f32, f32),
    (hub_r, hub_w, hub_res): (f32, f32, u32),
    (minor, major): (u32, u32),
    c: &TwinColours,
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

/// The mains, port (`+X`) first: under each nacelle an oleo leg and a drag
/// brace from inside it down to the wheel's hub, and a fat tyre that rolls
/// with her.
pub(super) fn mains(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    for side in [1.0f32, -1.0] {
        let x = side * NACELLE_X;
        let end = plan.at(x - side * LEG_IN, WHEEL_R + LEG_RISE, MAIN_Z);
        let top_x = x - side * TOP_IN;
        kids.push(rod(
            plan.at_off(top_x, NACELLE_Y - OLEO_DOWN, MAIN_Z, OLEO_AHEAD),
            end,
            dim(plan.len(OLEO_R)),
            c.metal.clone(),
        ));
        kids.push(rod(
            plan.at_off(top_x, NACELLE_Y - BRACE_DOWN, MAIN_Z, -BRACE_AFT),
            end,
            dim(plan.len(BRACE_R)),
            c.metal.clone(),
        ));
        kids.push(wheel(
            plan,
            plan.at(x, WHEEL_R, MAIN_Z),
            (WHEEL_R, TYRE_TUBE),
            (MAIN_HUB_R, MAIN_HUB_W, 14),
            (10, 20),
            c,
        ));
    }
}

/// The tail wheel: a fork down from the tail cone's belly, and a small
/// wheel at its end that steers with her and rolls.
pub(super) fn tail_wheel(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let axle_y = tail_wheel_y(plan);
    // The fork's root is RIGID off the tail wheel's station: the belly is
    // read where it is drawn.
    let root_z = plan.unstretch(plan.stretch(TAIL_Z) + FORK_ROOT_AHEAD);
    let fork = [
        plan.at_off(
            0.0,
            plan.sill(root_z) + FORK_ROOT_RISE,
            TAIL_Z,
            FORK_ROOT_AHEAD,
        ),
        plan.at_off(0.0, axle_y + FORK_END_RISE, TAIL_Z, FORK_END_AHEAD),
    ];
    // Drawn wider than deep by its node's x scale: the x is divided out.
    let fork: Vec<([f32; 3], f32)> = fork
        .iter()
        .map(|&[x, y, z]| ([x / FORK_WIDTH, y, z], dim(plan.len(MIN_DIM))))
        .collect();
    let mut g = prim(sweep(&fork, 8, 2, c.metal.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([FORK_WIDTH, 1.0, 1.0]);
    kids.push(g);
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
