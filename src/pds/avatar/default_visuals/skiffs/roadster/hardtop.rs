//! The hardtop: a body-coloured cabin on the canopy seat, glazed with a lit
//! window band - never a glass box (#1359 rule 4, #1367).
//!
//! [`RoadsterPlan::canopy_seat`] was published in #1364 with nothing reading it,
//! so that a canopy could never again choose its own height and hover over a
//! body it does not fit. This is its first reader: the cabin stands on that
//! plane, centred on that station, as wide as the body is there and as long as
//! the cockpit it closes.
//!
//! # Why a Superellipsoid
//!
//! The phase-1 prototype tried the family's own idiom first - an upper
//! half-pipe swept over the cockpit's stations, sprung out of the scuttle and
//! dying into the tail deck - and it failed twice by render, for one reason: a
//! swept dome has no upright side. From the chase camera's 22.9 degree pitch a
//! window band low on it read as a glowing ring round a pod, and one high on it
//! as a hatch rim. A boxy Superellipsoid - the "pressed panel" the redesign's
//! rules allow - has a flat roof on near-upright sides, and a band on those
//! sides reads as the glazing of a closed car. A taper draws its top in, so the
//! screens rake fore and aft and the sides tumble home.
//!
//! The connectedness guard resolves a Superellipsoid as its box; the drawn
//! cabin is inside that box everywhere, so there the guard can over-count
//! contact but never under-count it.

use std::f32::consts::TAU;

use crate::pds::generator::Generator;

use super::super::super::common::{id_quat, prim, superellipsoid, with_shape};
use super::super::SkiffColours;
use super::RoadsterPlan;
use super::{line, sweep};

/// The cabin's vertical profile: small is a flat roof on upright sides.
const CABIN_NS: f32 = 0.30;
/// Its plan: a rounded rectangle.
const CABIN_EW: f32 = 0.45;
/// Tumblehome across and the screens' rake fore and aft - the taper toward the
/// top, `[x, z]`.
const CABIN_TAPER: [f32; 2] = [0.10, 0.22];
/// How far it is bedded into the coaming, as a fraction of the length.
const CABIN_BED: f32 = 0.010;
/// Its half-width over the body's at the canopy seat - a hand inside the
/// flank, so the body's shoulder shows under it.
const CABIN_WIDTH: f32 = 0.94;
/// How far past each end of the cockpit it reaches, as a fraction of the
/// length, so its rounded ends close the opening.
const CABIN_OVERHANG: f32 = 0.01;
/// The window band's centre and half-height, as fractions of the cabin's
/// height.
const WINDOW_AT: f32 = 0.56;
const WINDOW_HALF: f32 = 0.11;
/// Stations round the band's closed loop, the first repeated as the last.
const WINDOW_STATIONS: usize = 15;
/// The door pillars' thickness fore and aft, as a node scale on a round tube -
/// a flattened pillar is no rotated node, so the guard's similarity rule holds.
const PILLAR_FLATTEN: f32 = 0.35;

/// The cabin's centre and half extents, all read off the plan: the canopy
/// seat (its plane and its station), the cockpit's length, the body's own
/// half-width there, and the screen top the blueprint sets.
fn cabin(plan: &RoadsterPlan) -> ([f32; 3], [f32; 3]) {
    let l = plan.length;
    let [_, base, centre] = plan.canopy_seat();
    let (aft, fwd) = plan.cockpit_z();
    let bed = CABIN_BED * l;
    let roof = plan.screen_top() - base;
    let half = [
        plan.half_width_at(centre) * CABIN_WIDTH,
        (roof + bed) * 0.5,
        (fwd - aft) * 0.5 + CABIN_OVERHANG * l,
    ];
    ([0.0, base - bed + half[1], centre], half)
}

/// The top of the cabin's roof (m above the datum) - where a decorative aura
/// hovers over a closed car rather than inside it.
pub(super) fn roof_y(plan: &RoadsterPlan) -> f32 {
    let (centre, half) = cabin(plan);
    centre[1] + half[1]
}

/// Sign-preserving power, the Superellipsoid's own parametrisation - with a
/// vanishing base snapped to zero. In f32, `sin(PI)` is about 1e-7 rather than
/// nought, and a fractional power turns that residue into a third of a
/// millimetre: the window loop came back open at its seam and a hair off the
/// flank where it crosses it.
fn spow(v: f32, e: f32) -> f32 {
    if v.abs() < 1e-6 {
        0.0
    } else {
        v.signum() * v.abs().powf(e)
    }
}

/// A point on the TAPERED cabin at height `y` over its centre and longitude
/// `omega`: `(x, z)` relative to the centre. The DRAWN surface - the section
/// the Superellipsoid's own parametrisation gives at that height, shrunk by
/// the taper at that height - so a band laid on it stays on it.
fn cabin_point(half: [f32; 3], y: f32, omega: f32) -> (f32, f32) {
    let [ax, ay, az] = half;
    let t = (y + ay) / (2.0 * ay);
    let s = (1.0 - (y / ay).abs().powf(2.0 / CABIN_NS))
        .max(0.0)
        .powf(CABIN_NS / 2.0);
    let (kx, kz) = (1.0 - CABIN_TAPER[0] * t, 1.0 - CABIN_TAPER[1] * t);
    (
        ax * s * spow(omega.cos(), CABIN_EW) * kx,
        az * s * spow(omega.sin(), CABIN_EW) * kz,
    )
}

/// The cabin, its lit window band and a door pillar a side.
pub(super) fn build(kids: &mut Vec<Generator>, plan: &RoadsterPlan, c: &SkiffColours) {
    let (centre, half) = cabin(plan);
    kids.push(prim(
        with_shape(
            superellipsoid(half, CABIN_NS, CABIN_EW, 16, 24, c.paint.clone()),
            CABIN_TAPER,
            [0.0; 3],
            [0.0; 2],
        ),
        centre,
        id_quat(),
    ));
    // The glazing: ONE closed lit loop round the cabin at window height, in
    // the lamp slot - which is already the lit window material - lying ON the
    // tapered surface. Where it crosses the front and back it is the
    // windscreen and the rear light.
    let height = 2.0 * half[1];
    let y = height * WINDOW_AT - half[1];
    let band = height * WINDOW_HALF;
    let loop_: Vec<([f32; 3], f32)> = (0..WINDOW_STATIONS)
        .map(|k| {
            let omega = TAU * k as f32 / (WINDOW_STATIONS - 1) as f32;
            let (x, z) = cabin_point(half, y, omega);
            ([x, centre[1] + y, centre[2] + z], band)
        })
        .collect();
    kids.push(line(&loop_, 8, c.lamp.clone()));
    // A body-coloured door pillar a side, standing on the cabin's flank across
    // the band just abaft the canopy seat - fatter than the band, so it
    // breaks the band into panes.
    for side in [-1.0f32, 1.0] {
        let column: Vec<([f32; 3], f32)> = [-1.9f32, 0.0, 1.9]
            .iter()
            .map(|&dy| {
                let yy = y + dy * band;
                let (x, _) = cabin_point(half, yy, 0.0);
                (
                    [side * x, centre[1] + yy, centre[2] - half[2] * 0.12],
                    band * 1.25,
                )
            })
            .collect();
        kids.push(sweep(
            &column,
            8,
            [1.0, 1.0, PILLAR_FLATTEN],
            [0.0, 1.0],
            0.0,
            c.paint.clone(),
        ));
    }
}
