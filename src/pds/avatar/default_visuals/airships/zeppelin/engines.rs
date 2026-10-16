//! The Zeppelin's four engine cars: a streamlined nacelle hung off each
//! lower flank on two struts, a pair amidships and a pair further aft, each
//! with a four-blade wooden pusher turning behind it - the two sides turning
//! opposite ways, as a real ship's did.

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, id_quat, prim, quat_mul, quat_x, quat_xyzw, quat_z, rod, sphere, sweep,
};
use super::super::{AirshipColours, HullPlan, MIN_DIM, dim};

/// The engine cars' noses, as shares of the hull's length: a pair
/// amidships and a pair aft.
pub(super) const ENGINE_X: [f32; 2] = [0.50, 0.66];
/// Their bearing off her widest line, down.
pub(super) const ENGINE_DEG: f32 = -35.0;
/// Their axis's stand-off from the hull's skin, and the nacelle's length and
/// greatest radius (m, prototype frame).
const ENGINE_GAP: f32 = 0.125;
pub(super) const NACELLE_L: f32 = 0.25;
pub(super) const NACELLE_R: f32 = 0.047;
/// The nacelle's stations, `(aft of its nose, radius)` (m, prototype frame).
const NACELLE: [(f32, f32); 6] = [
    (0.00, 0.014),
    (0.03, 0.034),
    (0.08, NACELLE_R),
    (0.14, NACELLE_R * 0.97),
    (0.20, NACELLE_R * 0.78),
    (NACELLE_L, 0.018),
];
/// The radiator over the nacelle's nose: radius, and how far ahead of it.
const RADIATOR_R: f32 = 0.016;
const RADIATOR_AHEAD: f32 = 0.004;
/// The two struts: how far aft of the nacelle's nose, their radius, and how
/// far inside the skin their upper ends reach.
const STRUTS_AFT: [f32; 2] = [0.06, 0.17];
const STRUT_R: f32 = 0.011;
const STRUT_BURY: f32 = 0.02;
/// How steeply a strut climbs to the hull: its upper end stands at this
/// share of the nacelle's bearing.
const STRUT_LEAN: f32 = 0.6;

/// The airscrew: its tip radius, its boss's radius, where its blades start,
/// their chord, their pitch, how far behind the nacelle's tail its hub
/// turns (m, prototype frame), and how fast it turns (degrees a second).
pub(super) const PROP_R: f32 = 0.088;
const BOSS_R: f32 = 0.019;
const BLADE_ROOT: f32 = 0.014;
const BLADE_CHORD: f32 = 0.019;
const BLADE_PITCH_DEG: f32 = 18.0;
const PROP_AFT: f32 = 0.008;
pub(super) const PROP_DEG_S: f32 = 900.0;

/// Where an engine car stands: `(nose z, axis x, axis y)` (drawn frame) of
/// the one at `x` on `side` (`+1` her port, `+X`), `deg` degrees under her
/// widest line.
pub(super) fn nacelle_axis(plan: &HullPlan, x: f32, side: f32, deg: f32) -> (f32, f32, f32) {
    let zf = plan.z(x);
    let girth = plan.radius(plan.x(zf - plan.len(NACELLE_L) * 0.5));
    let a = deg.to_radians();
    let d = girth + plan.len(ENGINE_GAP);
    (zf, side * d * a.cos(), d * a.sin())
}

/// One engine car at `x` on `side`, `deg` degrees under her widest line:
/// its nacelle, the radiator on its nose, its two struts - their upper ends
/// at `lean` of its bearing - and its airscrew.
pub(super) fn engine_car(
    kids: &mut Vec<Generator>,
    plan: &HullPlan,
    (x, side, deg, lean): (f32, f32, f32, f32),
    c: &AirshipColours,
) {
    let (zf, cx, cy) = nacelle_axis(plan, x, side, deg);
    let points: Vec<([f32; 3], f32)> = NACELLE
        .iter()
        .map(|&(dz, r)| ([cx, cy, zf - plan.len(dz)], plan.len(r)))
        .collect();
    kids.push(prim(
        sweep(&points, 16, 4, c.car.clone()),
        [0.0; 3],
        id_quat(),
    ));
    kids.push(prim(
        sphere(plan.len(RADIATOR_R), 3, c.dark.clone()),
        [cx, cy, zf + plan.len(RADIATOR_AHEAD)],
        id_quat(),
    ));
    // Each strut leaves the nacelle's back a little inside it and climbs,
    // steeper than the nacelle's own bearing, into the hull a little inside
    // the skin.
    let a = deg.to_radians() * lean;
    for aft in STRUTS_AFT {
        let z = zf - plan.len(aft);
        let inner = plan.radius(plan.x(z)) - plan.len(STRUT_BURY);
        kids.push(rod(
            [cx * 0.96, cy * 0.96, z],
            [side * inner * a.cos(), inner * a.sin(), z],
            dim(plan.len(STRUT_R)),
            c.dark.clone(),
        ));
    }
    kids.push(airscrew(
        plan,
        [cx, cy, zf - plan.len(NACELLE_L + PROP_AFT)],
        side,
        c,
    ));
}

/// A four-blade wooden pusher on a hidden hub at `at`, turning about `+Z`;
/// the port side's one way and the starboard's the other.
fn airscrew(plan: &HullPlan, at: [f32; 3], side: f32, c: &AirshipColours) -> Generator {
    let mut parts = vec![prim(
        sphere(plan.len(BOSS_R), 3, c.metal.clone()),
        [0.0; 3],
        id_quat(),
    )];
    let mid = plan.len(BLADE_ROOT + PROP_R) * 0.5;
    for k in 0..4 {
        let angle = (45.0 + 90.0 * k as f32).to_radians();
        parts.push(prim(
            cuboid(
                [
                    plan.len(PROP_R - BLADE_ROOT),
                    plan.len(BLADE_CHORD),
                    MIN_DIM,
                ],
                c.wood.clone(),
            ),
            [angle.cos() * mid, angle.sin() * mid, 0.0],
            quat_xyzw(quat_mul(
                quat_z(angle),
                quat_x(BLADE_PITCH_DEG.to_radians()),
            )),
        ));
    }
    let side_len = dim(plan.len(0.02));
    let mut hub = prim(cuboid([side_len; 3], c.dark.clone()), at, id_quat());
    hub.children = parts;
    hub.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Constant {
            rate: Fp(PROP_DEG_S * side),
        },
    ));
    hub
}

/// The four engine cars, port and starboard of each pair.
pub(super) fn engines(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    for x in ENGINE_X {
        for side in [1.0, -1.0] {
            engine_car(kids, plan, (x, side, ENGINE_DEG, STRUT_LEAN), c);
        }
    }
}
