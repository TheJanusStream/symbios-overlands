//! The buggy's running gear: the suspension between the frame and the hub
//! anchors, and the wheels on them.
//!
//! Every wheel is carried - the roadster's missing axles were the owner's
//! first complaint of the epic (#1364). A front wheel hangs on a lower and
//! an upper A-arm and a red coilover; a rear wheel on a trailing arm, an
//! axle shaft out of the transaxle and a coilover off the rear stay. Every
//! arm ends inside its wheel's rim, at the hub: the long-travel look, fat
//! wheels standing clear of a narrow frame.
//!
//! A wheel is two turned nodes. **A Lathe whose profile ends off the axis is
//! capped with a full disc** (the disc-cap trap, #1359), so the tyre is a
//! drum, and the rim stands proud of BOTH its end caps: the far wheels'
//! inboard faces are seen through the open frame, and a rim that reads there
//! saves the roadster's inboard brake drum.

use crate::pds::avatar::livery::BuggyColours;
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::frame::Frame;
use super::{NEAR, line, outboard, rim_profile, side, solid, tyre_half_width, tyre_profile};

/// The front A-arms, the coilovers, the trailing arms and the axle shafts.
pub(super) fn suspension(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let arm = 0.0085 * l;
    for s in [-1.0f32, 1.0] {
        // FRONT: the lower and the upper A-arm, each a V off two rail joints
        // to the hub, and a coilover off the shoulder rail to the lower
        // arm's outer end.
        let hub_x = s * (f.half_track - 0.55 * f.front.w);
        let low = [hub_x, f.front.y - 0.010 * l, f.front.z];
        let high = [hub_x, f.front.y + 0.022 * l, f.front.z];
        kids.push(line(
            &[
                (f.lower(f.arm_fore, s), arm),
                (low, arm),
                (f.lower(f.arm_aft, s), arm),
            ],
            6,
            &c.frame,
        ));
        kids.push(line(
            &[
                (f.upper(f.arm_fore, s), arm * 0.9),
                (high, arm * 0.9),
                (f.upper(f.arm_aft, s), arm * 0.9),
            ],
            6,
            &c.frame,
        ));
        kids.push(line(
            &[(f.upper(f.front.z, s), 0.011 * l), (low, 0.012 * l)],
            6,
            &c.spring,
        ));
        // REAR: a trailing arm off the floor rail's pivot, the axle shaft
        // out of the transaxle, and a coilover off the rear stay.
        let hub = [s * (f.half_track - 0.55 * f.rear.w), f.rear.y, f.rear.z];
        let pivot = f.plan.hw(f.pivot_z);
        kids.push(line(
            &[
                (f.lower(f.pivot_z, s), 0.011 * l),
                (
                    [
                        s * (pivot + 0.35 * (f.half_track - pivot)),
                        (f.plan.floor(f.pivot_z) + f.rear.y) * 0.5,
                        f.rear.z + 0.080 * l,
                    ],
                    0.011 * l,
                ),
                (hub, 0.012 * l),
            ],
            6,
            &c.frame,
        ));
        kids.push(line(
            &[
                ([s * 0.030 * l, f.rear.y, f.rear.z], 0.0075 * l),
                (hub, 0.0075 * l),
            ],
            6,
            &c.alloy,
        ));
        kids.push(line(
            &[(f.stay_mid(s), 0.011 * l), (hub, 0.013 * l)],
            6,
            &c.spring,
        ));
    }
}

/// The four wheels on the plan's anchors, front axle first. A worn buggy's
/// near rear wheel - the one the chase quarter shows whole - rolls on the
/// mismatched rim.
pub(super) fn wheels(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours, worn: bool) {
    for (at, r) in f.plan.wheels() {
        let w = tyre_half_width(at[2], r);
        let lay = outboard(at[0]);
        let odd = worn && at[2] < 0.0 && side(at[0]) == NEAR;
        kids.push(solid(&tyre_profile(r, w), 28, true, &c.tyre, at, lay));
        let rim: &SovereignMaterialSettings = if odd { &c.odd_rim } else { &c.rim };
        kids.push(solid(&rim_profile(r, w), 24, false, rim, at, lay));
    }
}

/// A spare wheel of radius `r` and half-width `w` at `at`, turned by
/// `rotation`: the road wheel's profiles, a little coarser.
pub(super) fn spare(
    kids: &mut Vec<Generator>,
    c: &BuggyColours,
    at: [f32; 3],
    r: f32,
    w: f32,
    rotation: [f32; 4],
) {
    kids.push(solid(&tyre_profile(r, w), 24, true, &c.tyre, at, rotation));
    kids.push(solid(&rim_profile(r, w), 20, false, &c.rim, at, rotation));
}
