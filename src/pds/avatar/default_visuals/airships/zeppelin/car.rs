//! The Zeppelin's control car: a short sweep under the bow, flattened a
//! little across, its crown buried in the hull - the bridge forward and the
//! saloon behind it, one band of lit windows wrapping her bow and running
//! down both flanks, the scheme's cheat line under it and a landing bumper
//! under her keel.

use crate::pds::generator::Generator;

use super::super::super::common::{id_quat, prim, sphere};
use super::super::{AirshipColours, HullPlan, flank_bands, flat_sweep};

/// Her nose, as a share of the hull's length.
const CAR_X: f32 = 0.068;
/// Her greatest radius (m, prototype frame), her width over her height, and
/// how far her crown sits inside the hull's skin at her widest station.
pub(super) const CAR_R: f32 = 0.096;
const CAR_FLAT: f32 = 0.86;
const CAR_BURY: f32 = 0.026;
/// Her stations, `(aft of her nose, radius)` (m, prototype frame).
const CAR_PROFILE: [(f32, f32); 8] = [
    (0.00, 0.022),
    (0.025, 0.056),
    (0.07, 0.083),
    (0.14, CAR_R),
    (0.26, CAR_R),
    (0.38, CAR_R * 0.97),
    (0.48, CAR_R * 0.80),
    (0.56, 0.042),
];
/// Her widest station, aft of her nose (m, prototype frame).
const CAR_WIDEST: f32 = 0.14;
const CAR_RES: u32 = 24;
const CAR_SAMPLES: u32 = 6;

/// The window band: degrees over her widest line, and its run along her
/// (shares of her arc length); the cheat line under it.
const WINDOWS_DEG: (f32, f32) = (4.0, 32.0);
const WINDOWS_RUN: [f32; 2] = [0.025, 0.84];
const TRIM_DEG: (f32, f32) = (-16.0, -2.0);
const TRIM_RUN: [f32; 2] = [0.03, 0.92];
const SKIN_PROUD: f32 = 0.003;

/// The landing bumper under her keel: its radius, how far its middle stands
/// over her keel line, and how far aft of her nose (m, prototype frame).
pub(super) const BUMPER_R: f32 = 0.026;
pub(super) const BUMPER_RISE: f32 = 0.012;
const BUMPER_AFT: f32 = 0.20;

/// `(z of her nose, her centreline's height)` (drawn frame): her crown
/// [`CAR_BURY`] inside the hull at her widest station.
pub(super) fn axis(plan: &HullPlan) -> (f32, f32) {
    let zf = plan.z(CAR_X);
    let widest = zf - plan.len(CAR_WIDEST);
    let y = -plan.radius(plan.x(widest)) + plan.len(CAR_BURY) - plan.len(CAR_R);
    (zf, y)
}

/// Her stations, `proud` (m, prototype frame) off her own skin.
fn stations(plan: &HullPlan, proud: f32) -> Vec<([f32; 3], f32)> {
    let (zf, cy) = axis(plan);
    CAR_PROFILE
        .iter()
        .map(|&(dz, r)| ([0.0, cy, zf - plan.len(dz)], plan.len(r + proud)))
        .collect()
}

/// The control car.
pub(super) fn car(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    kids.push(flat_sweep(
        &stations(plan, 0.0),
        CAR_RES,
        CAR_SAMPLES,
        0,
        CAR_FLAT,
        &c.car,
        None,
    ));
    for (deg, run, material) in [
        (WINDOWS_DEG, WINDOWS_RUN, &c.lamp),
        (TRIM_DEG, TRIM_RUN, &c.trim),
    ] {
        for cut in flank_bands(deg.0, deg.1) {
            kids.push(flat_sweep(
                &stations(plan, SKIN_PROUD),
                CAR_RES,
                CAR_SAMPLES,
                0,
                CAR_FLAT,
                material,
                Some((cut, run)),
            ));
        }
    }
    let (zf, cy) = axis(plan);
    kids.push(prim(
        sphere(plan.len(BUMPER_R), 3, c.dark.clone()),
        [
            0.0,
            cy - plan.len(CAR_R) + plan.len(BUMPER_RISE),
            zf - plan.len(BUMPER_AFT),
        ],
        id_quat(),
    ));
}
