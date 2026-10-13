//! The pylons and the masts (#1630 Q-A): a low hump on the cockpit's roof
//! and the front mast out of it; the tall aft pylon over the cabin's rear -
//! a rounded tower carrying her two engine pods on its flanks, in the trim
//! colour, and the rear mast on its top; and the white light on its tail.
//!
//! The aft pylon stands on a stretched station, and everything on it rides
//! it RIGIDLY ([`PodPlan::at_off`]): a longer boom carries the whole tower
//! aft and leaves it the shape the prototype drew. The rear mast stands on a
//! stretched station of its own, as the prototype drew it: 10 cm aft of the
//! tower's middle on the nominal boom, and 0.8 cm either side of that at the
//! boom's extremes.

use crate::pds::generator::Generator;

use super::super::super::common::{cylinder, id_quat, prim, sphere, superellipsoid, sweep};
use super::super::{PodPlan, TandemColours, dim};
use super::cabin_section;
use super::rotor::{FRONT_Z, REAR_Z, front_hub, rear_hub};

/// The hump on the cockpit's roof - the front pylon - (prototype frame):
/// its half-extents, and how far under the roof its middle stands.
const HUMP: [f32; 3] = [0.10, 0.07, 0.24];
const HUMP_SINK: f32 = 0.02;

/// The front mast's radius, and how far over the roof its foot stands,
/// inside the hump (prototype frame).
const FRONT_MAST_R: f32 = 0.035;
const FRONT_MAST_FOOT: f32 = 0.02;

/// The aft pylon (prototype frame): its half-extents - across her, up and
/// along her - its station, stretched, and how far over the roof its middle
/// stands.
pub(super) const PYLON: [f32; 3] = [0.12, 0.30, 0.40];
pub(super) const PYLON_Z: f32 = -1.10;
const PYLON_RISE: f32 = 0.16;

/// Both pylons' exponents, upright and across: rounded towers, boxier than
/// an ellipsoid. The upright one is what the beacon an ornate machine
/// stands on the aft pylon's back reads its surface by.
pub(super) const PYLON_NS: f32 = 0.6;
const PYLON_EW: f32 = 0.5;

/// The engine pods (prototype frame): their radius; how far out of the
/// pylon's middle each stands - sunk a centimetre into its flank - and how
/// far over its middle; and their stations, `(dz, radius)` nose to tail,
/// rigid off the pylon's station.
const ENGINE_R: f32 = 0.055;
const ENGINE_X: f32 = PYLON[0] + ENGINE_R - 0.01;
const ENGINE_RISE: f32 = 0.04;
const ENGINE: [(f32, f32); 6] = [
    (0.34, 0.035),
    (0.28, ENGINE_R),
    (0.10, ENGINE_R),
    (-0.10, ENGINE_R),
    (-0.26, ENGINE_R * 0.85),
    (-0.36, ENGINE_R * 0.7),
];

/// The rear mast's radius, and how far under the pylon's top its foot
/// stands (prototype frame).
const REAR_MAST_R: f32 = 0.04;
const REAR_MAST_FOOT: f32 = 0.02;

/// The tail light's radius, and how far inside the pylon's tail its middle
/// stands (prototype frame): it pokes out a centimetre, her aftmost part.
const TAIL_LIGHT_R: f32 = 0.020;
const TAIL_LIGHT_IN: f32 = 0.01;

/// The cabin's roof (prototype frame).
fn roof(plan: &PodPlan) -> f32 {
    let (cy, _, h) = cabin_section(plan);
    cy + h
}

/// The aft pylon's middle height (prototype frame).
pub(super) fn pylon_y(plan: &PodPlan) -> f32 {
    roof(plan) + PYLON_RISE
}

/// The hump over the cockpit and the front mast out of it, up into the
/// front rotor's head.
pub(super) fn front(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let top = roof(plan);
    kids.push(prim(
        superellipsoid(
            HUMP.map(|v| plan.len(v)),
            PYLON_NS,
            PYLON_EW,
            12,
            20,
            c.body.clone(),
        ),
        plan.at(0.0, top - HUMP_SINK, FRONT_Z),
        id_quat(),
    ));
    let (foot, hub) = (top + FRONT_MAST_FOOT, front_hub(plan));
    kids.push(prim(
        cylinder(
            dim(plan.len(FRONT_MAST_R)),
            plan.len(hub - foot),
            12,
            c.metal.clone(),
        ),
        plan.at(0.0, (hub + foot) * 0.5, FRONT_Z),
        id_quat(),
    ));
}

/// The aft pylon, its two engine pods - port first - and the rear mast out
/// of its top, up into the rear rotor's head.
pub(super) fn aft(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let py = pylon_y(plan);
    kids.push(prim(
        superellipsoid(
            PYLON.map(|v| plan.len(v)),
            PYLON_NS,
            PYLON_EW,
            20,
            28,
            c.body.clone(),
        ),
        plan.at(0.0, py, PYLON_Z),
        id_quat(),
    ));
    for side in [1.0f32, -1.0] {
        let (x, y) = (side * ENGINE_X, py + ENGINE_RISE);
        let pod: Vec<([f32; 3], f32)> = ENGINE
            .iter()
            .map(|&(dz, r)| (plan.at_off(x, y, PYLON_Z, dz), plan.len(r)))
            .collect();
        kids.push(prim(
            sweep(&pod, 18, 5, c.trim.clone()),
            [0.0; 3],
            id_quat(),
        ));
    }
    let (foot, hub) = (py + PYLON[1] - REAR_MAST_FOOT, rear_hub(plan));
    kids.push(prim(
        cylinder(
            dim(plan.len(REAR_MAST_R)),
            plan.len(hub - foot),
            12,
            c.metal.clone(),
        ),
        plan.at(0.0, (hub + foot) * 0.5, REAR_Z),
        id_quat(),
    ));
}

/// The white light on the aft pylon's tail.
pub(super) fn tail_light(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    kids.push(prim(
        sphere(dim(plan.len(TAIL_LIGHT_R)), 3, c.white.clone()),
        plan.at_off(0.0, pylon_y(plan), PYLON_Z, -PYLON[2] + TAIL_LIGHT_IN),
        id_quat(),
    ));
}

/// Her port engine pod's tail, in the drawn frame: its last station's
/// centre - where an exhaust aura leaves her (`Tandem::fx_mount`).
pub(super) fn exhaust_mouth(plan: &PodPlan) -> [f32; 3] {
    let (dz, _) = ENGINE[ENGINE.len() - 1];
    plan.at_off(ENGINE_X, pylon_y(plan) + ENGINE_RISE, PYLON_Z, dz)
}
