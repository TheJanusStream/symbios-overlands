//! Her tail (#1628 Q-A, Q-B): an H-tail - a stabiliser through the boom and
//! a raked fin at each tip, their tops in the trim colour - and behind it the
//! pusher, five blades turning with her speed inside a shroud in the trim
//! colour, the shroud held on three stators off the tail cone; and the
//! lights, the nav lights on the fins and a white light under the tail
//! cone's end.
//!
//! Everything here stands on boom stations, so the blueprint's boom stretch
//! carries it ([`PodPlan::at`]); what is fixed to something on a stretched
//! station - a fin to the stabiliser, a light to a fin - rides it rigidly
//! ([`PodPlan::at_off`]).

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z, rod,
    sphere, with_cut, with_shape,
};
use super::super::{CoaxialColours, PodPlan, dim};
use super::PROFILE;

/// The tail cone's end, in the stators: the fuselage's last station, whose
/// centreline is the pusher's axis (prototype frame).
const TAIL_CONE_Z: f32 = PROFILE.stations[PROFILE.stations.len() - 1].0;

/// The stabiliser's station, half-span - its tips reaching into the fins -
/// thickness and chord (prototype frame).
const STAB_Z: f32 = -1.30;
const STAB_HALF: f32 = 0.425;
const STAB_THICK: f32 = 0.024;
const STAB_CHORD: f32 = 0.20;

/// The end fins (prototype frame): their `x`, height, thickness and chord,
/// how far their middle stands over the stabiliser and aft of its station,
/// their rake (degrees about `+X`; negative tips their tops aft), and how far
/// their chord draws in toward their tops.
const FIN_X: f32 = 0.42;
const FIN_H: f32 = 0.44;
const FIN_THICK: f32 = 0.022;
const FIN_CHORD: f32 = 0.25;
const FIN_RISE: f32 = 0.07;
const FIN_AFT: f32 = 0.02;
const FIN_RAKE_DEG: f32 = -14.0;
const FIN_TAPER: f32 = 0.30;

/// Each fin's top in the trim colour (prototype frame): its size, and how
/// far under the fin's top its middle stands, along the fin's rake.
const FIN_CAP: [f32; 3] = [0.024, 0.10, 0.19];
const FIN_CAP_DOWN: f32 = 0.05;

/// The pusher (prototype frame): its hub's station, its blades' tip radius
/// and where they start, their chord and thickness, and their pitch
/// (degrees).
const PROP_Z: f32 = -1.73;
const PROP_R: f32 = 0.235;
const PROP_ROOT: f32 = 0.04;
const PROP_CHORD: f32 = 0.045;
const PROP_THICK: f32 = 0.012;
const PROP_PITCH_DEG: f32 = 28.0;
const PROP_BLADES: u32 = 5;

/// The pusher's spinner, `(radius, height)` base to tip along its axis, aft
/// (prototype frame).
const SPINNER: [(f32, f32); 4] = [(0.055, 0.0), (0.052, 0.02), (0.035, 0.045), (0.0, 0.06)];

/// The pusher's spin (#1628 Q-G): idling at 540 degrees a second and 63
/// more for each metre a second she travels, so it reaches the runtime's
/// ceiling, four turns a second, at 14.3 m/s - under her top speed of 14.4.
pub(crate) const PROP_IDLE_DEG_S: f32 = 540.0;
pub(crate) const PROP_GAIN_DEG_PER_M: f32 = 63.0;

/// The driveshaft's radius, how far ahead of the tail cone's end it starts,
/// inside it, and how far past the hub's middle it reaches, into it
/// (prototype frame).
const SHAFT_R: f32 = 0.020;
const SHAFT_FROM: f32 = 0.02;
const SHAFT_INTO: f32 = 0.005;

/// The shroud (prototype frame): its middle's station, outer radius, length
/// and bore - its inner radius a share of its outer.
const DUCT_Z: f32 = -1.73;
const DUCT_R: f32 = 0.30;
const DUCT_LEN: f32 = 0.14;
const DUCT_HOLLOW: f32 = 0.86;

/// The stators (prototype frame): their station - just inside the tail
/// cone's end, ahead of the pusher's disc - their radius, and how far each
/// reaches past its two ends: into the tail cone off the axis, and into the
/// shroud's wall.
const STATOR_Z: f32 = -1.665;
const STATOR_R: f32 = 0.014;
const STATOR_IN: f32 = 0.02;

/// The nav lights (prototype frame): their radius, how far outboard of the
/// fins' middles and over the stabiliser they stand, and how far aft of its
/// station.
const NAV_R: f32 = 0.014;
const NAV_OUT: f32 = 0.015;
const NAV_RISE: f32 = 0.02;
const NAV_AFT: f32 = 0.06;

/// The white tail light (prototype frame): its radius, how far over the
/// tail cone's keel and ahead of its end it stands.
const TAIL_LIGHT_R: f32 = 0.016;
const TAIL_LIGHT_RISE: f32 = 0.005;
const TAIL_LIGHT_AHEAD: f32 = 0.02;

/// The pusher's axis height: the tail cone's centreline (prototype frame).
fn tail_y(plan: &PodPlan) -> f32 {
    plan.pod_at(TAIL_CONE_Z).0
}

/// The stabiliser's height: the boom's centreline at its station
/// (prototype frame).
fn stab_y(plan: &PodPlan) -> f32 {
    plan.pod_at(STAB_Z).0
}

/// The H-tail: the stabiliser through the boom, and a fin at each tip,
/// starboard (`-X`) first, each raked and its top in the trim colour.
pub(super) fn h_tail(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let y = stab_y(plan);
    kids.push(prim(
        cuboid(
            [
                plan.len(2.0 * STAB_HALF),
                plan.len(STAB_THICK),
                plan.len(STAB_CHORD),
            ],
            c.body.clone(),
        ),
        plan.at(0.0, y, STAB_Z),
        id_quat(),
    ));
    let rake = FIN_RAKE_DEG.to_radians();
    let turn = quat_xyzw(quat_x(rake));
    // Up the fin to its cap, turned by the rake: a turn about +X carries +Y
    // toward +Z by its sine, so a negative rake carries it aft.
    let up = FIN_H * 0.5 - FIN_CAP_DOWN;
    for side in [-1.0f32, 1.0] {
        let x = side * FIN_X;
        kids.push(prim(
            with_shape(
                cuboid(
                    [plan.len(FIN_THICK), plan.len(FIN_H), plan.len(FIN_CHORD)],
                    c.body.clone(),
                ),
                [0.0, FIN_TAPER],
                [0.0; 3],
                [0.0; 2],
            ),
            plan.at_off(x, y + FIN_RISE, STAB_Z, -FIN_AFT),
            turn,
        ));
        kids.push(prim(
            cuboid(
                [
                    plan.len(FIN_CAP[0]),
                    plan.len(FIN_CAP[1]),
                    plan.len(FIN_CAP[2]),
                ],
                c.trim.clone(),
            ),
            plan.at_off(
                x,
                y + FIN_RISE + up * rake.cos(),
                STAB_Z,
                -FIN_AFT + up * rake.sin(),
            ),
            turn,
        ));
    }
}

/// The pusher on a hidden hub at the tail cone's axis: its spinner, aft,
/// and five blades, turning about `+Z` with her speed and idling at rest;
/// and the driveshaft that carries it, still, out of the tail cone.
pub(super) fn pusher(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let ty = tail_y(plan);
    let spinner: Vec<(f32, f32)> = SPINNER
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    // A quarter turn about -X lays a lathe's +Y aft along -Z.
    let mut parts = vec![prim(
        lathe(&spinner, 16, true, c.metal.clone()),
        [0.0; 3],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    )];
    let mid = (PROP_ROOT + PROP_R) * 0.5;
    for k in 0..PROP_BLADES {
        // Each blade built along +Y and pitched about it, turned about the
        // shaft to its own place.
        let a = (360.0 * k as f32 / PROP_BLADES as f32).to_radians();
        parts.push(prim(
            cuboid(
                [
                    plan.len(PROP_CHORD),
                    plan.len(PROP_R - PROP_ROOT),
                    dim(plan.len(PROP_THICK)),
                ],
                c.dark.clone(),
            ),
            [plan.len(a.sin() * mid), plan.len(a.cos() * mid), 0.0],
            quat_xyzw(quat_mul(quat_z(-a), quat_y(PROP_PITCH_DEG.to_radians()))),
        ));
    }
    let mut hub = super::hub(plan, plan.at(0.0, ty, PROP_Z), c);
    hub.children = parts;
    hub.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Speed {
            idle: Fp(PROP_IDLE_DEG_S),
            gain: Fp(PROP_GAIN_DEG_PER_M),
        },
    ));
    kids.push(hub);
    // The shaft from inside the tail cone into the hub, rigid off the hub's
    // station at its aft end.
    let fore = plan.at(0.0, ty, TAIL_CONE_Z + SHAFT_FROM);
    let aft = plan.at_off(0.0, ty, PROP_Z, -SHAFT_INTO);
    kids.push(prim(
        cylinder(
            dim(plan.len(SHAFT_R)),
            fore[2] - aft[2],
            10,
            c.metal.clone(),
        ),
        [0.0, fore[1], (fore[2] + aft[2]) * 0.5],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
}

/// The shroud round the pusher, in the trim colour, and the three stators
/// that hold it off the tail cone ahead of the disc, the first straight up -
/// rigid off the shroud's station, so the boom stretch carries them with it:
/// on a stretched station of their own, the longest boom drew them onto the
/// shroud's front lip.
pub(super) fn shroud(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let ty = tail_y(plan);
    // A quarter turn about +X lays a cylinder's +Y along her.
    kids.push(prim(
        with_cut(
            cylinder(plan.len(DUCT_R), plan.len(DUCT_LEN), 28, c.trim.clone()),
            [0.0, 1.0],
            [0.0, 1.0],
            DUCT_HOLLOW,
        ),
        plan.at(0.0, ty, DUCT_Z),
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    let reach = DUCT_R * DUCT_HOLLOW + STATOR_IN;
    for k in 0..3 {
        let (s, cs) = (90.0 + 120.0 * k as f32).to_radians().sin_cos();
        let dz = STATOR_Z - DUCT_Z;
        kids.push(rod(
            plan.at_off(cs * STATOR_IN, ty + s * STATOR_IN, DUCT_Z, dz),
            plan.at_off(cs * reach, ty + s * reach, DUCT_Z, dz),
            dim(plan.len(STATOR_R)),
            c.body.clone(),
        ));
    }
}

/// Her nav lights on the fins' outer faces - red to port (her left, `+X`),
/// green to starboard - and the white light under the tail cone's end.
pub(super) fn lights(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let y = stab_y(plan);
    for (side, lamp) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        kids.push(prim(
            sphere(dim(plan.len(NAV_R)), 3, lamp.clone()),
            plan.at_off(side * (FIN_X + NAV_OUT), y + NAV_RISE, STAB_Z, -NAV_AFT),
            id_quat(),
        ));
    }
    kids.push(prim(
        sphere(dim(plan.len(TAIL_LIGHT_R)), 3, c.white.clone()),
        plan.at_off(
            0.0,
            plan.sill(TAIL_CONE_Z) + TAIL_LIGHT_RISE,
            TAIL_CONE_Z,
            TAIL_LIGHT_AHEAD,
        ),
        id_quat(),
    ));
}
