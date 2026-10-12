//! The ladder of secondary masses by ornateness - #1625 Q-F, agreed. An
//! adorned machine carries a pitot under her nose and an aerial mast on her
//! roof with a wire from its top to each fin; an ornate one a beacon on her
//! roof and a fuel tank on each wingtip in the trim colour, as the post-war
//! Twin Beech did - her nav lights moved out onto them
//! ([`super::wing::lights`]). Wear has no ladder of its own: it is the
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s grime on every
//! surface.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{id_quat, prim, rod, sphere, sweep};
use super::super::{FusePlan, MIN_DIM, TwinColours, dim};
use super::tail::wire_end;
use super::wing::{HALF_SPAN, chord_at, wing_le, wing_y};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &FusePlan,
    c: &TwinColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    pitot(kids, plan, c);
    aerial(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        beacon(kids, plan, c);
        tanks(kids, plan, c);
    }
}

/// The pitot (prototype frame): its root's station, `x`, how far over the
/// belly it stands, and its tip ahead of it and down - a tube raked forward
/// and down under her nose.
const PITOT_Z: f32 = 1.30;
const PITOT_X: f32 = 0.06;
const PITOT_RISE: f32 = 0.03;
const PITOT_AHEAD: f32 = 0.16;

/// A pitot under her nose, raked forward and down.
fn pitot(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let belly = plan.sill(PITOT_Z);
    kids.push(rod(
        plan.at(PITOT_X, belly + PITOT_RISE, PITOT_Z),
        plan.at(PITOT_X, belly - PITOT_RISE, PITOT_Z + PITOT_AHEAD),
        dim(plan.len(MIN_DIM)),
        c.metal.clone(),
    ));
}

/// The aerial mast (prototype frame): its foot's station and how far into
/// the roof it stands, its top over the roof and aft of its foot, and its
/// radius.
const MAST_Z: f32 = 0.70;
const MAST_SINK: f32 = 0.02;
const MAST_RISE: f32 = 0.13;
const MAST_AFT: f32 = 0.04;
const MAST_R: f32 = 0.012;

/// The aerial: a mast on the cabin's roof, raked aft, and a wire from its
/// top aft to each fin, port (`+X`) first.
fn aerial(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let roof = plan.crown(MAST_Z);
    let top = plan.at_off(0.0, roof + MAST_RISE, MAST_Z, -MAST_AFT);
    kids.push(rod(
        plan.at(0.0, roof - MAST_SINK, MAST_Z),
        top,
        dim(plan.len(MAST_R)),
        c.metal.clone(),
    ));
    for side in [1.0f32, -1.0] {
        kids.push(wire(
            top,
            wire_end(plan, side),
            dim(plan.len(MIN_DIM)),
            &c.metal,
        ));
    }
}

/// A wire: a straight rod at six facets, the prototype's.
fn wire(a: [f32; 3], b: [f32; 3], radius: f32, material: &SovereignMaterialSettings) -> Generator {
    prim(
        sweep(&[(a, radius), (b, radius)], 6, 2, material.clone()),
        [0.0; 3],
        id_quat(),
    )
}

/// The beacon (prototype frame): its station on the cabin's roof and how
/// far its middle stands over it - its lowest point in the roof.
const BEACON_Z: f32 = 0.30;
const BEACON_OVER: f32 = 0.012;

/// The anti-collision beacon on the cabin's roof.
fn beacon(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    kids.push(prim(
        sphere(dim(plan.len(0.026)), 3, c.beacon.clone()),
        plan.at(0.0, plan.crown(BEACON_Z) + BEACON_OVER, BEACON_Z),
        id_quat(),
    ));
}

/// A tip tank (prototype frame): its middle's `x`, a centimetre in from the
/// wingtip, and its stations `(dz from the tip's mid-chord, radius)` nose to
/// tail.
pub(super) const TANK_X: f32 = HALF_SPAN - 0.01;
pub(super) const TANK: [(f32, f32); 6] = [
    (0.30, 0.020),
    (0.22, 0.055),
    (0.08, 0.072),
    (-0.08, 0.070),
    (-0.22, 0.050),
    (-0.32, 0.020),
];

/// A tank on each wingtip in the trim colour, port (`+X`) first: a sweep
/// along her through the tip, hung rigidly off the tip's leading edge.
fn tanks(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let y = wing_y(HALF_SPAN);
    let mid = -0.5 * chord_at(HALF_SPAN);
    for side in [1.0f32, -1.0] {
        let points: Vec<([f32; 3], f32)> = TANK
            .iter()
            .map(|&(dz, r)| {
                (
                    plan.at_off(side * TANK_X, y, wing_le(HALF_SPAN), mid + dz),
                    plan.len(r),
                )
            })
            .collect();
        kids.push(prim(
            sweep(&points, 18, 5, c.trim.clone()),
            [0.0; 3],
            id_quat(),
        ));
    }
}
