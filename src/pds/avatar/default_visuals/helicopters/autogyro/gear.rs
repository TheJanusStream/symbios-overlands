//! Her landing gear (#1624): each side a V of legs from her belly to the
//! main wheel's hub and a tall shock leg from her flank - the C.30's wide
//! main gear - and a tail wheel on a fork under her tail cone, which she
//! stands on tail-down ([`super::stance`]).
//!
//! The tyres roll with her (a Roll spin) and the tail wheel turns with her
//! turning (a Steer spin), each on a hidden hub at its axle, as the
//! airplanes' do.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::Fp3;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{cylinder, id_quat, prim, quat_xyzw, quat_z, rod, sweep, torus};
use super::super::{AutogyroColours, MIN_DIM, PodPlan, dim};
use super::stance::GROUND_DEG;
use super::{MAIN_Z, SHOCK_TOP_Z, TAIL_R, TAIL_Z, TRACK, WHEEL_R, flank, hub};

/// A main tyre's tube radius and its hub's radius and width (prototype
/// frame).
const TYRE_TUBE: f32 = 0.05;
const MAIN_HUB_R: f32 = 0.06;
const MAIN_HUB_W: f32 = 0.075;

/// Each V leg's foot, as `dz` from the axle's station (one forward, one
/// aft), its `x` at girth 1, growing with her, how far over the belly it
/// stands, and the legs' radius (prototype frame).
const V_LEGS: [f32; 2] = [0.18, -0.16];
const V_FOOT_X: f32 = 0.09;
const V_FOOT_RISE: f32 = 0.05;
const V_LEG_R: f32 = 0.016;

/// The shock leg's top - over the centreline and in from the flank at
/// [`SHOCK_TOP_Z`] - and its radius (prototype frame).
const SHOCK_RISE: f32 = 0.02;
const SHOCK_IN: f32 = 0.01;
const SHOCK_R: f32 = 0.022;

/// Where every leg of a side meets: in the wheel's hub, this far in from the
/// axle's end and over its line (prototype frame).
const HUB_IN: f32 = 0.03;
const HUB_RISE: f32 = 0.01;

/// The tail wheel's tyre tube, hub and fork (prototype frame): the fork's
/// root ahead of the tail wheel's station and over the belly there, its
/// end over the axle and ahead of it, drawn 2.6 times as wide as it is
/// deep, at the sanitiser's floor.
const TAIL_TUBE: f32 = 0.02;
const TAIL_HUB_R: f32 = 0.022;
const TAIL_HUB_W: f32 = 0.03;
const FORK_ROOT_AHEAD: f32 = 0.12;
const FORK_ROOT_RISE: f32 = 0.015;
const FORK_END_AHEAD: f32 = 0.01;
const FORK_END_RISE: f32 = 0.035;
const FORK_WIDTH: f32 = 2.6;

/// The tail wheel's steering: two fifths of a degree for each degree a
/// second she turns, held to 25 degrees either side - the airplanes'.
pub(crate) const TAIL_STEER_GAIN: f32 = 0.4;
pub(crate) const TAIL_STEER_LIMIT: f32 = 25.0;

/// The tail wheel's axle height, drawn level (prototype frame): where the
/// stance's turn about the main axle brings it [`TAIL_R`] over the ground,
/// so her tail tyre stands beside her mains'. The prototype set the line
/// from the mains' contact to the tail tyre's at [`GROUND_DEG`] BEFORE the
/// turn, which left the tail tyre 2.2 mm off the ground after it.
pub(super) fn tail_wheel_y(plan: &PodPlan) -> f32 {
    let g = GROUND_DEG.to_radians();
    let run = plan.stretch(MAIN_Z) - plan.stretch(TAIL_Z);
    WHEEL_R + (TAIL_R - WHEEL_R) / g.cos() + run * g.tan()
}

/// A tyre and its hub on a hidden hub at `at`, rolling with her.
fn wheel(
    plan: &PodPlan,
    at: [f32; 3],
    (outer, tube): (f32, f32),
    (hub_r, hub_w, hub_res): (f32, f32, u32),
    (minor, major): (u32, u32),
    c: &AutogyroColours,
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

/// Where a side's legs meet, drawn level: in the wheel's hub.
fn leg_end(plan: &PodPlan, side: f32) -> [f32; 3] {
    plan.at(side * (TRACK - HUB_IN), WHEEL_R + HUB_RISE, MAIN_Z)
}

/// The port (`side` 1) or starboard (`-1`) shock leg's two ends, drawn
/// level: its top on her flank, its foot in the wheel's hub.
pub(super) fn shock_leg(plan: &PodPlan, side: f32) -> ([f32; 3], [f32; 3]) {
    let z = SHOCK_TOP_Z;
    let y = plan.pod_at(z).0 + SHOCK_RISE;
    (
        plan.at(side * (flank(plan, z, y) - SHOCK_IN), y, z),
        leg_end(plan, side),
    )
}

/// The mains: each side a V of legs from the belly and a shock leg from
/// the flank, all three meeting in the hub of a fat tyre that rolls with
/// her.
pub(super) fn mains(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    for side in [-1.0f32, 1.0] {
        let end = leg_end(plan, side);
        for dz in V_LEGS {
            let z = MAIN_Z + dz;
            kids.push(rod(
                plan.at(
                    side * V_FOOT_X * plan.girth(),
                    plan.sill(z) + V_FOOT_RISE,
                    z,
                ),
                end,
                dim(plan.len(V_LEG_R)),
                c.metal.clone(),
            ));
        }
        let (top, foot) = shock_leg(plan, side);
        kids.push(rod(top, foot, dim(plan.len(SHOCK_R)), c.metal.clone()));
        kids.push(wheel(
            plan,
            plan.at(side * TRACK, WHEEL_R, MAIN_Z),
            (WHEEL_R, TYRE_TUBE),
            (MAIN_HUB_R, MAIN_HUB_W, 14),
            (10, 20),
            c,
        ));
    }
}

/// The tail wheel: a fork down from the tail cone's belly, and a small
/// wheel at its end that steers with her and rolls.
pub(super) fn tail_wheel(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
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
        },
    ));
    kids.push(steering);
}
