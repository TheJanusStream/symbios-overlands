//! What works the scow: her stern gear - a steering sweep, or on the
//! frontier a small stern wheel - the quant pole she is poled with, the
//! samson post it rests on, and the fire drum burning on a scrap or frontier
//! scow's foredeck. Every piece reads at 12 m, and every rotated one is at
//! uniform scale.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::generator::Generator;

use super::super::super::common::{quat_x, quat_z};
use super::super::ScowColours;
use super::super::profile::HullProfile;
use super::super::shape::{UPRIGHT, line, panel, turned};
use super::house::House;
use super::hull::deck_y;
use super::{BITTS_ZF, FIRE_ZF};

/// The samson post's height over the foredeck, as a fraction of the length.
const POST_H: f32 = 0.050;

/// The sweep's loom runs 30 degrees down astern from its crutch.
const SWEEP_DOWN: f32 = PI / 6.0;

/// A samson post on the foredeck: a square timber post the pole rests on and
/// the mooring line is made fast to.
pub(super) fn bitts(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) {
    let l = hull.loa;
    let z = BITTS_ZF * l;
    let y = deck_y(hull, z, 0.0);
    let h = l * POST_H;
    kids.push(panel(
        [l * 0.030, h + l * 0.010, l * 0.030],
        &c.post,
        [0.0, y + h * 0.5 - l * 0.004, z],
        UPRIGHT,
        l * 0.004,
    ));
}

/// The quant pole - her propulsion, drawn - lying from the deckhouse roof to
/// the samson post.
pub(super) fn pole(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours, h: &House) {
    let l = hull.loa;
    let z1 = BITTS_ZF * l;
    let y1 = deck_y(hull, z1, 0.0) + l * POST_H + l * 0.001;
    let z0 = h.za + h.ln * 0.2;
    let xr = h.hw * 0.62;
    let y0 = h.roof_y(xr) + l * 0.005;
    kids.push(line(
        &[
            ([xr, y0, z0], l * 0.0060),
            ([0.0, y1, z1 + l * 0.03], l * 0.0060),
        ],
        6,
        &c.oar,
    ));
}

/// A steering sweep: an iron crutch post on the transom, the loom from a
/// handle over the steering deck through the crutch and down astern, and a
/// painted blade in the water - the seed's accent.
pub(super) fn sweep_oar(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) {
    let l = hull.loa;
    let zc = hull.transom_z() + l * 0.025;
    let dy = deck_y(hull, zc, 0.0);
    let pivot = [0.0, dy + l * 0.075, zc];
    kids.push(line(
        &[
            ([0.0, dy - l * 0.008, zc], l * 0.011),
            ([0.0, pivot[1] + l * 0.006, zc], l * 0.010),
        ],
        6,
        &c.iron,
    ));
    // The loom, handle forward and blade aft, through the pivot.
    let (dy_, dz) = (-SWEEP_DOWN.sin(), -SWEEP_DOWN.cos());
    let (fwd, aft) = (l * 0.13, l * 0.34);
    let handle = [0.0, pivot[1] - dy_ * fwd, pivot[2] - dz * fwd];
    let end = [0.0, pivot[1] + dy_ * aft, pivot[2] + dz * aft];
    kids.push(line(
        &[(handle, l * 0.0065), (pivot, l * 0.0080), (end, l * 0.0075)],
        8,
        &c.oar,
    ));
    // The blade: a board along the loom, its root on the loom's end.
    // `quat_x(-SWEEP_DOWN)` takes the board's +Z onto the loom's direction.
    let bl = l * 0.13;
    kids.push(panel(
        [0.012, l * 0.050, bl],
        &c.blade,
        [0.0, end[1] + dy_ * bl * 0.40, end[2] + dz * bl * 0.40],
        quat_x(-SWEEP_DOWN),
        0.0,
    ));
}

/// A small stern wheel on two wheel beams run aft from the quarters: an
/// axle, a hub, two iron rims (bored Lathe bands) in the seed's accent and
/// eight paddles as four boards through the axle - one board missing on a
/// `broken` (battered) wheel. Her bottom paddle is just in the water.
pub(super) fn paddle_wheel(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    broken: bool,
) {
    let l = hull.loa;
    let t = hull.transom_z();
    let r = l * 0.085;
    let w = hull.half_beam_at(t) * 2.0 * 0.66;
    let xb = w * 0.5 + l * 0.012;
    let z_ax = t - r - l * 0.022;
    let y_ax = r - l * 0.022;
    let zb = t + l * 0.10;
    for side in [-1.0f32, 1.0] {
        kids.push(line(
            &[
                ([side * xb, hull.sheer_z(zb) - l * 0.004, zb], l * 0.011),
                ([side * xb, y_ax + l * 0.012, t - l * 0.01], l * 0.010),
                ([side * xb, y_ax, z_ax - r * 0.30], l * 0.010),
            ],
            8,
            &c.beam,
        ));
    }
    kids.push(line(
        &[
            ([-xb - l * 0.004, y_ax, z_ax], l * 0.0075),
            ([xb + l * 0.004, y_ax, z_ax], l * 0.0075),
        ],
        6,
        &c.iron,
    ));
    kids.push(turned(
        &[(r * 0.26, -w * 0.30), (r * 0.26, w * 0.30)],
        12,
        false,
        &c.iron,
        [0.0, y_ax, z_ax],
        quat_z(FRAC_PI_2),
        0.0,
    ));
    for side in [-1.0f32, 1.0] {
        kids.push(turned(
            &[(r, -l * 0.005), (r, l * 0.005)],
            20,
            false,
            &c.wheel,
            [side * w * 0.47, y_ax, z_ax],
            quat_z(FRAC_PI_2),
            0.86,
        ));
    }
    for k in 0..4 {
        if broken && k == 1 {
            continue;
        }
        kids.push(panel(
            [w * 0.96, 0.012, r * 1.96],
            &c.paddle,
            [0.0, y_ax, z_ax],
            quat_x(PI * k as f32 / 4.0 + PI / 8.0),
            0.0,
        ));
    }
}

/// The fire drum's radius and height, as fractions of the length.
const DRUM_R: f32 = 0.028;
const DRUM_H: f32 = 0.060;

/// Where the fire drum stands on the foredeck.
fn drum_foot(hull: &HullProfile) -> [f32; 3] {
    let z = FIRE_ZF * hull.loa;
    [0.0, deck_y(hull, z, 0.0) - hull.loa * 0.004, z]
}

/// The fire drum's mouth - the Embers mount.
pub(super) fn fire_mouth(hull: &HullProfile) -> [f32; 3] {
    let [x, y, z] = drum_foot(hull);
    [x, y + hull.loa * DRUM_H, z]
}

/// A fire drum on the foredeck, burning: a rusted drum with a rolled hoop,
/// bored open, and the embers glowing inside its mouth.
pub(super) fn brazier(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) {
    let l = hull.loa;
    let foot = drum_foot(hull);
    let (r, h) = (l * DRUM_R, l * DRUM_H);
    kids.push(turned(
        &[
            (r, 0.0),
            (r, h * 0.5),
            (r * 1.04, h * 0.52),
            (r, h * 0.55),
            (r, h),
        ],
        12,
        false,
        &c.drum_fire,
        foot,
        UPRIGHT,
        0.86,
    ));
    kids.push(turned(
        &[(r * 0.90, h * 0.80), (r * 0.90, h * 0.90)],
        12,
        false,
        &c.ember,
        foot,
        UPRIGHT,
        0.0,
    ));
}
