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
use super::{NEAR, line, outboard, side, solid, tyre_half_width};

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

/// A fat off-road tyre of radius `r` and half-width `w`: squared shoulders
/// and a single crown point at `r`, so the drawn tyre reaches exactly the
/// axle's radius and stands on the ground. Its end caps are solid discs at
/// `0.86 w`.
fn tyre_profile(r: f32, w: f32) -> [(f32, f32); 9] {
    let lip = r * 0.58;
    [
        (lip, -w * 0.86),
        (r * 0.80, -w),
        (r * 0.95, -w * 0.93),
        (r * 0.995, -w * 0.55),
        (r, 0.0),
        (r * 0.995, w * 0.55),
        (r * 0.95, w * 0.93),
        (r * 0.80, w),
        (lip, w * 0.86),
    ]
}

/// A wide mag rim standing proud of the tyre's end caps on BOTH faces, at
/// `1.10 w`.
fn rim_profile(r: f32, w: f32) -> [(f32, f32); 10] {
    let lip = r * 0.58;
    [
        (0.0, -w * 1.10),
        (lip * 0.30, -w * 1.06),
        (lip * 0.42, -w * 0.92),
        (lip * 1.00, -w * 0.90),
        (lip * 1.03, -w * 0.50),
        (lip * 1.03, w * 0.50),
        (lip * 1.00, w * 0.90),
        (lip * 0.42, w * 0.92),
        (lip * 0.30, w * 1.06),
        (0.0, w * 1.10),
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A tyre reaches exactly its radius and no further, so with its axle one
    /// radius over the ground it stands ON the ground; and the rim stands
    /// proud of both of the tyre's end caps, or the drum swallows it.
    #[test]
    fn the_tyre_meets_the_ground_and_the_rim_stands_proud() {
        for r in [0.2f32, 0.31, 0.37, 0.5] {
            for w in [r * 0.26, r * 0.38] {
                let tyre = tyre_profile(r, w);
                let widest = tyre.iter().map(|&(x, _)| x).fold(0.0f32, f32::max);
                assert!(
                    (widest - r).abs() < 1e-6,
                    "the tyre is {widest} across a {r} wheel"
                );
                let cap = tyre[0].1.abs().max(tyre[tyre.len() - 1].1.abs());
                let rim = rim_profile(r, w);
                assert!(
                    rim[0].1 < -cap && rim[rim.len() - 1].1 > cap,
                    "the rim is inside the tyre's end caps at {cap}"
                );
            }
        }
    }
}
