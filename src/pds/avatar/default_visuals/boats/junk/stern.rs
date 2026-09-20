//! The junk's stern: the high flat transom the chase camera looks at, the
//! painted roundel on it, the lit windows of her stern cabin on her
//! quarters, and the big slotted rudder hung under it - on show, because
//! she hovers.
//!
//! # The transom is its own board
//!
//! The stern's section is shallow and rides clear of the water, and a plug's
//! end cap is no bigger than the section it closes, so the high flat face is
//! a [`board`] of its own aft of the plug: the stern's section from the flat
//! bottom to the sheer, flaring to a taffrail [`TAFFRAIL`] over it.
//!
//! # The rudder is her draft
//!
//! The finless allowance under her canoe body IS the rudder's depth, so her
//! derived draft covers it and her hover clears it by a quarter of a draft,
//! by construction: its foot is minus her draft ([`rudder_foot`]). The blade
//! is SOLID - no boolean cut exists - and its six slots, the fenestrated
//! junk rudder's, are dark diamonds a hair proud of both faces. It keeps all
//! six at every wear: a missing slot did not read at 12 m.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use crate::pds::generator::Generator;

use super::super::super::common::{cuboid, prim, quat_x, quat_xyzw};
use super::super::profile::HullProfile;
use super::super::shape::{flat_sweep, line, turned};
use super::super::{JunkColours, dim};
use super::hull::{Board, board, lay_on, poop_deck, side_point};

/// The transom board's thickness and how far it stands over the sheer, as
/// fractions of the length.
const TRANSOM_T: f32 = 0.012;
const TAFFRAIL: f32 = 0.070;

/// The rudder blade's chord and its stock's radius, as fractions of the
/// length.
const RUDDER_CHORD: f32 = 0.130;
const STOCK_R: f32 = 0.013;

/// The roundel's radius, as a fraction of the length.
const ROUNDEL_R: f32 = 0.058;

/// The quarter windows' stations, as fractions of the length: two a side,
/// abreast the poop.
const WINDOWS_ZF: [f32; 2] = [-0.445, -0.365];

/// The transom board's after face: the rudder's stock lies on it, and the
/// roundel is painted on it.
fn transom_aft(hull: &HullProfile) -> f32 {
    hull.transom_z() - 0.003 - hull.loa * TRANSOM_T
}

/// The rudder's foot: minus her draft, so the hover clears it by
/// construction.
pub(super) fn rudder_foot(hull: &HullProfile) -> f32 {
    -hull.draft
}

/// The high flat transom: a board across her stern aft of the plug - aft
/// of her last station, where the profile reads the stern's own - from the
/// flat bottom to the sheer, a little outside the shell, and flaring to the
/// taffrail over it.
pub(super) fn transom(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let t = hull.transom_z();
    let sh = hull.sheer_z(t);
    kids.push(board(
        hull,
        &c.transom,
        &Board {
            z: t - 0.003 - l * TRANSOM_T * 0.5,
            y0: hull.keel_at(t) - l * 0.002,
            y1: sh,
            wall: 1.0,
            grow: 1.012,
            thick: TRANSOM_T,
            cap: Some((sh + l * TAFFRAIL, hull.half_beam_at(t) * 1.045)),
        },
    ));
}

/// The rudder hung on the transom: a stock from over the taffrail down the
/// transom's face to the foot, a tiller forward over the poop, the blade
/// abaft the stock, and its six slots.
pub(super) fn rudder(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let t = hull.transom_z();
    let rs = l * STOCK_R;
    let zs = transom_aft(hull) - rs * 0.7;
    let top = hull.sheer_z(t) + l * TAFFRAIL + l * 0.012;
    let foot = rudder_foot(hull);
    kids.push(line(
        &[
            ([0.0, top, zs], rs),
            ([0.0, foot + l * 0.006, zs], rs * 0.85),
        ],
        8,
        &c.rudder,
    ));
    let zt = t + l * 0.17;
    kids.push(line(
        &[
            ([0.0, top - l * 0.006, zs], l * 0.007),
            ([0.0, poop_deck(hull) + l * 0.045, zt], l * 0.0055),
        ],
        6,
        &c.rudder,
    ));
    // The blade: a flattened sweep down the stock, narrow under the stern
    // and full over the foot. Its lower three stations stand on ONE
    // vertical, so the foot's chord is level at the draft: a path leaning
    // into its last segment tilts the rings the mesher lays near its end,
    // and the blade's after corner drooped under the foot.
    let chord = l * RUDDER_CHORD;
    let blade_top = hull.keel_at(t) + l * 0.004;
    let h = blade_top - foot;
    let zc = zs - chord * 0.50 + rs * 0.4;
    let blade = [
        ([0.0, blade_top, zs - chord * 0.30 + rs * 0.4], chord * 0.30),
        ([0.0, blade_top - h * 0.40, zc], chord * 0.50),
        ([0.0, foot + h * 0.08, zc], chord * 0.50),
        ([0.0, foot, zc], chord * 0.46),
    ];
    kids.push(flat_sweep(
        &blade,
        16,
        0,
        l * 0.006 / (chord * 0.5),
        &c.rudder,
        [0.0; 3],
    ));
    // The slots: two columns of three down the blade's middle.
    let d = dim(l * 0.030);
    for row in 0..3 {
        for col in 0..2 {
            let y = foot + h * (0.22 + 0.22 * row as f32);
            let z = zs - chord * (0.30 + 0.32 * col as f32) + rs * 0.4;
            kids.push(prim(
                cuboid([dim(l * 0.012 + 0.004), d, d], c.inlay.clone()),
                [0.0, y, z],
                quat_xyzw(quat_x(FRAC_PI_4)),
            ));
        }
    }
}

/// Her identity from the chase camera: the ROUNDEL on the transom's upper
/// face, a turned disc in the seed's accent (the painted sun on a junk's
/// stern) in a gilt ring.
pub(super) fn roundel(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let at = [
        0.0,
        hull.sheer_z(hull.transom_z()) + l * 0.012,
        transom_aft(hull) + l * 0.002,
    ];
    // A lathe's +Y turned to face aft.
    let aft = quat_x(-FRAC_PI_2);
    let r = l * ROUNDEL_R;
    kids.push(turned(
        &[(r * 1.10, -l * 0.004), (r * 1.10, l * 0.002)],
        20,
        false,
        &c.ring,
        at,
        aft,
        0.0,
    ));
    kids.push(turned(
        &[(r, 0.0), (r, l * 0.005)],
        20,
        false,
        &c.mark,
        at,
        aft,
        0.0,
    ));
}

/// The stern cabin's lit windows on her QUARTERS, two a side on the flared
/// topsides abreast the poop, a hand under the sheer - the chase camera
/// sees the near quarter's, and the transom is left to the roundel. Lit
/// bands, no glass.
pub(super) fn quarter_windows(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    for zf in WINDOWS_ZF {
        let z = zf * l;
        for side in [-1.0f32, 1.0] {
            let (p, n) = side_point(hull, z, 0.30, side);
            kids.push(prim(
                cuboid(
                    [dim(l * 0.050), dim(l * 0.004 + 0.004), dim(l * 0.028)],
                    c.window.clone(),
                ),
                [p[0] - n[0] * l * 0.0015, p[1] - n[1] * l * 0.0015, z],
                quat_xyzw(lay_on(n)),
            ));
        }
    }
}
