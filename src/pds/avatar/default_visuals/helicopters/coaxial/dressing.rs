//! The ladder of secondary masses by ornateness - #1628 Q-F, agreed: an
//! adorned machine carries a sensor ball under her chin, its lens forward,
//! and a step on each skid; an ornate one stub wings low on her cabin with a
//! lamp pod at each tip, and a beacon on her fairing's top behind her mast.
//! Wear has no ladder of its own: it is the
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s grime on every
//! surface.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{
    cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, sweep,
};
use super::super::{CoaxialColours, PodPlan, dim};
use super::fuselage::{FAIRING, FAIRING_SINK, FAIRING_SQUASH};
use super::skids::{CROSS_Z, arch, track};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    c: &CoaxialColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    sensor(kids, plan, c);
    steps(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        stub_wings(kids, plan, c);
        beacon(kids, plan, c);
    }
}

/// The sensor ball (prototype frame): its station under the chin, how far
/// its middle hangs under the keel, its radius; and its lens - its radius,
/// thickness and how far ahead of the ball's middle it stands.
const SENSOR_Z: f32 = 1.18;
const SENSOR_DROP: f32 = 0.035;
const SENSOR_R: f32 = 0.065;
const LENS_R: f32 = 0.034;
const LENS_H: f32 = 0.012;
const LENS_AHEAD: f32 = 0.06;

/// A sensor ball under the chin, half in the keel, and its lens looking
/// forward.
fn sensor(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let y = plan.sill(SENSOR_Z) - SENSOR_DROP;
    kids.push(prim(
        sphere(dim(plan.len(SENSOR_R)), 4, c.metal.clone()),
        plan.at(0.0, y, SENSOR_Z),
        id_quat(),
    ));
    // A quarter turn about +X lays the lens's face forward.
    kids.push(prim(
        cylinder(
            dim(plan.len(LENS_R)),
            dim(plan.len(LENS_H)),
            16,
            c.glass.clone(),
        ),
        plan.at(0.0, y, SENSOR_Z + LENS_AHEAD),
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
}

/// A step plate's size (prototype frame).
const STEP: [f32; 3] = [0.08, 0.012, 0.10];

/// A step plate on each front cross tube's leg, starboard (`-X`) first,
/// straddling the leg a hand over the skid.
fn steps(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let z = CROSS_Z[0];
    let (x, y) = arch(track(plan), plan.sill(z) + 0.01)[1];
    for side in [-1.0f32, 1.0] {
        kids.push(prim(
            cuboid(
                [plan.len(STEP[0]), dim(plan.len(STEP[1])), plan.len(STEP[2])],
                c.metal.clone(),
            ),
            plan.at(side * x.abs(), y, z),
            id_quat(),
        ));
    }
}

/// The stub wings (prototype frame): their station, how far under the
/// cabin's centreline they cross it, how far past its flank each reaches,
/// their half-chord at the root and the tips, and their thickness over
/// chord.
const WING_Z: f32 = 0.40;
const WING_DROP: f32 = 0.12;
const WING_OUT: f32 = 0.30;
const WING_ROOT: f32 = 0.13;
const WING_TIP: f32 = 0.10;
const WING_THICK: f32 = 0.16;

/// A lamp pod at a wing's tip, `(dz from the wing's station, radius)` nose to
/// tail, and its lamp's radius at the pod's nose (prototype frame).
const POD: [(f32, f32); 5] = [
    (0.20, 0.015),
    (0.15, 0.040),
    (-0.02, 0.045),
    (-0.18, 0.030),
    (-0.24, 0.012),
];
const LAMP_R: f32 = 0.022;

/// How far out a stub wing reaches, each side, to its tip's middle
/// (prototype frame): past the cabin's flank where it crosses it.
fn wing_half(plan: &PodPlan) -> f32 {
    let y = plan.pod_at(WING_Z).0 - WING_DROP;
    plan.flank(WING_Z, y) + WING_OUT
}

/// The stub wings: one flattened sweep across the cabin, low, and a lamp
/// pod in the trim colour at each tip, its lamp at its nose - starboard
/// (`-X`) first.
fn stub_wings(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let y = plan.pod_at(WING_Z).0 - WING_DROP;
    let half = wing_half(plan);
    let wing: Vec<([f32; 3], f32)> = [(-half, WING_TIP), (0.0, WING_ROOT), (half, WING_TIP)]
        .iter()
        .map(|&(x, r)| {
            let [px, py, pz] = plan.at(x, y, WING_Z);
            ([px, py / WING_THICK, pz], plan.len(r))
        })
        .collect();
    let mut g = prim(sweep(&wing, 16, 4, c.body.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, WING_THICK, 1.0]);
    kids.push(g);
    for side in [-1.0f32, 1.0] {
        let x = side * half;
        let pod: Vec<([f32; 3], f32)> = POD
            .iter()
            .map(|&(dz, r)| (plan.at(x, y, WING_Z + dz), dim(plan.len(r))))
            .collect();
        kids.push(prim(
            sweep(&pod, 14, 4, c.trim.clone()),
            [0.0; 3],
            id_quat(),
        ));
        kids.push(prim(
            sphere(dim(plan.len(LAMP_R)), 3, c.white.clone()),
            plan.at(x, y, WING_Z + POD[0].0),
            id_quat(),
        ));
    }
}

/// The beacon (prototype frame): the fairing's station it stands on, behind
/// the mast, and its radius - half of which it sits down into the fairing's
/// top.
const BEACON_STATION: usize = 4;
const BEACON_R: f32 = 0.020;

/// The anti-collision beacon on the fairing's top behind the mast. Read off
/// the fairing's own station there, its radius and squash: stood on the
/// fuselage's crown instead, as first drawn, it was buried in the fairing.
fn beacon(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let (z, r) = FAIRING[BEACON_STATION];
    let top = plan.crown(z) - FAIRING_SINK + r * FAIRING_SQUASH;
    kids.push(prim(
        sphere(dim(plan.len(BEACON_R)), 3, c.beacon.clone()),
        plan.at(0.0, top + BEACON_R * 0.5, z),
        id_quat(),
    ));
}
