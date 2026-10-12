//! The landing gear: two skid tubes with up-turned toes, and two cross
//! tubes rising from them and rounding into the cabin's belly.
//!
//! The skid bottoms are the prototype frame's `y = 0`: the ground she rests
//! on, and the floor of her box (`helicopters::travel_drop`). The skids run
//! aft past the cabin's rear, where the boom stretch starts, so every point
//! of them is drawn rigid off that station ([`cabin`]).

use crate::pds::generator::Generator;

use super::super::super::common::{id_quat, prim, sweep};
use super::super::{CoaxialColours, PodPlan, dim};
use super::cabin;

/// The skid tubes' radius and the cross tubes' (m, prototype frame).
const SKID_R: f32 = 0.017;
const CROSS_R: f32 = 0.015;

/// The cross tubes' stations: under the cabin's front and its rear.
pub(super) const CROSS_Z: [f32; 2] = [0.82, 0.10];

/// A skid's stations, `(height, station)` tail to toe: straight along the
/// ground, evenly spaced so no gap is over twice its neighbour's, and up
/// into its toe.
const SKID: [(f32, f32); 7] = [
    (SKID_R, -0.36),
    (SKID_R, 0.06),
    (SKID_R, 0.48),
    (SKID_R, 0.90),
    (0.032, 1.12),
    (0.072, 1.27),
    (0.120, 1.36),
];

/// How many stations each half of a cross tube's arch has past its skid,
/// and how high up its rise the curve's second control point stands, as a
/// share of the belly's height.
const ARCH_STEPS: usize = 5;
const ARCH_RISE: f32 = 0.80;

/// How finely a cross tube's curve is sampled to space its stations.
const ARCH_SAMPLES: usize = 200;

/// Half the skid track (prototype frame): it grows with the cabin, so a
/// fuller machine stands on a wider stance.
pub(super) fn track(plan: &PodPlan) -> f32 {
    0.34 * plan.girth()
}

/// A cross tube's stations `(x, y)`, skid to skid, `belly` its top: up from
/// one skid's line and round into the belly along a cubic, then down to the
/// other, evenly spaced along the curve so that no gap is over twice its
/// neighbour's (a sweep is a uniform Catmull-Rom, #1616) - the prototype's
/// `arch`. Its second station stands on the skid's leg.
pub(super) fn arch(track: f32, belly: f32) -> Vec<(f32, f32)> {
    let p = [
        (-track, 0.02),
        (-track, belly * ARCH_RISE),
        (-track * 0.55, belly),
        (0.0, belly),
    ];
    let at = |u: f32| {
        let w = [
            (1.0 - u).powi(3),
            3.0 * u * (1.0 - u).powi(2),
            3.0 * u * u * (1.0 - u),
            u.powi(3),
        ];
        (
            w.iter().zip(p).map(|(w, q)| w * q.0).sum::<f32>(),
            w.iter().zip(p).map(|(w, q)| w * q.1).sum::<f32>(),
        )
    };
    let fine: Vec<(f32, f32)> = (0..=ARCH_SAMPLES)
        .map(|k| at(k as f32 / ARCH_SAMPLES as f32))
        .collect();
    let mut run = vec![0.0f32];
    for w in fine.windows(2) {
        let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        run.push(run[run.len() - 1] + d);
    }
    let total = run[run.len() - 1];
    let half: Vec<(f32, f32)> = (0..=ARCH_STEPS)
        .map(|k| {
            let goal = total * k as f32 / ARCH_STEPS as f32;
            let i = (0..run.len())
                .min_by(|&a, &b| (run[a] - goal).abs().total_cmp(&(run[b] - goal).abs()))
                .unwrap_or(0);
            fine[i]
        })
        .collect();
    let mut out = half.clone();
    out.extend(half[..ARCH_STEPS].iter().rev().map(|&(x, y)| (-x, y)));
    out
}

/// The skids, starboard (`-X`) first, and the two cross tubes, the front
/// first.
pub(super) fn skids(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let track = track(plan);
    let skid_r = dim(plan.len(SKID_R));
    for side in [-1.0f32, 1.0] {
        let pts: Vec<([f32; 3], f32)> = SKID
            .iter()
            .map(|&(y, z)| (cabin(plan, side * track, y, z), skid_r))
            .collect();
        kids.push(prim(
            sweep(&pts, 10, 6, c.metal.clone()),
            [0.0; 3],
            id_quat(),
        ));
    }
    let cross_r = dim(plan.len(CROSS_R));
    for z in CROSS_Z {
        // Up into the belly: a centimetre inside the keel, so the gear is
        // carried.
        let belly = plan.sill(z) + 0.01;
        let pts: Vec<([f32; 3], f32)> = arch(track, belly)
            .into_iter()
            .map(|(x, y)| (cabin(plan, x, y, z), cross_r))
            .collect();
        kids.push(prim(
            sweep(&pts, 10, 4, c.metal.clone()),
            [0.0; 3],
            id_quat(),
        ));
    }
}
