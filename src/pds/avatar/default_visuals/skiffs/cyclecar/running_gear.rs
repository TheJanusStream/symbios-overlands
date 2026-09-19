//! The cyclecar's running gear: the stub axles that carry the front pair, the
//! trailing fork and the spat over the single rear wheel, and the three
//! wheels.
//!
//! Every wheel is carried and every arm ends inside its rim, at the hub (the
//! roadster's missing axles were the epic's first complaint, #1364). A wheel
//! is two turned nodes, the buggy's: a tyre whose profile ends off the axis
//! is capped with a full disc (the disc-cap trap, #1359), so the rim stands
//! proud of BOTH its end caps - on a lit kit it is the brief's "turned disc
//! with an emissive face".

use crate::pds::avatar::livery::CyclecarColours;
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::{
    CyclecarPlan, FAIRING_CLEAR, FAIRING_DROP, NEAR, dim, fairing, line, outboard, rim_profile,
    side, solid, spat_cut_y, sweep, tyre_profile,
};

/// The stub axles' tube radius, as a fraction of the length.
const ARM_R: f32 = 0.0110;

/// The spat's half-width over the tyre's.
const SPAT_W: f32 = 2.10;

/// The front pair hang on swept stub axles: a lower and an upper arm a side,
/// each a V from two points inside the pod's flank out to the hub, ending
/// inside the rim.
pub(super) fn stub_axles(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let l = plan.length;
    let a = plan.axle(true);
    let half_track = plan.track * 0.5;
    let reach = plan.depth() * 0.85;
    for s in [-1.0f32, 1.0] {
        let hub_x = s * (half_track - 0.55 * a.w);
        for (dy, rr) in [(-0.012 * l, ARM_R), (0.020 * l, ARM_R * 0.85)] {
            let r = rr * l;
            let yy = (a.y + dy).clamp(-reach, reach);
            let inside = |dz: f32| ([s * plan.side_at(a.z + dz, yy) * 0.90, yy, a.z + dz], r);
            kids.push(line(
                &[
                    inside(-0.055 * l),
                    ([hub_x, a.y + dy, a.z], r),
                    inside(0.055 * l),
                ],
                6,
                &c.arm,
            ));
        }
    }
}

/// What carries the single wheel: a trailing fork out of the pod's belly to
/// the hub, one leg a side of the tyre, inside the spat.
pub(super) fn fork(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let l = plan.length;
    let a = plan.axle(false);
    // The pivot is toward the middle of the machine.
    let fwd = if a.z < 0.0 { 1.0 } else { -1.0 };
    let zp = a.z + fwd * a.r * 1.05;
    let yp = (plan.sill_at(zp) * 0.70).min(a.y + a.r * 0.5);
    for s in [-1.0f32, 1.0] {
        let x = s * a.w * 1.25;
        kids.push(line(
            &[([x, yp, zp], 0.010 * l), ([x, a.y, a.z], 0.010 * l)],
            6,
            &c.arm,
        ));
    }
}

/// The single wheel's spat: an upper half-pipe swept fore and aft over the
/// tyre on the family's [`fairing`] profile, its cut plane under the axle -
/// in the scheme, or a two-tone's second colour.
pub(super) fn spat(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let a = plan.axle(false);
    let y = spat_cut_y(plan);
    let pts: Vec<([f32; 3], f32)> = fairing()
        .iter()
        .map(|&(dz, k)| {
            debug_assert!(k > 0.0, "a spat station at {dz} has radius {k}");
            ([0.0, y, a.z + dz * a.r], dim(k * a.r))
        })
        .collect();
    let top = 1.0 + FAIRING_DROP + FAIRING_CLEAR;
    let sx = a.w * SPAT_W / (top * a.r);
    kids.push(sweep(
        &pts,
        16,
        [sx, 1.0, 1.0],
        [0.0, 0.5],
        0.0,
        c.lower.as_ref().unwrap_or(&c.body).clone(),
    ));
}

/// The three wheels on the plan's anchors, the front pair first and then the
/// single rear one. A worn cyclecar's near-front rim - the one the chase
/// quarter shows whole - is bare steel.
pub(super) fn wheels(
    kids: &mut Vec<Generator>,
    plan: &CyclecarPlan,
    c: &CyclecarColours,
    worn: bool,
) {
    for (at, r) in plan.wheels() {
        let w = r * super::TYRE_W;
        let lay = outboard(at[0]);
        let odd = worn && at[2] > 0.0 && side(at[0]) == NEAR;
        kids.push(solid(&tyre_profile(r, w), 28, true, &c.tyre, at, lay));
        let rim: &SovereignMaterialSettings = if odd { &c.odd_rim } else { &c.rim };
        kids.push(solid(&rim_profile(r, w), 24, false, rim, at, lay));
    }
}
