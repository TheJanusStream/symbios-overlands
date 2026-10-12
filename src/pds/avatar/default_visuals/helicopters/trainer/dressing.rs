//! The ladder of secondary masses by ornateness - #1621 Q-F, agreed: an
//! adorned trainer carries a searchlight under her chin and a step on each
//! skid; an ornate one the ambulance's litter pods over her skids as well.
//! Wear has no ladder of its own: it is the
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s grime on every
//! surface, as on the utility.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, rod, sweep};
use super::super::{PodPlan, TrainerColours, dim};
use super::skids::{CROSS_Z, track};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    c: &TrainerColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    searchlight(kids, plan, c);
    steps(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        litter_pods(kids, plan, c);
    }
}

/// The searchlight's station under the bubble's front, and its aim
/// (degrees about `+X`: a turn of 130 carries the housing's `+Y` forward and
/// 40 degrees down, so its lens looks ahead of her and down - the utility's
/// aim). At `1.32` the bubble's drawn floor is its sill to 3 mm at every
/// girth; at the prototype's `1.42` the fullest bubble's forward rings,
/// tilted up its climbing centreline, stood its floor 3 cm over the sill,
/// and the light hung off it.
const SEARCHLIGHT_Z: f32 = 1.32;
const SEARCHLIGHT_AIM_DEG: f32 = 130.0;

/// The searchlight: a housing half buried in the bubble's floor - its
/// centre 5 mm inside it, as the utility's is in her keel - and its lens on
/// the housing's forward face: the fleet's lit window light, as the
/// utility's is.
fn searchlight(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let aim = SEARCHLIGHT_AIM_DEG.to_radians();
    let turn = quat_xyzw(quat_x(aim));
    let centre = (plan.sill(SEARCHLIGHT_Z) + 0.005, SEARCHLIGHT_Z);
    kids.push(prim(
        cylinder(plan.len(0.035), plan.len(0.05), 14, c.dark.clone()),
        plan.at(0.0, centre.0, centre.1),
        turn,
    ));
    // A turn of `aim` about +X carries the housing's +Y to (cos, sin) in
    // (y, z): the lens stands on its forward face.
    let reach = 0.026;
    kids.push(prim(
        cylinder(plan.len(0.030), dim(plan.len(0.006)), 14, c.lamp.clone()),
        plan.at(
            0.0,
            centre.0 + reach * aim.cos(),
            centre.1 + reach * aim.sin(),
        ),
        turn,
    ));
}

/// A step plate on each front cross tube's leg, just inside the skid.
fn steps(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let x = track(plan) - 0.02;
    for side in [-1.0f32, 1.0] {
        kids.push(prim(
            cuboid(
                [plan.len(0.06), dim(plan.len(0.010)), plan.len(0.10)],
                c.metal.clone(),
            ),
            plan.at(side * x, 0.13, CROSS_Z[0]),
            id_quat(),
        ));
    }
}

/// A litter pod's capsule, drawn 85 % as tall as wide, as the prototype's:
/// the node scale squashes the sweep's section, so its stations are stored
/// over it. Its stations, `(z, radius)` nose to tail, at the height the pods
/// ride at and how far outboard of the skids.
const POD_SQUASH: f32 = 0.85;
const POD: [(f32, f32); 6] = [
    (1.36, 0.02),
    (1.30, 0.062),
    (1.05, 0.070),
    (0.80, 0.070),
    (0.58, 0.062),
    (0.52, 0.02),
];
const POD_Y: f32 = 0.25;
const POD_OUT: f32 = 0.07;

/// The pod on `side` (`+X` or `-X`): its cover in `material`.
fn pod(plan: &PodPlan, side: f32, material: SovereignMaterialSettings) -> Generator {
    let x = side * (track(plan) + POD_OUT);
    let pts = POD.map(|(z, r)| {
        let [px, py, pz] = plan.at(x, POD_Y, z);
        ([px, py / POD_SQUASH, pz], plan.len(r))
    });
    let mut pod = prim(sweep(&pts, 18, 6, material), [0.0; 3], id_quat());
    pod.transform.scale = Fp3([1.0, POD_SQUASH, 1.0]);
    pod
}

/// How far out the litter pods are drawn (m, drawn frame): their capsules'
/// outboard faces, read off the mesher's own vertices of the very sweep
/// [`litter_pods`] draws - what an ornate trainer's box is as wide as. The
/// node's squash scales `y` alone.
pub(super) fn pods_half_width(plan: &PodPlan) -> f32 {
    let g = pod(plan, 1.0, SovereignMaterialSettings::default());
    crate::world_builder::primitive_points(&g.kind)
        .into_iter()
        .map(|p| p.x.abs())
        .fold(0.0, f32::max)
}

/// The litter pods: a long turned capsule outboard of each skid, its cover
/// in the body colour, carried on a strut down to each cross tube's leg.
fn litter_pods(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let track = track(plan);
    for side in [-1.0f32, 1.0] {
        kids.push(pod(plan, side, c.body.clone()));
        let x = side * (track + POD_OUT);
        for z in CROSS_Z {
            kids.push(rod(
                plan.at(x, 0.20, z),
                plan.at(side * track * 0.95, 0.10, z),
                dim(plan.len(0.008)),
                c.metal.clone(),
            ));
        }
    }
}
