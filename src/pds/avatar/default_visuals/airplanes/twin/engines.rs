//! Her engines (#1625): a nacelle on the wing either side, a radial in a
//! closed cowl at its front - the cowl in the trim colour, its mouth the
//! engine's dark face - an exhaust stack on the nacelle's outboard flank,
//! and a two-blade metal airscrew on a spinner ahead of the cowl.
//!
//! The nacelle's own nose fills the cowl's mouth, so a radial's cylinders
//! behind it would never show: the prototype found eighteen of them hidden
//! there, and drew the engine's face instead. Everything here stands ahead
//! of the cockpit's back wall or is hung rigidly off a station that does,
//! so the cabin's stretch moves none of it.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::generator::Generator;
use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z, sweep,
    with_cut, with_shape,
};
use super::super::{FusePlan, TwinColours, dim};
use super::{NACELLE_X, NACELLE_Y, hub};

/// The nacelle, `(dz from its nose, rise over the thrust line, radius)` cowl
/// to its tail aft of the wing, hung off its nose's station (prototype
/// frame).
const NACELLE_Z: f32 = 1.27;
const NACELLE: [(f32, f32, f32); 8] = [
    (0.0, 0.0, 0.150),
    (-0.10, 0.0, 0.162),
    (-0.25, 0.0, 0.160),
    (-0.45, -0.005, 0.150),
    (-0.67, -0.010, 0.130),
    (-0.87, -0.015, 0.100),
    (-1.07, -0.020, 0.060),
    (-1.22, -0.020, 0.025),
];

/// The cowl's two ends, its radius and its bore; and the engine's face, a
/// dark disc across its mouth just inside the lip (prototype frame).
const COWL_Z: [f32; 2] = [1.17, 1.30];
const COWL_R: f32 = 0.165;
const COWL_HOLLOW: f32 = 0.80;
const FACE_R: f32 = 0.128;
const FACE_H: f32 = 0.02;
const FACE_IN: f32 = 0.025;

/// The exhaust stack (prototype frame): its station, how far outboard of
/// the thrust line and under it it stands, its rake (degrees about `+X`)
/// and its length and radius.
const EXHAUST_Z: f32 = 1.10;
const EXHAUST_OUT: f32 = 0.15;
const EXHAUST_DOWN: f32 = 0.04;
const EXHAUST_RAKE_DEG: f32 = 120.0;
const EXHAUST_LEN: f32 = 0.10;
const EXHAUST_R: f32 = 0.018;

/// Where each airscrew's hub turns: 2 cm ahead of its cowl's lip; its
/// spinner's silhouette, `(radius, height)` base to tip along its axis; and
/// the shaft back through the engine's face (prototype frame).
const PROP_Z: f32 = 1.32;
const SPINNER: [(f32, f32); 4] = [(0.055, 0.0), (0.055, 0.025), (0.040, 0.055), (0.0, 0.085)];
const SHAFT_R: f32 = 0.025;
const SHAFT_LEN: f32 = 0.09;

/// A blade's size (chord, length, thickness), how far its middle stands out
/// from the hub, how far its chord narrows toward its tip, and its pitch
/// (degrees) - all prototype frame.
const BLADE: [f32; 3] = [0.050, 0.30, 0.012];
const BLADE_OUT: f32 = 0.19;
const BLADE_TAPER: f32 = 0.40;
const BLADE_PITCH_DEG: f32 = 18.0;

/// The airscrews' spin (#1625 Q-H): idling at 540 degrees a second, and 41
/// more for each metre a second she travels - so they reach the runtime's
/// ceiling, four turns a second, at 22 m/s, under her level-flight speed of
/// 24, and hold there.
pub(crate) const PROP_IDLE_DEG_S: f32 = 540.0;
pub(crate) const PROP_GAIN_DEG_PER_M: f32 = 41.0;

/// Each airscrew's blades.
const PROP_BLADES: f32 = 2.0;

/// How many times a second a blade passes a point flying level, where her
/// airscrews turn at the ceiling: what her twin-radial voice's thrum is
/// gated at (`fx::twin_radial_drone`), so what is heard is what is seen.
pub(crate) const BLADE_PASS_HZ: f32 = PROP_BLADES * MAX_SPIN_RATE_DEG / 360.0;

/// Both engines, port (`+X`) first: each a nacelle, its cowl, the engine's
/// face, its exhaust stack and its airscrew.
pub(super) fn engines(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    // A quarter turn about +X lays a cylinder's or a lathe's +Y along her.
    let along = quat_xyzw(quat_x(FRAC_PI_2));
    for side in [1.0f32, -1.0] {
        let x = side * NACELLE_X;
        let nacelle: Vec<([f32; 3], f32)> = NACELLE
            .iter()
            .map(|&(dz, dy, r)| (plan.at_off(x, NACELLE_Y + dy, NACELLE_Z, dz), plan.len(r)))
            .collect();
        kids.push(prim(
            sweep(&nacelle, 20, 6, c.body.clone()),
            [0.0; 3],
            id_quat(),
        ));
        let [aft, fore] = COWL_Z;
        kids.push(prim(
            with_cut(
                cylinder(plan.len(COWL_R), plan.len(fore - aft), 24, c.trim.clone()),
                [0.0, 1.0],
                [0.0, 1.0],
                COWL_HOLLOW,
            ),
            plan.at(x, NACELLE_Y, (aft + fore) * 0.5),
            along,
        ));
        kids.push(prim(
            cylinder(plan.len(FACE_R), plan.len(FACE_H), 24, c.dark.clone()),
            plan.at(x, NACELLE_Y, fore - FACE_IN),
            along,
        ));
        kids.push(prim(
            cylinder(
                dim(plan.len(EXHAUST_R)),
                plan.len(EXHAUST_LEN),
                10,
                c.dark.clone(),
            ),
            plan.at(x + side * EXHAUST_OUT, NACELLE_Y - EXHAUST_DOWN, EXHAUST_Z),
            quat_xyzw(quat_x(EXHAUST_RAKE_DEG.to_radians())),
        ));
        kids.push(airscrew(plan, x, c));
    }
}

/// The stack's mouth on the `side` (`1` port, `-1` starboard) engine, in the
/// prototype frame: its `-Y` end, the stack's own half-length along its
/// raked axis from its centre - turned 120 degrees about `+X`, it points
/// aft and up off the nacelle's flank. Where an exhaust aura leaves her
/// (`Twin::fx_mount`, the port engine's).
pub(super) fn exhaust_mouth(side: f32) -> (f32, f32, f32) {
    let rake = EXHAUST_RAKE_DEG.to_radians();
    let half = EXHAUST_LEN * 0.5;
    // A turn of `rake` about +X carries the stack's -Y to (0, -cos, -sin).
    (
        side * (NACELLE_X + EXHAUST_OUT),
        NACELLE_Y - EXHAUST_DOWN - half * rake.cos(),
        EXHAUST_Z - half * rake.sin(),
    )
}

/// An airscrew on a hidden hub ahead of the cowl at `x`: its spinner, two
/// metal blades along `y` and the shaft back through the engine's face,
/// turning about `z` with her speed and idling at rest.
fn airscrew(plan: &FusePlan, x: f32, c: &TwinColours) -> Generator {
    let along = quat_xyzw(quat_x(FRAC_PI_2));
    let cap: Vec<(f32, f32)> = SPINNER
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    let mut parts = vec![prim(
        lathe(&cap, 16, true, c.metal.clone()),
        [0.0; 3],
        along,
    )];
    for k in 0..PROP_BLADES as u32 {
        // Each blade built pointing +Y and narrowing toward that tip,
        // turned about the shaft to its own place - the second half round,
        // its tip down.
        let turn = k as f32 * PI;
        let (s, cs) = turn.sin_cos();
        let blade = with_shape(
            cuboid(
                [
                    plan.len(BLADE[0]),
                    plan.len(BLADE[1]),
                    dim(plan.len(BLADE[2])),
                ],
                c.metal.clone(),
            ),
            [BLADE_TAPER, 0.0],
            [0.0; 3],
            [0.0; 2],
        );
        parts.push(prim(
            blade,
            [s * plan.len(BLADE_OUT), cs * plan.len(BLADE_OUT), 0.0],
            quat_xyzw(quat_mul(
                quat_z(-turn),
                quat_y(BLADE_PITCH_DEG.to_radians()),
            )),
        ));
    }
    parts.push(prim(
        cylinder(
            dim(plan.len(SHAFT_R)),
            plan.len(SHAFT_LEN),
            10,
            c.metal.clone(),
        ),
        [0.0, 0.0, -plan.len(SHAFT_LEN * 0.5)],
        along,
    ));
    let mut screw = hub(plan, plan.at(x, NACELLE_Y, PROP_Z), c);
    screw.children = parts;
    screw.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Speed {
            idle: Fp(PROP_IDLE_DEG_S),
            gain: Fp(PROP_GAIN_DEG_PER_M),
        },
    ));
    screw
}
