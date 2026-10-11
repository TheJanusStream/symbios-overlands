//! The landing gear: two skid tubes with up-turned toes, and two arched
//! cross tubes that reach up into the belly.
//!
//! The skid bottoms are the prototype frame's `y = 0`: the ground she rests
//! on, and the floor of her box (`helicopters::travel_drop`).

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::texture::SovereignMaterialSettings;

use super::super::super::common::{id_quat, prim, spine};
use super::super::{HelicopterColours, PodPlan, dim};

/// The skid tubes' radius and the cross tubes' (m, prototype frame).
pub(super) const SKID_R: f32 = 0.018;
const CROSS_R: f32 = 0.016;

/// The cross tubes' stations: under the front doors and the cabin's rear.
pub(super) const CROSS_Z: [f32; 2] = [0.40, -0.22];

/// Half the skid track (prototype frame): it grows with the cabin, so a
/// fuller machine stands on a wider stance.
pub(super) fn track(plan: &PodPlan) -> f32 {
    0.31 * plan.girth()
}

/// A tube along `pts`, at the prototype's coarse sampling.
fn tube(pts: &[([f32; 3], f32)], material: SovereignMaterialSettings) -> Generator {
    let mut kind = spine(pts, 10, material);
    if let GeneratorKind::Spine {
        samples_per_segment,
        ..
    } = &mut kind
    {
        *samples_per_segment = 6;
    }
    prim(kind, [0.0; 3], id_quat())
}

pub(super) fn skids(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let track = track(plan);
    let skid_r = dim(plan.len(SKID_R));
    for side in [-1.0f32, 1.0] {
        let x = side * track;
        let pts = [
            (x, SKID_R, -0.55),
            (x, SKID_R, 0.40),
            (x, 0.035, 0.62),
            (x, 0.10, 0.74),
        ]
        .map(|(x, y, z)| (plan.at(x, y, z), skid_r));
        kids.push(tube(&pts, c.metal.clone()));
    }
    let cross_r = dim(plan.len(CROSS_R));
    for z in CROSS_Z {
        // Up into the belly: 2 cm inside the keel, so the gear is carried.
        let belly = plan.sill(z) + 0.02;
        let pts = [
            (-track, 0.02),
            (-track * 0.9, 0.16),
            (-track * 0.55, belly),
            (track * 0.55, belly),
            (track * 0.9, 0.16),
            (track, 0.02),
        ]
        .map(|(x, y)| (plan.at(x, y, z), cross_r));
        kids.push(tube(&pts, c.metal.clone()));
    }
}
