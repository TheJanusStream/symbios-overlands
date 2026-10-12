//! The fuselage: the one sweep, the skins laid on it - the cheat line, the
//! cowl's nose ring, the windscreen and the lit cabin windows - the airscrew
//! on its spinner, and the cowl's exhaust stack and cylinder heads.
//!
//! # Skins are laid on the fuselage's own stations
//!
//! Every skin is a second sweep over the fuselage's OWN stations, a few
//! millimetres proud, with its own section scale - trimmed fore and aft by
//! arc length (`profile_cut`, [`FusePlan::along`]) and round the section by
//! angle (`path_cut`), the helicopter's method (#1613), and the family's
//! since the biplane (`super::super::skin`, #1623). Each set of angular
//! bands is mirror-symmetric (`0` is `+X`, `0.25` the crown, `0.5` is `-X`,
//! `0.75` the keel), so it is the same whichever way round a sweep's frame
//! turns.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::generator::Generator;
use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z, with_shape,
};
use super::super::{AirplaneColours, FUSE_RES, FusePlan, dim, over_fuselage, skin};
use super::hub;

/// How far the airscrew's hub stands INTO the cowl's face, and the
/// spinner's length ahead of it (prototype frame) - together, where her
/// airframe's length starts.
pub(super) const HUB_INSET: f32 = 0.005;
pub(super) const SPINNER_LEN: f32 = 0.14;

/// The airscrew's spin (#1616 recommendation 5): idling at 540 degrees a
/// second, and 55 more for each metre a second she travels - which reaches
/// the runtime's ceiling, four turns a second, at her cruise, past which it
/// holds.
pub(crate) const PROP_IDLE_DEG_S: f32 = 540.0;
pub(crate) const PROP_GAIN_DEG_PER_M: f32 = 55.0;

/// The airscrew's blades.
const PROP_BLADES: f32 = 2.0;

/// How many times a second a blade passes a point at her cruise, where her
/// airscrew turns at the ceiling: what her voice's thrum is gated at
/// (`fx::propeller_drone`), so what is heard is what is seen flying.
pub(crate) const BLADE_PASS_HZ: f32 = PROP_BLADES * MAX_SPIN_RATE_DEG / 360.0;

/// The exhaust stack's station, rise over the belly, its rake (degrees,
/// about `+X`; positive tips its top forward) and its length.
const EXHAUST_Z: f32 = 1.16;
const EXHAUST_RISE: f32 = 0.02;
const EXHAUST_RAKE_DEG: f32 = 30.0;
const EXHAUST_LEN: f32 = 0.12;
const EXHAUST_X: f32 = 0.07;

/// The fuselage: one sweep, spinner to tail post.
pub(super) fn fuselage(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    kids.push(over_fuselage(plan, 0.0, FUSE_RES, c.body.clone()));
}

/// Every skin, in the prototype's order.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    // The cheat line: a stripe down each flank, cowl to tail - the seed's
    // accent, the fleet's identity slot.
    skin(
        kids,
        plan,
        (-1.30, 1.40),
        &[Some((0.035, 0.075)), Some((0.425, 0.465))],
        &c.stripe,
        0.003,
    );
    // The cowl's nose ring.
    skin(kids, plan, (1.36, 1.47), &[None], &c.trim, 0.004);
    // The windscreen across the crown, a centre post left between its panes.
    skin(
        kids,
        plan,
        (0.80, 1.06),
        &[Some((0.07, 0.24)), Some((0.26, 0.43))],
        &c.glass,
        0.006,
    );
    // The cabin windows, lit: a door window and a rear one, each side.
    for run in [(0.47, 0.78), (0.14, 0.43)] {
        skin(
            kids,
            plan,
            run,
            &[Some((0.05, 0.18)), Some((0.32, 0.45))],
            &c.lamp,
            0.006,
        );
    }
}

/// The spinner's silhouette, `(radius at girth 1, height)` base to tip
/// along its axis (prototype frame).
const SPINNER: [(f32, f32); 5] = [
    (0.09, 0.0),
    (0.085, 0.04),
    (0.06, 0.09),
    (0.025, 0.125),
    (0.0, SPINNER_LEN),
];

/// An airscrew blade's size (width, length, thickness), how far its middle
/// stands out from the hub and forward of it, how far it narrows toward its
/// tip, and its pitch (degrees) - all prototype frame.
const BLADE: [f32; 3] = [0.06, 0.40, 0.014];
const BLADE_OUT: f32 = 0.22;
const BLADE_FORWARD: f32 = 0.03;
const BLADE_TAPER: f32 = 0.45;
const BLADE_PITCH_DEG: f32 = 18.0;

/// The spinner and the two-blade airscrew on a hidden hub at the cowl's
/// face: blades along `y`, turning about `z` with her speed, idling at rest.
pub(super) fn airscrew(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    let (face, yc, _) = plan.stations().next().expect("the fuselage has stations");
    let points: Vec<(f32, f32)> = SPINNER
        .iter()
        .map(|&(r, h)| (plan.len(r * plan.girth()), plan.len(h)))
        .collect();
    let mut parts = vec![prim(
        lathe(&points, 20, true, c.metal.clone()),
        [0.0; 3],
        // A lathe turns about its +Y; a quarter turn about +X lays that
        // along +Z, her nose.
        quat_xyzw(quat_x(FRAC_PI_2)),
    )];
    for side in [1.0f32, -1.0] {
        // Each blade built pointing +Y and narrowing toward that tip; the
        // lower one turned half round about the shaft, so its tip is down.
        let turn = if side > 0.0 { 0.0 } else { PI };
        let blade = with_shape(
            cuboid(
                [
                    plan.len(BLADE[0]),
                    plan.len(BLADE[1]),
                    dim(plan.len(BLADE[2])),
                ],
                c.dark.clone(),
            ),
            [BLADE_TAPER, 0.0],
            [0.0; 3],
            [0.0; 2],
        );
        parts.push(prim(
            blade,
            [0.0, side * plan.len(BLADE_OUT), plan.len(BLADE_FORWARD)],
            quat_xyzw(quat_mul(quat_z(turn), quat_y(BLADE_PITCH_DEG.to_radians()))),
        ));
    }
    let mut screw = hub(plan, plan.at(0.0, yc, face - HUB_INSET), c);
    screw.children = parts;
    screw.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Speed {
            idle: Fp(PROP_IDLE_DEG_S),
            gain: Fp(PROP_GAIN_DEG_PER_M),
        },
    ));
    kids.push(screw);
}

/// The exhaust stack under the cowl, raked forward, and a cylinder head
/// standing out of each of the cowl's cheeks.
pub(super) fn cowl_dressing(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    kids.push(prim(
        cylinder(
            dim(plan.len(0.022)),
            plan.len(EXHAUST_LEN),
            12,
            c.dark.clone(),
        ),
        plan.at(EXHAUST_X, plan.sill(1.20) + EXHAUST_RISE, EXHAUST_Z),
        quat_xyzw(quat_x(EXHAUST_RAKE_DEG.to_radians())),
    ));
    const HEAD_Z: f32 = 1.24;
    let y = plan.fuse_at(HEAD_Z).0 + 0.02;
    for side in [1.0f32, -1.0] {
        kids.push(prim(
            cylinder(
                dim(plan.len(0.038 * plan.girth())),
                plan.len(0.07),
                12,
                c.dark.clone(),
            ),
            plan.at(side * (plan.flank(HEAD_Z, y) + 0.01), y, HEAD_Z),
            quat_xyzw(quat_z(FRAC_PI_2)),
        ));
    }
}

/// The stack's mouth, in the prototype frame: its lower end, the stack's
/// own half-length along its raked axis from its centre - where an exhaust
/// aura leaves her (`HighWing::fx_mount`).
pub(super) fn exhaust_mouth(plan: &FusePlan) -> (f32, f32, f32) {
    let rake = EXHAUST_RAKE_DEG.to_radians();
    let half = EXHAUST_LEN * 0.5;
    // A turn of `rake` about +X carries the stack's +Y toward +Z by its
    // sine, so its -Y end - the mouth, under the cowl - goes aft.
    (
        EXHAUST_X,
        plan.sill(1.20) + EXHAUST_RISE - half * rake.cos(),
        EXHAUST_Z - half * rake.sin(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Her voice is gated at what her airscrew does at her cruise: two
    /// blades at the ceiling a speed term's rate is held to.
    #[test]
    fn the_blade_pass_is_her_airscrew_at_the_ceiling() {
        assert_eq!(BLADE_PASS_HZ, 8.0);
        // And the ceiling is where her cruise puts it: 1.5 times her stall
        // speed, as the agent's pilot flies her (11 m/s on the preset).
        let cruise = 11.0 * 1.5;
        let rate = PROP_IDLE_DEG_S + PROP_GAIN_DEG_PER_M * cruise;
        assert!(
            (rate - MAX_SPIN_RATE_DEG).abs() < 0.02 * MAX_SPIN_RATE_DEG,
            "at her cruise the airscrew asks {rate} degrees a second"
        );
    }
}
