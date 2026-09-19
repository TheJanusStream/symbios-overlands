//! The tug's ornateness and wear ladder, as masses that read at 12 m.
//! ORNATENESS: Adorned carries the ship's boat on the casing top abaft the
//! funnel and two cowl ventilators forward of it; Ornate adds a signal mast
//! on the wheelhouse roof with a lit masthead lamp (and, on a towing tug,
//! the hawser coiled on her after deck - see `gear`). WEAR is on the funnel,
//! the deck and the gear, never the topsides, which roll away under the deck
//! edge from the chase camera (#1365): Worn re-lays a quadrant of the
//! working deck in pale new boards and sends the soot down the funnel;
//! Battered sends it further, rusts the funnel through at its base, loses
//! the after towing arch and a tyre, and puts the boat under a tarp.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;

use super::super::super::common::quat_x;
use super::super::TugColours;
use super::super::profile::HullProfile;
use super::super::shape::{DECK_CROWN, PATCH_CUT, UPRIGHT, line, panel, run_z, sweep, turned};
use super::hull::{deck_edge, deck_line};
use super::works::Works;

/// The ship's boat's after and fore ends on the casing top, as fractions of
/// the length.
const BOAT: (f32, f32) = (-0.194, -0.090);

/// Her plan, `(fraction of her length, half-beam fraction)` stern to stem -
/// a double-ender - and her depth over her half-beam.
const BOAT_PLAN: [(f32, f32); 7] = [
    (0.00, 0.14),
    (0.12, 0.62),
    (0.30, 0.92),
    (0.50, 1.00),
    (0.70, 0.92),
    (0.88, 0.62),
    (1.00, 0.14),
];
const BOAT_DEPTH: f32 = 0.70;

/// The re-laid quadrant of the working deck, aft end to fore end, as
/// fractions of the length.
const PATCH: (f32, f32) = (-0.370, -0.250);

/// Adorned: the ship's boat on chocks on the casing top abaft the funnel - a
/// double-ender under a canvas cover, a mass the chase camera sees whole.
/// The cover sits INSIDE her gunwale, so the white boat shows round it: a
/// cover over the whole boat drew a grey saucer. A `battered` tug's boat is
/// under a weathered tarp.
pub(super) fn boat(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &TugColours,
    w: &Works,
    battered: bool,
) {
    let l = hull.loa;
    let (z0, z1) = (BOAT.0 * l, BOAT.1 * l);
    let ln = z1 - z0;
    let hb = (w.hw * 0.62).min(l * 0.042);
    for f in [0.28f32, 0.72] {
        kids.push(panel(
            [hb * 1.5, l * 0.014, l * 0.012],
            &c.iron,
            [0.0, w.top + l * 0.006, z0 + f * ln],
            UPRIGHT,
            0.0,
        ));
    }
    let gunwale = w.top + l * 0.012 + hb * BOAT_DEPTH * 0.90 - l * 0.004;
    let pts: Vec<_> = BOAT_PLAN
        .iter()
        .map(|&(f, rf)| ([0.0, gunwale, z0 + f * ln], hb * rf))
        .collect();
    kids.push(sweep(
        &pts,
        12,
        [1.0, BOAT_DEPTH, 1.0],
        [0.5, 1.0],
        &c.boat,
        0.0,
    ));
    let cover: Vec<_> = BOAT_PLAN[1..BOAT_PLAN.len() - 1]
        .iter()
        .map(|&(f, rf)| ([0.0, gunwale - l * 0.002, z0 + f * ln], hb * rf * 0.90))
        .collect();
    kids.push(sweep(
        &cover,
        12,
        [1.0, 0.36, 1.0],
        [0.0, 0.5],
        if battered { &c.tarp } else { &c.cover },
        0.0,
    ));
}

/// Adorned: two cowl ventilators forward of the funnel, their bell mouths
/// turned to the wind - a pipe and a flared bell, both turned.
pub(super) fn vents(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours, w: &Works) {
    let l = hull.loa;
    let (r, h) = (l * 0.013, l * 0.100);
    let z = w.funnel_foot[2] + l * 0.040;
    let y0 = w.top - l * 0.004;
    for side in [-1.0f32, 1.0] {
        let x = side * w.hw * 0.70;
        kids.push(turned(
            &[(r, 0.0), (r, h)],
            12,
            false,
            &c.vent,
            [x, y0, z],
            UPRIGHT,
            0.0,
        ));
        kids.push(turned(
            &[(r, 0.0), (r * 1.3, l * 0.012), (r * 2.3, l * 0.032)],
            12,
            false,
            &c.vent,
            [x, y0 + h - r, z - r],
            quat_x(FRAC_PI_2),
            0.75,
        ));
    }
}

/// Ornate: a signal mast on the wheelhouse roof with its crosstree and a lit
/// masthead lamp.
pub(super) fn signal_mast(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &TugColours,
    w: &Works,
) {
    let l = hull.loa;
    let z = w.wa + l * 0.024;
    let y0 = w.wtop + l * 0.004;
    let h = l * 0.180;
    kids.push(turned(
        &[(l * 0.008, 0.0), (l * 0.007, h * 0.8), (l * 0.0055, h)],
        10,
        false,
        &c.mast,
        [0.0, y0, z],
        UPRIGHT,
        0.0,
    ));
    let yt = y0 + h * 0.70;
    kids.push(line(
        &[
            ([-l * 0.050, yt, z], l * 0.004),
            ([l * 0.050, yt, z], l * 0.004),
        ],
        6,
        &c.mast,
    ));
    kids.push(turned(
        &[(l * 0.010, 0.0), (l * 0.010, l * 0.018)],
        10,
        false,
        &c.lamp,
        [0.0, y0 + h - l * 0.006, z],
        UPRIGHT,
        0.0,
    ));
}

/// Worn: a quadrant of the working deck re-laid in new pale boards, a hair
/// proud of the deck - on the deck the chase camera looks down on, never the
/// topsides.
pub(super) fn deck_patch(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours) {
    let l = hull.loa;
    let pts: Vec<_> = run_z(hull, PATCH.0 * l, PATCH.1 * l)
        .into_iter()
        .map(|z| {
            (
                [0.0, deck_line(hull, z) + l * 0.0015, z],
                deck_edge(hull, z),
            )
        })
        .collect();
    kids.push(sweep(
        &pts,
        8,
        [1.0, DECK_CROWN * 1.02, 1.0],
        PATCH_CUT,
        &c.patch,
        0.0,
    ));
}
