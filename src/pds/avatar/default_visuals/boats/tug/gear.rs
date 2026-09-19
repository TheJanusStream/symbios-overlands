//! What works the tug: the towing gear on her after deck - the hook on the
//! casing, the arches the tow line rides over, the H-bitt on the counter and
//! the Ornate hawser - or, on the harbour tender, a cargo derrick in its
//! place; and what every tug hangs round her: the tyre fenders along the
//! sheer and the rope puddings at the stem and the counter. Every rotated
//! piece is at uniform scale.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;

use super::super::super::common::quat_z;
use super::super::TugColours;
use super::super::profile::HullProfile;
use super::super::shape::{UPRIGHT, line, panel, turned};
use super::hull::{WALL, deck_edge, deck_y, inner_half_width};
use super::works::Works;
use super::{TYRE_R, TYRE_W};

/// The two towing arches, forward one first, and their height over the deck,
/// as fractions of the length.
const ARCHES_ZF: [f32; 2] = [-0.285, -0.385];
const ARCH_H: f32 = 0.095;

/// The H-bitt on the counter, the Ornate hawser between the arches, and the
/// derrick boom's head over the working deck.
const BITTS_ZF: f32 = -0.445;
const HAWSER_ZF: f32 = -0.335;
const BOOM_ZF: f32 = -0.410;

/// The four stations the tyres hang at, aft to forward.
const TYRES_ZF: [f32; 4] = [-0.330, -0.170, -0.010, 0.150];

/// The towing hook on the casing's after end: a heavy bracket curving aft
/// and up to the hook's point.
pub(super) fn hook(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours, w: &Works) {
    let l = hull.loa;
    let y = (w.base + w.top) * 0.5;
    let z = w.za;
    kids.push(line(
        &[
            ([0.0, y + l * 0.010, z + l * 0.006], l * 0.0075),
            ([0.0, y - l * 0.004, z - l * 0.030], l * 0.0075),
            ([0.0, y + l * 0.014, z - l * 0.046], l * 0.0060),
        ],
        8,
        &c.iron,
    ));
}

/// The towing bows: steel arches across the working deck from rail to rail
/// that the tow line rides over - a tug's own silhouette from astern, which
/// nothing else in the family has. A `battered` tug has lost the after one.
pub(super) fn arches(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &TugColours,
    battered: bool,
) {
    let l = hull.loa;
    for (i, zf) in ARCHES_ZF.into_iter().enumerate() {
        if battered && i + 1 == ARCHES_ZF.len() {
            continue;
        }
        let z = zf * l;
        let x = hull.half_beam_at(z) * WALL;
        let y0 = hull.sheer_z(z) - l * 0.004;
        let top = deck_y(hull, z, 0.0) + l * ARCH_H;
        let r = l * 0.009;
        kids.push(line(
            &[
                ([-x, y0, z], r),
                ([-x * 0.86, top - l * 0.030, z], r),
                ([0.0, top, z], r),
                ([x * 0.86, top - l * 0.030, z], r),
                ([x, y0, z], r),
            ],
            8,
            &c.iron,
        ));
    }
}

/// An H-bitt on the counter: two turned posts and a crossbar.
pub(super) fn bitts(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours) {
    let l = hull.loa;
    let z = BITTS_ZF * l;
    let x = deck_edge(hull, z) * 0.34;
    let (r, h) = (l * 0.014, l * 0.058);
    let y = deck_y(hull, z, x) - l * 0.006;
    for side in [-1.0f32, 1.0] {
        kids.push(turned(
            &[(r, 0.0), (r, h * 0.86), (r * 1.3, h * 0.90), (r * 1.3, h)],
            12,
            false,
            &c.iron,
            [side * x, y, z],
            UPRIGHT,
            0.0,
        ));
    }
    let y = y + h * 0.60;
    kids.push(line(
        &[
            ([-x - r * 0.6, y, z], l * 0.0075),
            ([x + r * 0.6, y, z], l * 0.0075),
        ],
        8,
        &c.iron,
    ));
}

/// Where a fender hangs at `zf`: bedded against the topsides a hand under
/// the rail. Its z, its centre's height, and the hull's half-width there.
fn hanging(hull: &HullProfile, zf: f32) -> (f32, f32, f32) {
    let l = hull.loa;
    let z = zf * l;
    let y = hull.sheer_z(z) - l * TYRE_R * 1.15;
    (z, y, inner_half_width(hull, z, hull.sheer_z(z) - y, 1.0))
}

/// Tyre fenders hung along the sheer, four a side, each a bored Lathe band
/// with its axis athwartships - the fleet's ring, which both guards read as
/// a band where they could not read a torus - bedded against the topsides a
/// hand under the rail. A `battered` tug has lost one.
pub(super) fn tyres(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours, battered: bool) {
    let l = hull.loa;
    let (r, w) = (l * TYRE_R, l * TYRE_W);
    for (i, zf) in TYRES_ZF.into_iter().enumerate() {
        let (z, y, xo) = hanging(hull, zf);
        for side in [-1.0f32, 1.0] {
            if battered && i == 1 && side > 0.0 {
                continue;
            }
            kids.push(turned(
                &[(r, -w * 0.5), (r, w * 0.5)],
                14,
                false,
                &c.tyre,
                [side * (xo + w * 0.5 - l * 0.003), y, z],
                quat_z(FRAC_PI_2),
                0.50,
            ));
        }
    }
}

/// A rope collar round the stem head and a rope fender round the counter -
/// pale, fat, and in frame from astern.
pub(super) fn puddings(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours) {
    let l = hull.loa;
    let r = l * 0.020;
    let [z0, z1] = [0.43 * l, 0.475 * l];
    let stem = hull.stem_z();
    kids.push(line(
        &[
            (
                [
                    -hull.half_beam_at(z0) * 0.99,
                    hull.sheer_z(z0) - r * 0.5,
                    z0,
                ],
                r,
            ),
            (
                [
                    -hull.half_beam_at(z1) * 0.99,
                    hull.sheer_z(z1) - r * 0.4,
                    z1,
                ],
                r,
            ),
            (
                [0.0, hull.sheer_z(stem) - r * 0.3, stem + r * 0.35],
                r * 1.1,
            ),
            (
                [hull.half_beam_at(z1) * 0.99, hull.sheer_z(z1) - r * 0.4, z1],
                r,
            ),
            (
                [hull.half_beam_at(z0) * 0.99, hull.sheer_z(z0) - r * 0.5, z0],
                r,
            ),
        ],
        10,
        &c.rope,
    ));
    let r = l * 0.018;
    let [z0, z1, z2] = [-0.40 * l, -0.46 * l, -0.49 * l];
    let t = hull.transom_z();
    let hang = |z: f32| hull.sheer_z(z) - r * 0.9;
    kids.push(line(
        &[
            ([-hull.half_beam_at(z0), hang(z0), z0], r),
            ([-hull.half_beam_at(z1), hang(z1), z1], r),
            ([-hull.half_beam_at(z2) * 0.80, hang(z2), z2 - r * 0.3], r),
            ([0.0, hang(t), t - r * 0.55], r),
            ([hull.half_beam_at(z2) * 0.80, hang(z2), z2 - r * 0.3], r),
            ([hull.half_beam_at(z1), hang(z1), z1], r),
            ([hull.half_beam_at(z0), hang(z0), z0], r),
        ],
        10,
        &c.rope,
    ));
}

/// Ornate, towing: a coil of towing hawser flat on the working deck between
/// the arches, a bored Lathe band - pale manila on the dark deck.
pub(super) fn hawser(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours) {
    let l = hull.loa;
    let z = HAWSER_ZF * l;
    let r = l * 0.050;
    kids.push(turned(
        &[(r, 0.0), (r, l * 0.018)],
        18,
        false,
        &c.rope,
        [0.0, deck_y(hull, z, 0.0) - l * 0.004, z],
        UPRIGHT,
        0.45,
    ));
}

/// The harbour tender's cargo derrick, where a towing tug's gear stands: a
/// turned post abaft the casing, a boom raised aft over the working deck on
/// a topping lift, a crate slung from its head - high enough to read as
/// hanging - and a winch at the post's foot.
pub(super) fn derrick(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours, w: &Works) {
    let l = hull.loa;
    let zp = w.za - l * 0.022;
    let yb = deck_y(hull, zp, 0.0) - l * 0.006;
    let h = l * 0.300;
    let r = l * 0.016;
    kids.push(turned(
        &[
            (r * 1.35, 0.0),
            (r, h * 0.10),
            (r * 0.78, h),
            (r * 1.1, h * 1.02),
            (r * 0.9, h * 1.04),
        ],
        12,
        false,
        &c.derrick,
        [0.0, yb, zp],
        UPRIGHT,
        0.0,
    ));
    let zh = BOOM_ZF * l;
    let heel = [0.0, yb + l * 0.050, zp - r * 0.5];
    let head = [0.0, deck_y(hull, zh, 0.0) + l * 0.200, zh];
    kids.push(line(
        &[(heel, l * 0.010), (head, l * 0.0070)],
        8,
        &c.derrick,
    ));
    kids.push(line(
        &[([0.0, yb + h * 0.98, zp], l * 0.0035), (head, l * 0.0035)],
        6,
        &c.iron,
    ));
    let cs = l * 0.072;
    let ct = deck_y(hull, zh, 0.0) + l * 0.100;
    kids.push(line(
        &[(head, l * 0.0035), ([0.0, ct + l * 0.004, zh], l * 0.0035)],
        6,
        &c.iron,
    ));
    kids.push(panel(
        [cs * 1.15, cs * 0.80, cs],
        &c.cargo,
        [0.0, ct - cs * 0.40, zh],
        UPRIGHT,
        cs * 0.06,
    ));
    let zw = zp - l * 0.045;
    kids.push(turned(
        &[(l * 0.020, -l * 0.032), (l * 0.020, l * 0.032)],
        12,
        false,
        &c.iron,
        [0.0, deck_y(hull, zw, 0.0) + l * 0.018, zw],
        quat_z(FRAC_PI_2),
        0.0,
    ));
}
