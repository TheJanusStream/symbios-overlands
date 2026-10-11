//! The ladder of secondary masses by ornateness - #1616 recommendation 8,
//! agreed: an adorned machine carries a pitot tube under her port wing and
//! a step on each front gear leg; an ornate one wheel spats over her mains
//! and an anti-collision beacon on her fin as well. Wear has no ladder of
//! its own: it is the [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s
//! grime on every surface, as the owner took it.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{
    cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, with_cut,
};
use super::super::{AirplaneColours, FusePlan, dim};
use super::tail::fin_top;
use super::{MAIN_Z, TRACK, WHEEL_R, WING_LE, sweep, wing_under};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &FusePlan,
    c: &AirplaneColours,
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

/// The pitot's span station under the port wing (her left, `+X`), its
/// length, and how far behind the leading edge its root reaches (prototype
/// frame): far enough back that the wing over it is thick enough to hold
/// it - six centimetres back, the elliptical section's underside had risen
/// clear of the tube.
const PITOT_X: f32 = 1.6;
const PITOT_LEN: f32 = 0.26;
const PITOT_ROOT: f32 = 0.15;

/// A pitot tube under the port wing, pointing ahead of the leading edge, its
/// root in the wing's underside.
fn pitot(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    kids.push(prim(
        cylinder(
            dim(plan.len(0.008)),
            plan.len(PITOT_LEN),
            8,
            c.metal.clone(),
        ),
        plan.at_off(
            PITOT_X,
            wing_under(plan, PITOT_X) + 0.005,
            WING_LE,
            PITOT_LEN * 0.5 - PITOT_ROOT,
        ),
        // A cylinder stands on its +Y; a quarter turn about +X lays it
        // along +Z, ahead.
        quat_xyzw(quat_x(90f32.to_radians())),
    ));
}

/// How far down each front gear leg its step stands, as a share of the leg
/// from belly to axle.
const STEP_ALONG: f32 = 0.6;

/// A step plate on each front gear leg, where a pilot climbs in.
fn steps(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    // The front leg's ends, as `gear::mains` draws it.
    let foot_z = MAIN_Z + 0.16;
    for side in [-1.0f32, 1.0] {
        let foot = plan.at(side * 0.10, plan.sill(foot_z) + 0.02, foot_z);
        let axle = plan.at(side * TRACK * 0.92, WHEEL_R + 0.01, MAIN_Z);
        let at: [f32; 3] = std::array::from_fn(|i| foot[i] + (axle[i] - foot[i]) * STEP_ALONG);
        kids.push(prim(
            cuboid(
                [plan.len(0.07), dim(plan.len(0.012)), plan.len(0.05)],
                c.metal.clone(),
            ),
            at,
            id_quat(),
        ));
    }
}

/// A wheel spat's stations along her, `(dz from the axle, radius)`, and how
/// narrow it is drawn (prototype frame).
const SPAT: [(f32, f32); 5] = [
    (0.24, 0.03),
    (0.12, 0.15),
    (0.0, 0.19),
    (-0.14, 0.17),
    (-0.28, 0.04),
];
const SPAT_WIDTH: f32 = 0.5;

/// A spat over each main tyre: a teardrop drawn half as wide as it is tall,
/// kept above its axle - the tyre's lower half shows, where it meets the
/// ground. Not on the rolling hub: a spat stays put while its tyre turns.
fn spats(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    for side in [-1.0f32, 1.0] {
        let points: Vec<([f32; 3], f32)> = SPAT
            .iter()
            .map(|&(dz, r)| {
                let [x, y, z] = plan.at(side * TRACK, WHEEL_R, MAIN_Z + dz);
                ([x / SPAT_WIDTH, y, z], plan.len(r))
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

/// The anti-collision beacon, seated down into the fin's top.
fn beacon(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    let (y, z) = fin_top(plan);
    kids.push(prim(
        sphere(dim(plan.len(0.018)), 3, c.beacon.clone()),
        plan.at(0.0, y - 0.005, z),
        id_quat(),
    ));
}
