//! The ladder of secondary masses by ornateness - #1630 Q-F, agreed: an
//! adorned machine carries a step under her starboard door behind the
//! cockpit and a rescue hoist over it - a boom out from her roof, its winch,
//! and a cable down to a hook; an ornate one a refuelling probe out from her
//! starboard cheek past her nose, and a beacon on her aft pylon's back.
//! Wear has no ladder of its own: it is the
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s grime on every
//! surface.

use crate::pds::generator::Generator;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{cuboid, id_quat, prim, rod, sphere, sweep};
use super::super::{MIN_DIM, PodPlan, TandemColours, dim};
use super::cabin_section;
use super::pylons::{PYLON, PYLON_NS, PYLON_Z, pylon_y};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    c: &TandemColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    step(kids, plan, c);
    hoist(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        probe(kids, plan, c);
        beacon(kids, plan, c);
    }
}

/// The step (prototype frame): its size, how far outside the cabin's flank
/// its middle stands and how far over her belly, and its station.
const STEP: [f32; 3] = [0.10, 0.012, 0.14];
const STEP_OUT: f32 = 0.03;
const STEP_RISE: f32 = 0.02;
const STEP_Z: f32 = 0.80;

/// A step plate under her starboard (`-X`) door, behind the cockpit.
fn step(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (cy, w, h) = cabin_section(plan);
    kids.push(prim(
        cuboid(
            [plan.len(STEP[0]), dim(plan.len(STEP[1])), plan.len(STEP[2])],
            c.metal.clone(),
        ),
        plan.at(-(w + STEP_OUT), cy - h + STEP_RISE, STEP_Z),
        id_quat(),
    ));
}

/// The rescue hoist over her starboard door (prototype frame): its station;
/// its boom's foot, inside the flank and under the roof, and its head,
/// outside the flank and over the roof, and the boom's radius; the winch's
/// size at the head; how far over her centreline the cable ends, and the
/// hook's size and height there.
const HOIST_Z: f32 = 0.78;
const HOIST_FOOT: [f32; 2] = [-0.04, -0.02];
const HOIST_HEAD: [f32; 2] = [0.16, 0.06];
const HOIST_R: f32 = 0.018;
const WINCH: [f32; 3] = [0.06, 0.05, 0.07];
const CABLE_END: f32 = -0.02;
const HOOK: [f32; 3] = [0.03, 0.04, 0.02];
const HOOK_Y: f32 = -0.03;

/// The hoist's boom head (prototype frame): outside her starboard flank,
/// over her roof, where the winch hangs.
pub(super) fn hoist_head(plan: &PodPlan) -> [f32; 3] {
    let (cy, w, h) = cabin_section(plan);
    [-(w + HOIST_HEAD[0]), cy + h + HOIST_HEAD[1], HOIST_Z]
}

/// The hoist: its boom out from her roof, the winch at its head, and the
/// cable down to the hook.
fn hoist(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (cy, w, h) = cabin_section(plan);
    let [x, y, z] = hoist_head(plan);
    kids.push(rod(
        plan.at(-(w + HOIST_FOOT[0]), cy + h + HOIST_FOOT[1], z),
        plan.at(x, y, z),
        dim(plan.len(HOIST_R)),
        c.metal.clone(),
    ));
    kids.push(prim(
        cuboid(WINCH.map(|v| plan.len(v)), c.dark.clone()),
        plan.at(x, y, z),
        id_quat(),
    ));
    // The cable: the thinnest tube the sanitiser keeps, on six facets.
    let r = dim(plan.len(MIN_DIM));
    kids.push(prim(
        sweep(
            &[(plan.at(x, y, z), r), (plan.at(x, cy + CABLE_END, z), r)],
            6,
            2,
            c.metal.clone(),
        ),
        [0.0; 3],
        id_quat(),
    ));
    kids.push(prim(
        cuboid(HOOK.map(|v| plan.len(v)), c.metal.clone()),
        plan.at(x, cy + HOOK_Y, z),
        id_quat(),
    ));
}

/// The refuelling probe (prototype frame): its root and its tip, each `(the
/// share of the cabin's half-width outboard to starboard, the rise over her
/// centreline, the station)`, and its radius.
const PROBE_ROOT: [f32; 3] = [0.75, 0.10, 0.95];
const PROBE_TIP: [f32; 3] = [0.62, 0.12, 1.95];
const PROBE_R: f32 = 0.016;

/// The probe: a straight rod out from her starboard cheek, 35 cm past her
/// nose - outside her box on purpose, as the utility's pitot is.
fn probe(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (cy, w, _) = cabin_section(plan);
    let at = |[share, rise, z]: [f32; 3]| plan.at(-(w * share), cy + rise, z);
    kids.push(rod(
        at(PROBE_ROOT),
        at(PROBE_TIP),
        dim(plan.len(PROBE_R)),
        c.metal.clone(),
    ));
}

/// The beacon (prototype frame): its radius, and how high up the aft
/// pylon's back it stands, as a share of the pylon's half-height.
const BEACON_R: f32 = 0.026;
const BEACON_UP: f32 = 0.75;

/// How far aft of the pylon's middle its back stands on its middle line at
/// a share `up` of its half-height (prototype frame): the superellipsoid's
/// own surface, `(z/c)^(2/ns) + (y/b)^(2/ns) = 1` where `x` is zero.
pub(super) fn pylon_back(up: f32) -> f32 {
    PYLON[2] * (1.0 - up.powf(2.0 / PYLON_NS)).powf(PYLON_NS * 0.5)
}

/// The anti-collision beacon on the aft pylon's back, its middle half its
/// radius aft of the pylon's surface. Drawn as the prototype first had it,
/// at 0.55 of the pylon's half-length aft of its middle, it stood 10 cm
/// inside the pylon - none of it showed.
fn beacon(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    kids.push(prim(
        sphere(dim(plan.len(BEACON_R)), 3, c.beacon.clone()),
        plan.at_off(
            0.0,
            pylon_y(plan) + PYLON[1] * BEACON_UP,
            PYLON_Z,
            -(pylon_back(BEACON_UP) + BEACON_R * 0.5),
        ),
        id_quat(),
    ));
}
