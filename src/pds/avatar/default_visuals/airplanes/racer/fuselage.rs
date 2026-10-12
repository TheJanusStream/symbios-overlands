//! The fuselage: the one sweep and what is laid on it - the racing stripe
//! down each flank, nose to tail, and the canopy over the cockpit: a dark
//! bubble whose lower sides are lit (#1629 Q-B).
//!
//! The stripe is a skin, the family's ([`skin`]): a second sweep over the
//! fuselage's own stations, trimmed fore and aft by arc length and round the
//! section by angle (`0` is `+X`, `0.25` the crown, `0.5` is `-X`) - its two
//! bands mirror-symmetric, so the same whichever way round a sweep's frame
//! turns. The canopy is a sweep of its own, and its lit sides are laid on
//! ITS stations the same way.

use crate::pds::generator::Generator;

use super::super::super::common::{id_quat, prim, sweep, with_cut};
use super::super::{FUSE_RES, FusePlan, RacerColours, over_fuselage, skin};

/// The fuselage: one sweep, cowl face to tail post.
pub(super) fn fuselage(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    kids.push(over_fuselage(plan, 0.0, FUSE_RES, c.body.clone()));
}

/// The racing stripe down each flank, nose to tail, just over the widest:
/// the seed's accent, the fleet's identity slot.
pub(super) fn stripe(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    skin(
        kids,
        plan,
        (-1.36, 1.42),
        &[Some((0.005, 0.045)), Some((0.455, 0.495))],
        &c.stripe,
        0.003,
    );
}

/// The canopy (prototype frame): its stations `(z, rise of its centre over
/// the crown, radius)` nose to tail, a true racer's - a bubble whose lower
/// half sits in the fuselage - drawn [`CANOPY_SCALE`] as large in her full
/// proportion (#1629 Q-I), and its facets and samples.
const CANOPY: [(f32, f32, f32); 7] = [
    (0.62, -0.035, 0.030),
    (0.52, 0.000, 0.085),
    (0.38, 0.025, 0.115),
    (0.22, 0.030, 0.120),
    (0.06, 0.020, 0.105),
    (-0.10, -0.005, 0.075),
    (-0.24, -0.035, 0.035),
];
const CANOPY_SCALE: f32 = 1.10;
const CANOPY_RES: u32 = 24;
const CANOPY_SAMPLES: u32 = 6;

/// The canopy's lit sides: their bands round its section, each from just
/// over its widest to 40 degrees up - mirror-symmetric about its crown - and
/// how proud of the canopy they lie (prototype frame). Fore and aft they run
/// from its second station to its second-last.
const LIT: [(f32, f32); 2] = [(0.02, 0.11), (0.39, 0.48)];
const LIT_PROUD: f32 = 0.004;

/// The canopy's stations as sweep points in the drawn frame, each radius
/// `proud` (prototype metres) over its own.
fn canopy_points(plan: &FusePlan, proud: f32) -> Vec<([f32; 3], f32)> {
    CANOPY
        .iter()
        .map(|&(z, rise, r)| {
            (
                plan.at(0.0, plan.crown(z) + rise * CANOPY_SCALE, z),
                plan.len(r * CANOPY_SCALE + proud),
            )
        })
        .collect()
}

/// The canopy: a dark bubble over the cockpit, and its lower sides lit - the
/// fleet's lit window, where the trainer's side panes are.
pub(super) fn canopy(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let glass = canopy_points(plan, 0.0);
    // How far along the canopy's own line each station stands, as a share
    // of its length: what a skin's fore-and-aft trim reads.
    let mut run = vec![0.0f32];
    for w in glass.windows(2) {
        let [a, b] = [w[0].0, w[1].0];
        let d = ((a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        run.push(run[run.len() - 1] + d);
    }
    let total = run[run.len() - 1];
    let along = [run[1] / total, run[run.len() - 2] / total];
    kids.push(prim(
        sweep(&glass, CANOPY_RES, CANOPY_SAMPLES, c.glass.clone()),
        [0.0; 3],
        id_quat(),
    ));
    let lit = canopy_points(plan, LIT_PROUD);
    for (a, b) in LIT {
        kids.push(prim(
            with_cut(
                sweep(&lit, CANOPY_RES, CANOPY_SAMPLES, c.lamp.clone()),
                [a, b],
                along,
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
}
