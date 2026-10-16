//! The Zeppelin's ladder of secondary masses by ornateness (#1657).
//!
//! A plain ship is the phase-1 prototype exactly. An adorned one carries a
//! searchlight under her control car's bow; an ornate one, as well, a fifth
//! engine car on her keel between the two pairs - LZ 127 Graf Zeppelin's
//! five-engine layout.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{cylinder, prim, quat_x, quat_xyzw};
use super::super::{AirshipColours, HullPlan, dim};
use super::car;
use super::engines;

/// The searchlight: its housing's radius and length, how far aft of the
/// car's nose it hangs, how far up into her keel it sits, and its lens's
/// radius and thickness (m, prototype frame).
const LIGHT_R: f32 = 0.022;
const LIGHT_L: f32 = 0.045;
const LIGHT_AFT: f32 = 0.07;
/// The car's radius at [`LIGHT_AFT`] - the profile's own station there.
const CAR_R_AT_LIGHT: f32 = 0.083;
const LIGHT_SET: f32 = 0.010;
const LENS_R: f32 = 0.018;
const LENS_T: f32 = 0.006;

/// The keel engine car: its nose's share of the hull's length, straight
/// down, its struts straight up.
const KEEL_ENGINE_X: f32 = 0.585;
const KEEL_ENGINE: (f32, f32, f32, f32) = (KEEL_ENGINE_X, 1.0, -90.0, 1.0);

/// The searchlight's housing middle (drawn frame).
pub(super) fn searchlight_at(plan: &HullPlan) -> [f32; 3] {
    let (zf, cy) = car::axis(plan);
    [
        0.0,
        cy - plan.len(CAR_R_AT_LIGHT) + plan.len(LIGHT_R - LIGHT_SET),
        zf - plan.len(LIGHT_AFT),
    ]
}

/// Dress the ship for `tier`.
pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &HullPlan,
    c: &AirshipColours,
    tier: OrnatenessTier,
) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    // A cylinder's axis is +Y; a quarter turn about +X lays it along her,
    // its lens facing ahead.
    let ahead = quat_xyzw(quat_x(FRAC_PI_2));
    let [x, y, z] = searchlight_at(plan);
    kids.push(prim(
        cylinder(plan.len(LIGHT_R), plan.len(LIGHT_L), 14, c.metal.clone()),
        [x, y, z],
        ahead,
    ));
    kids.push(prim(
        cylinder(
            dim(plan.len(LENS_R)),
            dim(plan.len(LENS_T)),
            14,
            c.lamp.clone(),
        ),
        [x, y, z + plan.len(LIGHT_L) * 0.5],
        ahead,
    ));
    if tier == OrnatenessTier::Ornate {
        engines::engine_car(kids, plan, KEEL_ENGINE, c);
    }
}
