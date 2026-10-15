//! Her landing gear (#1629 Q-C): a main wheel each side on a spring leg out
//! of her belly, in a teardrop spat in the trim colour, and a tail wheel on a
//! fork under the tail cone - where the Airplane preset's raycast wheels
//! stand (`Racer::layout`, #1617): the mains ahead of her centre of mass,
//! the tail wheel short enough that she sits ten degrees tail-down on all
//! three.
//!
//! The tyres roll with her (a Roll spin) and the tail wheel turns with her
//! turning (a Steer spin), each on a hidden hub at its axle; a spat is
//! still, its tyre's bottom showing under it. The mains ride the cockpit's
//! stations, as the prototype's do: each leg's top is read off her belly
//! where the leg meets it, and each spat hangs rigidly off its axle.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::Fp3;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{cylinder, id_quat, prim, quat_xyzw, quat_z, sweep, torus};
use super::super::{FusePlan, MIN_DIM, RacerColours, dim};
use super::{MAIN_X, MAIN_Z, TAIL_R, TAIL_Z, WHEEL_R, hub, tail_wheel_y};

/// A main tyre's tube radius, and its hub's: 5 mm into the tyre's inner
/// face, so the hub holds the tyre (prototype frame).
const TYRE_TUBE: f32 = 0.03;
const MAIN_HUB_R: f32 = WHEEL_R - 0.055;
const MAIN_HUB_W: f32 = 0.07;

/// A main leg (prototype frame): its top a little aft of the axle's
/// station, over the belly there and in from the flank; its foot inboard of
/// the wheel's middle and over its axle, in the hub; its radius, and its
/// facets - the prototype's.
const LEG_TOP_AFT: f32 = 0.03;
const LEG_TOP_RISE: f32 = 0.03;
const LEG_TOP_IN: f32 = 0.02;
const LEG_FOOT_IN: f32 = 0.025;
const LEG_FOOT_RISE: f32 = 0.005;
const LEG_R: f32 = 0.018;
const LEG_RES: u32 = 10;

/// A spat (prototype frame): its stations `(dz from the axle, rise over the
/// axle, radius)` nose to tail, a true racer's, drawn [`SPAT_SCALE`] as large
/// in her full proportion (#1629 Q-I) and [`SPAT_FLAT`] as wide as it is
/// tall - its tyre's bottom below it.
const SPAT: [(f32, f32, f32); 7] = [
    (0.22, 0.040, 0.020),
    (0.16, 0.030, 0.070),
    (0.06, 0.025, 0.100),
    (-0.06, 0.028, 0.098),
    (-0.18, 0.035, 0.075),
    (-0.29, 0.045, 0.040),
    (-0.36, 0.050, 0.012),
];
const SPAT_SCALE: f32 = 1.10;
const SPAT_FLAT: f32 = 0.58;

/// The tail wheel's tyre tube, hub and fork (prototype frame): the fork's
/// root ahead of the tail wheel's station and over the belly there, its end
/// over the axle and ahead of it, drawn 2.6 times as wide as it is deep, at
/// the sanitiser's floor.
const TAIL_TUBE: f32 = 0.015;
const TAIL_HUB_R: f32 = 0.016;
const TAIL_HUB_W: f32 = 0.024;
const FORK_ROOT_AHEAD: f32 = 0.08;
const FORK_ROOT_RISE: f32 = 0.015;
const FORK_END_AHEAD: f32 = 0.01;
const FORK_END_RISE: f32 = 0.025;
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
    c: &RacerColours,
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

/// The mains, port (`+X`) first: on each side a spring leg from her belly
/// down and out to the wheel's hub, a tyre that rolls with her, and its spat.
pub(super) fn mains(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    // The leg's top, rigid off the axle's station: the belly read where it
    // is drawn.
    let top_z = plan.unstretch(plan.stretch(MAIN_Z) + LEG_TOP_AFT);
    let top_y = plan.sill(top_z) + LEG_TOP_RISE;
    let top_x = plan.flank(top_z, top_y) - LEG_TOP_IN;
    let leg_r = dim(plan.len(LEG_R));
    for side in [1.0f32, -1.0] {
        let x = side * MAIN_X;
        let top = plan.at_off(side * top_x, top_y, MAIN_Z, LEG_TOP_AFT);
        let foot = plan.at(x - side * LEG_FOOT_IN, WHEEL_R + LEG_FOOT_RISE, MAIN_Z);
        kids.push(prim(
            sweep(&[(top, leg_r), (foot, leg_r)], LEG_RES, 2, c.metal.clone()),
            [0.0; 3],
            id_quat(),
        ));
        kids.push(wheel(
            plan,
            plan.at(x, WHEEL_R, MAIN_Z),
            (WHEEL_R, TYRE_TUBE),
            (MAIN_HUB_R, MAIN_HUB_W, 14),
            (10, 20),
            c,
        ));
        // Drawn narrower than tall by its node's x scale: the x is divided
        // out.
        let spat: Vec<([f32; 3], f32)> = SPAT
            .iter()
            .map(|&(dz, rise, r)| {
                let [px, py, pz] =
                    plan.at_off(x, WHEEL_R + rise * SPAT_SCALE, MAIN_Z, dz * SPAT_SCALE);
                ([px / SPAT_FLAT, py, pz], plan.len(r * SPAT_SCALE))
            })
            .collect();
        let mut g = prim(sweep(&spat, 20, 5, c.trim.clone()), [0.0; 3], id_quat());
        g.transform.scale = Fp3([SPAT_FLAT, 1.0, 1.0]);
        kids.push(g);
    }
}

/// The tail wheel: a fork down from the tail cone's belly, and a small
/// wheel at its end that steers with her and rolls.
pub(super) fn tail_wheel(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
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
