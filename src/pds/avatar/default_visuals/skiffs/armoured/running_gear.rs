//! The armoured car's running gear: a boxy arch over each wheel, a beam
//! between each axle's hubs, the four wheels, the spare bolted to her stern
//! plate and the exhaust pipe down her near flank.
//!
//! A wheel is two turned nodes, the buggy's: a tyre whose profile ends off
//! the axis is capped with a full disc (the disc-cap trap, #1359), so the rim
//! has to stand proud of BOTH end caps.

use crate::pds::avatar::livery::ArmouredColours;
use crate::pds::generator::Generator;

use super::{
    ARCH_OUT, ArmouredPlan, NEAR, NO_TURN, along_z, line, outboard, plate, rim_profile, side,
    solid, tyre_profile,
};

/// The exhaust pipe's stations along the machine (of the length), forward to
/// aft, and where on the upper flank facet it runs.
const PIPE: [f32; 5] = [0.150, 0.0, -0.200, -0.380, -0.470];
const PIPE_T: f32 = 0.10;

/// Boxy arches: one squared box over each wheel, bedded into the hull's own
/// drawn flank, its outboard face past the tyre and its underside clear of
/// the tyre's crown.
///
/// The brief's "boxy wheel arches" - and a box over a round wheel is the one
/// shape that reads as armour rather than as a mudguard. **Bedded at the
/// arch's OWN height**, not at a fraction of the widest line: above the chine
/// the hexagon has already drawn in, and four arches cut to the chine hung in
/// the air at the first render.
pub(super) fn arches(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    for front in [true, false] {
        let a = plan.axle(front);
        let top = a.y + a.r * 1.10;
        let inner = plan.flank_x(a.z, top) - a.r * 0.26;
        let outer = plan.track * 0.5 + a.w * ARCH_OUT;
        for s in [-1.0f32, 1.0] {
            kids.push(plate(
                [outer - inner, a.r * 0.55, a.r * 2.20],
                &c.hull,
                [s * (outer + inner) * 0.5, top, a.z],
                NO_TURN,
                0.16,
            ));
        }
    }
}

/// A beam between each axle's hubs, dipping through the hull's belly - the
/// roadster's front beam and live rear axle.
///
/// It is what the wheels hang on, and what makes them part of the machine
/// rather than four discs beside it; at 22.9 degrees of look-down it is
/// hidden under the arch.
pub(super) fn axle_beams(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    for front in [true, false] {
        let a = plan.axle(front);
        let hub = plan.track * 0.5 - a.w * 0.30;
        let dip = plan.at(0.010);
        kids.push(line(
            &[
                ([-hub, a.y, a.z], plan.at(0.014)),
                ([-hub * 0.42, a.y - dip, a.z], plan.at(0.016)),
                ([hub * 0.42, a.y - dip, a.z], plan.at(0.016)),
                ([hub, a.y, a.z], plan.at(0.014)),
            ],
            6,
            &c.arm,
        ));
    }
}

/// The four wheels on the plan's anchors, the front pair first. A worn
/// machine's near-FRONT rim - the one the chase quarter shows whole - is bare
/// steel: a mismatched wheel, where the camera looks.
pub(super) fn wheels(
    kids: &mut Vec<Generator>,
    plan: &ArmouredPlan,
    c: &ArmouredColours,
    worn: bool,
) {
    for (at, r) in plan.wheels() {
        let w = r * super::TYRE_W;
        let lay = outboard(at[0]);
        let odd = worn && at[2] > 0.0 && side(at[0]) == NEAR;
        kids.push(solid(&tyre_profile(r, w), 20, true, &c.tyre, at, lay));
        let rim = if odd { &c.odd_rim } else { &c.rim };
        kids.push(solid(&rim_profile(r, w), 12, false, rim, at, lay));
    }
}

/// The spare wheel, bolted flat to the stern plate - the face the chase
/// camera at the usual quarter looks straight at, so it is where a spare
/// reads. It reaches a quarter of its own half-width INSIDE the plate: a
/// plate hull's stern is a plane, not a swept form's ball.
pub(super) fn spare(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let a = plan.axle(false);
    let (rs, ws) = (a.r * 0.92, a.w * 0.86);
    let zt = plan.tail_z() - ws * 0.75;
    let at = [0.0, plan.crown_at(plan.tail_z()) * 0.10, zt];
    kids.push(solid(
        &tyre_profile(rs, ws),
        20,
        true,
        &c.tyre,
        at,
        along_z(),
    ));
    kids.push(solid(
        &rim_profile(rs, ws),
        12,
        false,
        &c.rim,
        at,
        along_z(),
    ));
}

/// A pipe down the near flank under the arch line, its mouth aft - so the
/// aura she trails leaves a pipe that is actually drawn.
pub(super) fn exhaust(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let pts: Vec<([f32; 3], f32)> = PIPE
        .iter()
        .map(|&zf| {
            let z = plan.at(zf);
            let (x, y) = plan.flank(z, PIPE_T);
            (
                [NEAR * x * 1.02, y - plan.crown_at(z) * 0.30, z],
                plan.at(0.014),
            )
        })
        .collect();
    kids.push(line(&pts, 6, &c.arm));
}

/// Where that pipe's mouth is (root-local, m) - the mount every aura she can
/// pick issues from, whichever one the seed picked.
pub(super) fn pipe_mouth(plan: &ArmouredPlan) -> [f32; 3] {
    let z = plan.at(PIPE[PIPE.len() - 1]);
    let (x, y) = plan.flank(z, PIPE_T);
    [
        NEAR * x * 1.02,
        y - plan.crown_at(z) * 0.30,
        z - plan.at(0.010),
    ]
}
