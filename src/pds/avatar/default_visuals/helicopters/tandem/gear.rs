//! Her landing gear (#1630 Q-A): a sponson low on each flank of her cabin,
//! a rounded box running most of her length, and her four wheels half in
//! them, each on a leg down out of its sponson - and the nav lights on the
//! sponsons' noses.
//!
//! The sponsons stand on stretched stations at both ends, so a longer boom
//! lengthens them with the cabin and carries the rear wheels aft with it.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::{Generator, GeneratorKind};

use super::super::super::common::{
    cylinder, id_quat, prim, quat_xyzw, quat_z, rod, sphere, superellipsoid, torus,
};
use super::super::{PodPlan, TandemColours, dim};
use super::cabin_section;

/// A sponson (prototype frame): its half-width and half-height; the station
/// of its middle and how far fore and aft of it it reaches, both ends
/// stretched; how far over the cabin's belly its bottom stands; and its
/// exponents - its upright profile and its section along her - and facets.
const SPONSON_HALF: [f32; 2] = [0.07, 0.065];
const SPONSON_Z: f32 = -0.10;
const SPONSON_REACH: f32 = 0.72;
const SPONSON_LIFT: f32 = 0.01;
const SPONSON_NS: f32 = 0.5;
const SPONSON_EW: f32 = 0.45;
const SPONSON_LAT: u32 = 16;
const SPONSON_LON: u32 = 24;

/// The port sponson's middle, `(x, y)` (prototype frame): half its width
/// out of the cabin's flank, its bottom [`SPONSON_LIFT`] over the belly.
fn sponson_middle(plan: &PodPlan) -> (f32, f32) {
    let (cy, w, h) = cabin_section(plan);
    (
        w + SPONSON_HALF[0] * 0.5,
        cy - h + SPONSON_HALF[1] + SPONSON_LIFT,
    )
}

/// The two sponsons, port first.
pub(super) fn sponsons(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (x, y) = sponson_middle(plan);
    let (fore, aft) = (
        plan.stretch(SPONSON_Z + SPONSON_REACH),
        plan.stretch(SPONSON_Z - SPONSON_REACH),
    );
    for side in [1.0f32, -1.0] {
        kids.push(prim(
            superellipsoid(
                [
                    plan.len(SPONSON_HALF[0]),
                    plan.len(SPONSON_HALF[1]),
                    plan.len((fore - aft) * 0.5),
                ],
                SPONSON_NS,
                SPONSON_EW,
                SPONSON_LAT,
                SPONSON_LON,
                c.body.clone(),
            ),
            plan.at(side * x, y, SPONSON_Z),
            id_quat(),
        ));
    }
}

/// The wheels (prototype frame): their stations, front and rear, both
/// stretched; the tyre's radius, which its axle stands at over the ground,
/// and its tube's; how far outboard of the sponson's middle a wheel turns;
/// the leg's radius and how far over the axle it ends; and the hub's radius
/// and width.
const WHEEL_Z: [f32; 2] = [0.55, -0.75];
const WHEEL_R: f32 = 0.11;
const TYRE_TUBE: f32 = 0.035;
const WHEEL_OUT: f32 = 0.01;
const LEG_R: f32 = 0.02;
const LEG_FOOT: f32 = 0.01;
const HUB_R: f32 = 0.045;
const HUB_W: f32 = 0.08;

/// The four wheels, front pair first and port first in each: a leg down out
/// of its sponson into the hub, the tyre and the hub, laid across her on
/// the axle.
pub(super) fn wheels(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (sx, sy) = sponson_middle(plan);
    // A torus rings its +Y and a cylinder stands on it; a quarter turn about
    // +Z lays both across her.
    let across = quat_xyzw(quat_z(FRAC_PI_2));
    for z in WHEEL_Z {
        for side in [1.0f32, -1.0] {
            let x = side * (sx + WHEEL_OUT);
            kids.push(rod(
                plan.at(x, sy - SPONSON_HALF[1] * 0.5, z),
                plan.at(x, WHEEL_R + LEG_FOOT, z),
                dim(plan.len(LEG_R)),
                c.metal.clone(),
            ));
            let mut tyre = torus(
                dim(plan.len(TYRE_TUBE)),
                plan.len(WHEEL_R - TYRE_TUBE),
                c.dark.clone(),
            );
            if let GeneratorKind::Torus {
                minor_resolution,
                major_resolution,
                ..
            } = &mut tyre
            {
                (*minor_resolution, *major_resolution) = (8, 20);
            }
            kids.push(prim(tyre, plan.at(x, WHEEL_R, z), across));
            kids.push(prim(
                cylinder(dim(plan.len(HUB_R)), plan.len(HUB_W), 12, c.metal.clone()),
                plan.at(x, WHEEL_R, z),
                across,
            ));
        }
    }
}

/// A nav light's radius, and how far aft of its sponson's nose its middle
/// stands (prototype frame) - rigid off the nose, which is stretched.
const NAV_R: f32 = 0.022;
const NAV_BACK: f32 = 0.02;

/// The nav lights on the sponsons' noses: red to port (`+X`), green to
/// starboard.
pub(super) fn nav_lights(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (x, y) = sponson_middle(plan);
    for (side, lamp) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        kids.push(prim(
            sphere(dim(plan.len(NAV_R)), 3, lamp.clone()),
            plan.at_off(side * x, y, SPONSON_Z + SPONSON_REACH, -NAV_BACK),
            id_quat(),
        ));
    }
}
