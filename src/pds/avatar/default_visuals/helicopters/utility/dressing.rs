//! The ladder of secondary masses by ornateness - #1613 recommendation 7,
//! agreed: an adorned machine carries a pitot tube on her nose and a step on
//! each skid; an ornate one a searchlight under her cabin and a cargo hook
//! under her belly as well. Wear has no ladder of its own: it is the
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s grime on every
//! surface, as the owner took it.

use crate::pds::generator::Generator;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw};
use super::super::{HelicopterColours, PodPlan, dim};
use super::skids::{CROSS_Z, track};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    c: &HelicopterColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    pitot(kids, plan, c);
    steps(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        searchlight(kids, plan, c);
        hook(kids, plan, c);
    }
}

/// A pitot tube out of the nose, along `+Z`, its root buried in the nose
/// cone.
fn pitot(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let (y, _, _) = plan.pod_at(1.16);
    kids.push(prim(
        cylinder(dim(plan.len(0.008)), plan.len(0.14), 8, c.metal.clone()),
        plan.at(0.0, y + 0.015, 1.22),
        quat_xyzw(quat_x(90f32.to_radians())),
    ));
}

/// A step plate on each front cross tube's leg, inside the skid track.
fn steps(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let x = track(plan) * 0.88;
    for side in [-1.0f32, 1.0] {
        kids.push(prim(
            cuboid(
                [plan.len(0.09), dim(plan.len(0.012)), plan.len(0.07)],
                c.metal.clone(),
            ),
            plan.at(side * x, 0.165, CROSS_Z[0]),
            id_quat(),
        ));
    }
}

/// The searchlight's station: under the cabin's floor, on the keel just
/// aft of the chin windows.
const SEARCHLIGHT_Z: f32 = 0.78;

/// The searchlight: a housing half buried in the keel, aimed down and
/// ahead, and its lens - the fleet's lit window light.
fn searchlight(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    // A turn of 130 degrees about +X carries the housing's +Y to point down
    // and forward.
    let aim = 130f32.to_radians();
    let centre = (plan.sill(SEARCHLIGHT_Z) + 0.005, SEARCHLIGHT_Z);
    kids.push(prim(
        cylinder(dim(plan.len(0.045)), plan.len(0.07), 12, c.dark.clone()),
        plan.at(0.0, centre.0, centre.1),
        quat_xyzw(quat_x(aim)),
    ));
    let reach = 0.036;
    kids.push(prim(
        cylinder(
            dim(plan.len(0.038)),
            dim(plan.len(0.012)),
            12,
            c.lamp.clone(),
        ),
        plan.at(
            0.0,
            centre.0 + reach * aim.cos(),
            centre.1 + reach * aim.sin(),
        ),
        quat_xyzw(quat_x(aim)),
    ));
}

/// The cargo hook's station: under the mast, between the cross tubes.
const HOOK_Z: f32 = 0.10;

/// The cargo hook: a block half buried in the belly, and the hook below it.
fn hook(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let y = plan.sill(HOOK_Z) - 0.012;
    kids.push(prim(
        cuboid(
            [plan.len(0.06), plan.len(0.05), plan.len(0.09)],
            c.dark.clone(),
        ),
        plan.at(0.0, y, HOOK_Z),
        id_quat(),
    ));
    kids.push(prim(
        cylinder(dim(plan.len(0.012)), plan.len(0.05), 8, c.metal.clone()),
        plan.at(0.0, y - 0.045, HOOK_Z),
        id_quat(),
    ));
}
