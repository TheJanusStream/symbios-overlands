//! The scow's flat-bottomed hull and what every scow carries on it - all of
//! it read off the [`HullProfile`] and the ONE polygon her section is swept
//! as.
//!
//! # The section is a trapezoid, on purpose
//!
//! A Spine's `resolution` counts segments over the KEPT arc
//! (`world_builder/prim/sweeps.rs`), so the lower half-pipe swept at
//! [`HULL_RES`] = 3 has exactly four vertices round its section: the deck
//! edge, a chine 60 degrees round, the other chine, the other deck edge. That
//! is a FLAT BOTTOM half the deck's width under flared sides - a punt - and
//! the `path_cut` caps at her ends are trapezoid transom faces.
//!
//! The polygon's bottom lies at `cos 30` of the swept radius, not at the
//! radius. So the profile keeps `section` as her DEPTH per half-beam - the
//! keel it derives, `sheer - half_beam x section`, is the drawn flat bottom
//! and every read off it is exact - and the node is y-scaled by
//! [`node_section`], which is `section / cos 30`.

use crate::pds::generator::Generator;

use super::super::ScowColours;
use super::super::profile::HullProfile;
use super::super::shape::{DECK_CROWN, UPRIGHT, deck_run, line, panel, sweep};
use super::hold_z;

/// Segments over the hull's kept half-section - see the module docs.
const HULL_RES: u32 = 3;

/// How far down the res-3 polygon reaches per unit of its swept radius:
/// `cos 30`, the flat between the two chines.
const BOTTOM: f32 = 0.866_025_4;

/// The shell is BORED, so the hold is a real well the cargo stands in - a
/// solid sweep's cut face is a flat lid at the sheer. The wall is a tenth of
/// the half-beam.
pub(super) const HOLLOW: f32 = 0.90;

/// The hold floor's depth under the sheer, over the bored shell's inner
/// depth: most of the way down, so the cargo stands IN the hold.
const FLOOR: f32 = 0.55;

/// The hull node's y-scale: her section depth over the polygon's reach.
fn node_section(hull: &HullProfile) -> f32 {
    hull.section / BOTTOM
}

/// The top of a crowned deck run (a [`deck_run`]: radius 0.985 of the
/// half-beam, crown [`DECK_CROWN`]) at `z`, `x` off the centreline.
pub(super) fn deck_y(hull: &HullProfile, z: f32, x: f32) -> f32 {
    let r = hull.half_beam_at(z) * 0.985;
    hull.sheer_z(z) - hull.loa * 0.002 + r * DECK_CROWN * (1.0 - (x / r).powi(2)).max(0.0).sqrt()
}

/// How deep under the sheer the hold floor lies amidships.
pub(super) fn floor_depth(hull: &HullProfile) -> f32 {
    hull.half_beam_at(0.0) * hull.section * HOLLOW * FLOOR
}

/// Half-width of the bored shell's INNER face `depth` under the sheer at `z`:
/// down the flared side from the deck edge to the chine, then the flat
/// bottom's half-width.
pub(super) fn inner_half_width(hull: &HullProfile, z: f32, depth: f32) -> f32 {
    let hb = hull.half_beam_at(z) * HOLLOW;
    let chine = hb * hull.section;
    if depth <= chine {
        hb + (hb * 0.5 - hb) * depth / chine
    } else {
        hb * 0.5
    }
}

/// The structural root. `apply_travel_pose` OVERWRITES the root's
/// translation, so whatever the root is sits at the waterline centre - which
/// on a bored hull is the void under the hold floor. So the root is a SPINE,
/// whose points are its own: a keelson post from the shell's inner bottom up
/// to the floor, honestly touching both.
pub(super) fn root(hull: &HullProfile, c: &ScowColours) -> Generator {
    let l = hull.loa;
    let (za, zf) = hold_z(hull);
    let zm = (za + zf) * 0.5;
    let floor = hull.sheer_z(zm) - floor_depth(hull);
    let bottom = hull.sheer_z(zm) - hull.half_beam_at(zm) * hull.section * HOLLOW;
    line(
        &[
            ([0.0, bottom - l * 0.006, zm], l * 0.02),
            ([0.0, floor, zm], l * 0.02),
        ],
        6,
        &c.interior,
    )
}

/// The bored shell, a solid plug at each end whose cap IS that end's transom
/// face, and the gunwale rail on the deck edge. No line down her topsides: at
/// the chase camera's 22.9 degrees they roll away under the deck edge
/// (#1365), so her identity line is on the deckhouse roof.
pub(super) fn skin(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) {
    let l = hull.loa;
    let sc = [1.0, node_section(hull), 1.0];
    let st = hull.stations();
    let path: Vec<_> = st
        .iter()
        .map(|s| ([0.0, s.sheer, s.z], s.half_beam))
        .collect();
    kids.push(sweep(&path, HULL_RES, sc, [0.5, 1.0], &c.hull, HOLLOW));
    let (t, b) = (hull.transom_z(), hull.stem_z());
    for (z0, z1) in [(t - 0.003, t + l * 0.03), (b - l * 0.03, b + 0.003)] {
        let plug = [
            (
                [0.0, hull.sheer_z(z0), z0],
                hull.half_beam_at(z0.clamp(t, b)) * 0.998,
            ),
            (
                [0.0, hull.sheer_z(z1), z1],
                hull.half_beam_at(z1.clamp(t, b)) * 0.998,
            ),
        ];
        kids.push(sweep(&plug, HULL_RES, sc, [0.5, 1.0], &c.transom, 0.0));
    }
    for side in [-1.0f32, 1.0] {
        let pts: Vec<_> = st
            .iter()
            .map(|s| ([side * s.half_beam * 0.995, s.sheer, s.z], l * 0.0080))
            .collect();
        kids.push(line(&pts, 8, &c.rail));
    }
}

/// The crowned foredeck from the hold to the stem and the after deck from
/// the transom to the hold - the hold is the gap between them.
pub(super) fn decks(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) {
    let l = hull.loa;
    let (za, zf) = hold_z(hull);
    kids.push(deck_run(hull, zf, hull.stem_z() - l * 0.004, &c.deck, 0.0));
    kids.push(deck_run(
        hull,
        hull.transom_z() + l * 0.004,
        za,
        &c.deck,
        0.0,
    ));
}

/// The hold floor inside the bored shell, let into the wall at both sides.
/// Returns its top.
pub(super) fn hold_floor(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) -> f32 {
    let l = hull.loa;
    let (za, zf) = hold_z(hull);
    let depth = floor_depth(hull);
    let zm = (za + zf) * 0.5;
    let y = hull.sheer_z(zm) - depth;
    let wall = hull.half_beam * (1.0 - HOLLOW);
    let w = inner_half_width(hull, za, depth).min(inner_half_width(hull, zf, depth));
    kids.push(panel(
        [2.0 * (w + wall * 0.5), l * 0.012, (zf - za) + l * 0.04],
        &c.floor,
        [0.0, y - l * 0.006, zm],
        UPRIGHT,
        0.0,
    ));
    y
}
