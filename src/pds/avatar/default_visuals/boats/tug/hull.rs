//! The tug's round-bilged hull and what every tug carries on it - all of it
//! read off the [`HullProfile`] and the ONE polygon her section is swept as.
//!
//! # The section is a round bilge, and its wall is the bulwark
//!
//! A Spine's `resolution` counts segments over the KEPT arc
//! (`world_builder/prim/sweeps.rs`), so the lower half-pipe swept at
//! [`HULL_RES`] = 20 has twenty-one vertices round its section, vertex `k`
//! at `pi k / 20`. The count is even, so one vertex is the keel at the full
//! radius and the profile's `section` IS the hull node's y-scale: the keel it
//! derives, `sheer - half_beam x section`, is the drawn canoe body with no
//! correction (the scow's odd resolution needs one).
//!
//! The shell is BORED, and ONE crowned deck is sunk under the sheer and
//! bedded into the wall at its depth, so the shell's own wall stands as her
//! bulwark: a low rail round the towing deck, climbing with the sheer's rise
//! to the bow ([`bulwark`]). No bulwark part. The deck's edge and every
//! fender are read off the polygon itself ([`inner_half_width`]); a circle
//! would move them by up to 1.4 mm.
//!
//! # The forefoot and the stem are wedges
//!
//! Depth is proportional to radius in one sweep, so a fine bow is a shallow
//! forefoot. The plumb stem, the deep forefoot, the keel and the deadwood are
//! their own parts: two thin Wedges whose slopes lie INSIDE the canoe body,
//! so what shows is exactly what the shell cannot draw ([`keel`]). A sweep
//! could not do it: its end cap tilts with its path, and a keel line rising
//! as steeply as a bow's drew the forefoot as a ram.

use std::f32::consts::PI;

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::super::super::common::{prim, quat_xyzw, wedge};
use super::super::profile::{HullProfile, SHEER_LOW};
use super::super::shape::{DECK_CROWN, hull_path, line, run_z, sweep};
use super::super::{TugColours, dim};

/// Segments over the hull's kept half-section - see the module docs.
const HULL_RES: u32 = 20;

/// The shell is BORED to this fraction of its radius, and its wall is the
/// bulwark.
pub(super) const HOLLOW: f32 = 0.92;

/// The middle of the shell's wall, as a fraction of the radius: where the
/// capping rail runs and the towing arches stand.
pub(super) const WALL: f32 = (1.0 + HOLLOW) * 0.5;

/// Where the antifoul starts round the section: `path_cut` 0.5 is one deck
/// edge, 0.75 the keel and 1.0 the other, so 0.58 is 29 degrees down - on
/// the waterline at the sheer's low point, a little above it toward the ends.
const BOOT_F: f32 = 0.58;

/// How far the deck lies under the sheer aft - a low rail round the working
/// deck - as a fraction of the length, and the share of the sheer's own rise
/// the bulwark keeps forward of its low point.
const BW_AFT: f32 = 0.020;
const BW_RISE: f32 = 0.55;

/// The stem bar and the keel plate's thickness, as a fraction of the length.
const KEEL_T: f32 = 0.010;

/// The forefoot's depth over the heel's: a tug drags her keel aft.
const FOREFOOT: f32 = 0.82;

/// The sternpost, the keel's after end where the counter starts, as a
/// fraction of the length.
const POST_ZF: f32 = -0.400;

/// Half-width of the shell `depth` under the sheer at `z`, on the section
/// polygon grown by `grow` - 1.0 for the outer skin, [`HOLLOW`] for the
/// bore's inner face. It walks the polygon's own vertices (vertex `k` at
/// `pi k / 20` round the section: `hb cos` across, `hb x section x sin`
/// down) and runs straight between the two that bracket the depth, because
/// that is what the mesher draws.
pub(super) fn inner_half_width(hull: &HullProfile, z: f32, depth: f32, grow: f32) -> f32 {
    let hb = hull.half_beam_at(z) * grow;
    let vertex = |k: u32| {
        let a = PI * k as f32 / HULL_RES as f32;
        (hb * a.cos(), hb * hull.section * a.sin())
    };
    let mut prev = vertex(0);
    if depth <= prev.1 {
        return hb;
    }
    for k in 1..=HULL_RES / 2 {
        let next = vertex(k);
        if depth <= next.1 {
            return prev.0 + (next.0 - prev.0) * (depth - prev.1) / (next.1 - prev.1);
        }
        prev = next;
    }
    // Past the keel: the keel vertex, on the centreline.
    prev.0
}

/// How far the deck lies under the sheer at `z`: a low rail aft, and forward
/// of the sheer's low point [`BW_RISE`] of the sheer's own rise on top - so
/// the bulwark climbs to the stem as the deck line springs.
pub(super) fn bulwark(hull: &HullProfile, z: f32) -> f32 {
    let rise = if z >= SHEER_LOW * hull.loa {
        hull.sheer_z(z) - hull.freeboard
    } else {
        0.0
    };
    hull.loa * BW_AFT + BW_RISE * rise.max(0.0)
}

/// The sunk deck's edge height at `z`.
pub(super) fn deck_line(hull: &HullProfile, z: f32) -> f32 {
    hull.sheer_z(z) - bulwark(hull, z)
}

/// The sunk deck's half-width at `z`: bedded halfway into the wall at its
/// depth.
pub(super) fn deck_edge(hull: &HullProfile, z: f32) -> f32 {
    let d = bulwark(hull, z);
    (inner_half_width(hull, z, d, HOLLOW) + inner_half_width(hull, z, d, 1.0)) * 0.5
}

/// The top of the crowned deck at `z`, `x` off the centreline.
pub(super) fn deck_y(hull: &HullProfile, z: f32, x: f32) -> f32 {
    let r = deck_edge(hull, z);
    deck_line(hull, z) + r * DECK_CROWN * (1.0 - (x / r).powi(2)).max(0.0).sqrt()
}

/// The structural root. `apply_travel_pose` OVERWRITES the root's
/// translation, so whatever the root is sits at the waterline centre - which
/// on a bored hull is the void under the deck. So the root is a SPINE, whose
/// points are its own: a keelson post from the shell's inner bottom up to the
/// deck amidships, honestly touching both.
pub(super) fn root(hull: &HullProfile, c: &TugColours) -> Generator {
    let l = hull.loa;
    let bottom = hull.sheer_z(0.0) - hull.half_beam_at(0.0) * hull.section * HOLLOW;
    line(
        &[
            ([0.0, bottom - l * 0.006, 0.0], l * 0.02),
            ([0.0, deck_line(hull, 0.0), 0.0], l * 0.02),
        ],
        6,
        &c.interior,
    )
}

/// The bored shell - its wall IS the bulwark - the antifoul a hair proud and
/// bored the same, a solid plug whose cap is the counter's end face, and the
/// capping rail along the bulwark's top.
pub(super) fn skin(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours) {
    let l = hull.loa;
    let sc = [1.0, hull.section, 1.0];
    kids.push(sweep(
        &hull_path(hull, 1.0, 0.0, None),
        HULL_RES,
        sc,
        [0.5, 1.0],
        &c.hull,
        HOLLOW,
    ));
    // The antifoul: the sloop's idiom, a second sweep of the same stations a
    // hair proud, cut from BOOT_F round the section and stopped short of the
    // counter so its cap is buried in the plug.
    let aft = hull.transom_z() + l * 0.004;
    kids.push(sweep(
        &hull_path(hull, 1.008, 0.0, Some(aft)),
        HULL_RES,
        sc,
        [BOOT_F, 1.0 - (BOOT_F - 0.5)],
        &c.antifoul,
        HOLLOW,
    ));
    // The counter plug: a short SOLID sweep of the after stations a hair
    // inside the shell, standing 3 mm proud astern - its after cap IS the
    // counter's end face, so the bored shell's annulus never shows.
    let t = hull.transom_z();
    let fwd = t + l * 0.03;
    let plug = [
        (
            [0.0, hull.sheer_z(t), t - 0.003],
            hull.half_beam_at(t) * 0.998,
        ),
        (
            [0.0, hull.sheer_z(fwd), fwd],
            hull.half_beam_at(fwd) * 0.998,
        ),
    ];
    kids.push(sweep(&plug, HULL_RES, sc, [0.5, 1.0], &c.transom, 0.0));
    let st = hull.stations();
    for side in [-1.0f32, 1.0] {
        let pts: Vec<_> = st
            .iter()
            .map(|s| {
                (
                    [side * s.half_beam * WALL, s.sheer + l * 0.002, s.z],
                    l * 0.0075,
                )
            })
            .collect();
        kids.push(line(&pts, 8, &c.rail));
    }
}

/// ONE crowned deck from the counter to the stem, sunk under the sheer and
/// bedded into the wall at its depth - the casing stands on it.
pub(super) fn deck(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours) {
    let l = hull.loa;
    let pts: Vec<_> = run_z(hull, hull.transom_z() + l * 0.03, hull.stem_z() - l * 0.012)
        .into_iter()
        .map(|z| ([0.0, deck_line(hull, z), z], deck_edge(hull, z)))
        .collect();
    kids.push(sweep(
        &pts,
        14,
        [1.0, DECK_CROWN, 1.0],
        [0.0, 0.5],
        &c.deck,
        0.0,
    ));
}

/// The plumb stem, the deep forefoot, the keel and the deadwood, as two thin
/// Wedges whose slopes lie INSIDE the canoe body - see the module docs.
///
/// Below the waterline, in the bottom paint: the forefoot, and the keel
/// deepest at the heel. Above it, in the hull's own paint: the stem, filling
/// the shell's rounded bow out to a plumb stem head - one wedge to the stem
/// head drew a red blade up the bow.
pub(super) fn keel(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours) {
    let l = hull.loa;
    let zs = hull.stem_z() + l * 0.004;
    let t = dim(l * KEEL_T);
    kids.push(wedge_at(
        &c.antifoul,
        (zs, -hull.draft * FOREFOOT, l * 0.002),
        (POST_ZF * l, -hull.draft),
        t,
    ));
    kids.push(wedge_at(
        &c.hull,
        (zs, 0.0, hull.sheer_z(hull.stem_z()) + l * 0.004),
        (hull.waterline().1, 0.0),
        t,
    ));
}

/// A Wedge `t` thick, turned end for end: its upright face at `zs` from `y0`
/// up to `top`, its bottom from `(zs, y0)` aft to `(zb, yb)`, and its slope
/// from the top of the face down to the after end of the bottom.
///
/// The prim's upright face is its `-Z` one, so it is turned a half turn about
/// `y` and then pitched by the bottom's slope `a` - in closed form,
/// `[0, cos a/2, -sin a/2, 0]`, whose `w` and `x` are exactly zero where
/// composing two quaternions leaves noise in them. Its centre is the point
/// its right-angle edge lands on, less that edge's rotated offset from it.
fn wedge_at(
    m: &SovereignMaterialSettings,
    (zs, y0, top): (f32, f32, f32),
    (zb, yb): (f32, f32),
    t: f32,
) -> Generator {
    let run = zs - zb;
    let a = (y0 - yb).atan2(run);
    let ln = run / a.cos();
    let h = top - y0;
    let (sin_half, cos_half) = (a * 0.5).sin_cos();
    let at = [
        0.0,
        y0 + h * 0.5 * a.cos() - ln * 0.5 * a.sin(),
        zs - h * 0.5 * a.sin() - ln * 0.5 * a.cos(),
    ];
    prim(
        wedge([t, h, ln].map(dim), m.clone()),
        at,
        quat_xyzw([0.0, cos_half, -sin_half, 0.0]),
    )
}
