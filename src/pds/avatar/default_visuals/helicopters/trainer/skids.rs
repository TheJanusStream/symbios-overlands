//! The landing gear: two tall skid tubes with up-turned toes, and two
//! arched cross tubes - the front one up into the bubble's floor, the aft
//! one up to the engine block's foot.
//!
//! The skid bottoms are the prototype frame's `y = 0`: the ground she rests
//! on, and the floor of her box (`helicopters::travel_drop`).

use crate::pds::generator::Generator;

use super::super::super::common::{id_quat, prim, sweep};
use super::super::{PodPlan, TrainerColours, dim};
use super::bay::ENGINE_FOOT;
use super::bubble::BULKHEAD_Z;

/// The skid tubes' radius and the cross tubes' (m, prototype frame).
pub(super) const SKID_R: f32 = 0.016;
const CROSS_R: f32 = 0.014;

/// The cross tubes' stations: under the bubble and under the engine.
pub(super) const CROSS_Z: [f32; 2] = [1.20, 0.62];

/// Half the skid track over her girth: the prototype's 0.36 at the nominal
/// 1.22, growing with the bubble so the skids stand outboard of it at every
/// girth, as the utility's track grows with her cabin.
const TRACK_PER_GIRTH: f32 = 0.36 / 1.22;

/// Half the skid track (prototype frame).
pub(super) fn track(plan: &PodPlan) -> f32 {
    TRACK_PER_GIRTH * plan.girth()
}

/// The skid tube's stations, `(z, y)` tail to toe. No gap is over twice its
/// neighbour's: one long run beside the toe's short ones dipped the tube
/// under the ground ahead of the bend (a Spine is a UNIFORM Catmull-Rom,
/// #1616). The toe turns up short of the bubble's nose, which is her box's.
const SKID: [(f32, f32); 7] = [
    (-0.12, SKID_R),
    (0.30, SKID_R),
    (0.72, SKID_R),
    (1.10, SKID_R),
    (1.32, 0.026),
    (1.48, 0.060),
    (1.58, 0.110),
];

pub(super) fn skids(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let track = track(plan);
    let skid_r = dim(plan.len(SKID_R));
    for side in [-1.0f32, 1.0] {
        let pts = SKID.map(|(z, y)| (plan.at(side * track, y, z), skid_r));
        kids.push(prim(
            sweep(&pts, 10, 6, c.metal.clone()),
            [0.0; 3],
            id_quat(),
        ));
    }
    let cross_r = dim(plan.len(CROSS_R));
    for z in CROSS_Z {
        // Up into the bubble's floor, or to the engine's foot; a station at
        // the middle keeps the long span across from looping past it.
        let top = if z > BULKHEAD_Z {
            plan.sill(z)
        } else {
            ENGINE_FOOT
        };
        let pts = [
            (-track, 0.02),
            (-track * 0.92, 0.17),
            (-track * 0.55, top),
            (0.0, top),
            (track * 0.55, top),
            (track * 0.92, 0.17),
            (track, 0.02),
        ]
        .map(|(x, y)| (plan.at(x, y, z), cross_r));
        kids.push(prim(
            sweep(&pts, 10, 6, c.metal.clone()),
            [0.0; 3],
            id_quat(),
        ));
    }
}
