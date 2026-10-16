//! The Zeppelin's hull and everything laid on it: the doped hull itself,
//! its battened nose cap and mooring cone, the gas-valve hoods along its
//! crown, the cheat line and the lit passenger deck down each flank, and the
//! tail cap with its white light.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;

use super::super::super::common::{
    id_quat, lathe, prim, quat_x, quat_xyzw, sphere, superellipsoid, sweep, with_cut,
};
use super::super::{AirshipColours, HullPlan, flank_bands};

/// The hull sweep's facets round and samples a segment: the prototype's.
const HULL_RES: u32 = 32;
const HULL_SAMPLES: u32 = 6;

/// How far a skin laid on the hull stands off it (m, prototype frame): the
/// nose cap and the cheat line, and the deck's windows a hair further.
const SKIN_PROUD: f32 = 0.003;
const DECK_PROUD: f32 = 0.004;

/// The nose cap's share of the hull's arc length.
const NOSE_CAP: f32 = 0.018;

/// The mooring cone, `(radius, height)` base to tip (m, prototype frame),
/// and how far its base sits inside the nose's tip.
const MOORING_CONE: [(f32, f32); 4] = [(0.050, 0.0), (0.040, 0.020), (0.018, 0.040), (0.0, 0.050)];
const MOORING_SET: f32 = 0.022;

/// The gas-valve hoods along her crown: how many, the first and last
/// stations (shares), their half-extents (m, prototype frame) and how far
/// each sits down into the skin.
const HOODS: usize = 8;
const HOOD_X: (f32, f32) = (0.16, 0.72);
pub(super) const HOOD_HALF: [f32; 3] = [0.020, 0.016, 0.034];
pub(super) const HOOD_SET: f32 = 0.004;

/// The cheat line: degrees over her widest line, and its run along her
/// (shares of the arc length).
const CHEAT_DEG: (f32, f32) = (2.0, 7.0);
const CHEAT_RUN: [f32; 2] = [0.05, 0.74];

/// The passenger deck's windows: degrees under her widest line, and the run.
const DECK_DEG: (f32, f32) = (-45.0, -31.0);
const DECK_RUN: [f32; 2] = [0.22, 0.36];

/// The tail cap, `(radius, height)` base to tip (m, prototype frame), and
/// how far its base sits inside the tail's tip; the white light on it.
const TAIL_CAP: [(f32, f32); 3] = [(0.020, 0.0), (0.014, 0.012), (0.0, 0.026)];
const TAIL_SET: f32 = 0.012;
pub(super) const TAIL_LAMP_R: f32 = 0.012;
pub(super) const TAIL_LAMP_AFT: f32 = 0.016;

/// A quarter turn about `+X` carries `+Y` to `+Z`: a lathe's axis laid
/// along her, pointing ahead.
fn ahead() -> crate::pds::types::Fp4 {
    quat_xyzw(quat_x(FRAC_PI_2))
}

/// The hull, nose to tail: one sweep through the profile's stations.
pub(super) fn hull(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    kids.push(prim(
        sweep(&plan.stations(0.0), HULL_RES, HULL_SAMPLES, c.hull.clone()),
        [0.0; 3],
        id_quat(),
    ));
}

/// The battened nose cap - the hull's own stations a few millimetres out,
/// kept over the nose's first stretch - and the mooring cone over its tip.
pub(super) fn nose(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    kids.push(prim(
        with_cut(
            sweep(
                &plan.stations(plan.len(SKIN_PROUD)),
                HULL_RES,
                HULL_SAMPLES,
                c.hood.clone(),
            ),
            [0.0, 1.0],
            [0.0, NOSE_CAP],
            0.0,
        ),
        [0.0; 3],
        id_quat(),
    ));
    let cone: Vec<(f32, f32)> = MOORING_CONE
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    kids.push(prim(
        lathe(&cone, 16, true, c.metal.clone()),
        [0.0, 0.0, plan.z(0.0) - plan.len(MOORING_SET)],
        ahead(),
    ));
}

/// Where the mooring cone's tip stands (z, drawn frame) - the most forward
/// thing she draws, and her box's nose.
#[cfg(test)]
pub(super) fn mooring_tip(plan: &HullPlan) -> f32 {
    plan.z(0.0) - plan.len(MOORING_SET) + plan.len(MOORING_CONE[3].1)
}

/// The share of each gas-valve hood along her crown.
pub(super) fn hood_stations() -> impl Iterator<Item = f32> {
    (0..HOODS).map(|k| HOOD_X.0 + k as f32 * (HOOD_X.1 - HOOD_X.0) / (HOODS - 1) as f32)
}

/// The gas-valve hoods: a row of low rounded hoods along her crown, each
/// sat a little down into the skin - what the play camera, above and behind
/// her, reads her length by.
pub(super) fn hoods(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    let half = HOOD_HALF.map(|v| plan.len(v));
    for x in hood_stations() {
        kids.push(prim(
            superellipsoid(half, 0.6, 0.6, 8, 12, c.hood.clone()),
            [0.0, plan.radius(x) - plan.len(HOOD_SET), plan.z(x)],
            id_quat(),
        ));
    }
}

/// A band laid on the hull's own stations, `proud` (m, prototype frame)
/// off it, between `deg` degrees over her widest line on each flank and
/// along `run` of her length.
fn flank_skin(
    kids: &mut Vec<Generator>,
    plan: &HullPlan,
    proud: f32,
    deg: (f32, f32),
    run: [f32; 2],
    material: &crate::pds::texture::SovereignMaterialSettings,
) {
    for cut in flank_bands(deg.0, deg.1) {
        kids.push(prim(
            with_cut(
                sweep(
                    &plan.stations(plan.len(proud)),
                    HULL_RES,
                    HULL_SAMPLES,
                    material.clone(),
                ),
                cut,
                run,
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
}

/// The cheat line down each flank, a little over her widest line: the
/// seeded accent, the fleet's identity slot.
pub(super) fn cheat_line(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    flank_skin(kids, plan, SKIN_PROUD, CHEAT_DEG, CHEAT_RUN, &c.stripe);
}

/// The passenger deck: a band of lit windows low on each flank, behind the
/// control car.
pub(super) fn deck(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    flank_skin(kids, plan, DECK_PROUD, DECK_DEG, DECK_RUN, &c.lamp);
}

/// The tail cap over the hull's last centimetre, and the white light on its
/// tip.
pub(super) fn tail_cap(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    let cap: Vec<(f32, f32)> = TAIL_CAP
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    let z = plan.z(1.0);
    kids.push(prim(
        lathe(&cap, 12, true, c.metal.clone()),
        [0.0, 0.0, z + plan.len(TAIL_SET)],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    kids.push(prim(
        sphere(plan.len(TAIL_LAMP_R), 3, c.white.clone()),
        [0.0, 0.0, z - plan.len(TAIL_LAMP_AFT)],
        id_quat(),
    ));
}
