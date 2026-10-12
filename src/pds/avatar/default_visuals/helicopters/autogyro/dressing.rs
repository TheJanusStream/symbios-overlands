//! The ladder of secondary masses by ornateness - #1624 Q-G, agreed: the
//! airplanes' ladder, in the helicopter family. An adorned machine carries
//! a pitot on her port front pylon leg and a step on each shock leg; an
//! ornate one the biplane's wheel spats, grown with her tyre, and an
//! anti-collision beacon on her rotor's housing as well. Wear has no ladder
//! of its own: it is the
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s grime on every
//! surface.

use bevy::math::Vec3;

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{cuboid, id_quat, prim, rod, sphere, sweep, with_cut};
use super::super::{AutogyroColours, MIN_DIM, PodPlan, dim};
use super::{MAIN_Z, TRACK, WHEEL_R, gear, rotor};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    c: &AutogyroColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    pitot(kids, plan, c);
    steps(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        spats(kids, plan, c);
        beacon(kids, plan, c);
    }
}

/// How far up the port front pylon leg the pitot's root stands, as a share
/// of the leg from foot to top, and how far ahead of it the tip reaches
/// (prototype frame).
const PITOT_UP: f32 = 0.55;
const PITOT_AHEAD: f32 = 0.22;

/// `share` of the way from `a` to `b`.
fn lerp(a: [f32; 3], b: [f32; 3], share: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * share)
}

/// A pitot tube on the port (her left, `+X`) front pylon leg, pointing
/// ahead - its root in the leg.
fn pitot(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let (foot, top) = rotor::front_leg(plan, 1.0);
    let root = lerp(foot, top, PITOT_UP);
    let tip = [root[0], root[1], root[2] + plan.len(PITOT_AHEAD)];
    kids.push(rod(root, tip, dim(plan.len(MIN_DIM)), c.metal.clone()));
}

/// A step's size and how far down its shock leg it stands, as a share of
/// the leg from its top (prototype frame).
const STEP: [f32; 3] = [0.07, 0.012, 0.05];
const STEP_DOWN: f32 = 0.55;

/// A step on each shock leg, starboard first, its middle on the leg's line.
fn steps(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    for side in [-1.0f32, 1.0] {
        let (top, foot) = gear::shock_leg(plan, side);
        kids.push(prim(
            cuboid(
                [plan.len(STEP[0]), dim(plan.len(STEP[1])), plan.len(STEP[2])],
                c.metal.clone(),
            ),
            lerp(top, foot, STEP_DOWN),
            id_quat(),
        ));
    }
}

/// A wheel spat's stations along her, `(dz from the axle, radius)` - the
/// high-wing's and the biplane's, at their 0.155 m tyre - and how narrow it
/// is drawn.
const SPAT: [(f32, f32); 5] = [
    (0.24, 0.03),
    (0.12, 0.15),
    (0.0, 0.19),
    (-0.14, 0.17),
    (-0.28, 0.04),
];
const SPAT_TYRE: f32 = 0.155;
const SPAT_WIDTH: f32 = 0.5;

/// A spat over each main tyre in her fuselage's hue: the biplane's teardrop
/// grown with her tyre, kept above its axle - the tyre's lower half shows,
/// where it meets the ground. Not on the rolling hub: a spat stays put
/// while its tyre turns.
fn spats(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let grow = WHEEL_R / SPAT_TYRE;
    for side in [-1.0f32, 1.0] {
        let points: Vec<([f32; 3], f32)> = SPAT
            .iter()
            .map(|&(dz, r)| {
                let [x, y, z] = plan.at_off(side * TRACK, WHEEL_R, MAIN_Z, dz * grow);
                ([x / SPAT_WIDTH, y, z], plan.len(r * grow))
            })
            .collect();
        // The upper half: +X, the crown, -X - symmetric about the crown, so
        // the same whichever way round the sweep's frame turns.
        let mut g = prim(
            with_cut(
                sweep(&points, 20, 6, c.body.clone()),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        );
        g.transform.scale = Fp3([SPAT_WIDTH, 1.0, 1.0]);
        kids.push(g);
    }
}

/// The beacon on her rotor's housing: down its axis from the housing's
/// middle and aft of it, half sunk in its wall (prototype frame, along the
/// leaning axis).
const BEACON_DOWN: f32 = 0.02;
const BEACON_AFT: f32 = 0.06;

/// The anti-collision beacon, on the aft face of her rotor's housing.
fn beacon(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let off = rotor::lean() * Vec3::new(0.0, -plan.len(BEACON_DOWN), -plan.len(BEACON_AFT));
    let at = Vec3::from(rotor::housing_at(plan)) + off;
    kids.push(prim(
        sphere(dim(plan.len(0.018)), 3, c.beacon.clone()),
        at.to_array(),
        id_quat(),
    ));
}
