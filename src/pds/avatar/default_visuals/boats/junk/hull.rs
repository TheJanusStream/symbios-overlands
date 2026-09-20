//! The junk's flat-bottomed hull and her two decks - all of it read off the
//! [`HullProfile`] and the ONE polygon her section is swept as.
//!
//! # The section is the scow's trapezoid
//!
//! A Spine's `resolution` counts segments over the KEPT arc
//! (`world_builder/prim/sweeps.rs`), so the lower half-pipe swept at
//! [`HULL_RES`] = 3 has four vertices round its section: the deck edge, a
//! chine 60 degrees round, the other chine, the other deck edge. That is a
//! FLAT BOTTOM half her deck's width under flared sides and hard chines - a
//! junk has no keel. The polygon's bottom lies at `cos 30` of the swept
//! radius, so the profile keeps `section` as her depth per half-beam (the
//! keel it derives is the drawn flat bottom) and the node is y-scaled by
//! [`node_section`]: the scow's idiom (#1373).
//!
//! # A sunk main deck, and a level poop over the break
//!
//! The shell is BORED and its own wall stands as her bulwark round a sunk,
//! crowned main deck that climbs only part of the sheer's rise aft, so the
//! bulwark grows toward the poop (the tug's idiom, #1370). Abaft the break
//! the POOP deck is a second crowned sweep a step higher, and LEVEL: a
//! sweep's sections are perpendicular to its local path, and the crown's
//! y-scale makes the pre-divided path six times steeper than the deck, so a
//! crowned deck laid up the stern's rise came out a dome bulging a whole
//! radius past its ends, out through the transom. And it can only be as low
//! as the stern's inner bottom lets it - depth is proportional to half-beam
//! in one sweep, so her stern's bottom climbs with her sheer - which is what
//! [`SheerLaw::Poop`](super::super::profile::SheerLaw::Poop) is for. The
//! break between the two decks is a bulkhead [`board`].

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::super::super::common::quat_z;
use super::super::JunkColours;
use super::super::profile::{HullProfile, SHEER_LOW};
use super::super::shape::{DECK_CROWN, flat_sweep, hull_path, line, sweep};

/// Segments over the hull's kept half-section - see the module docs.
const HULL_RES: u32 = 3;

/// How far down the res-3 polygon reaches per unit of its swept radius:
/// `cos 30`, the flat between the two chines.
const BOTTOM: f32 = 0.866_025_4;

/// The shell is BORED to this fraction of its radius, and its wall is the
/// bulwark.
const HOLLOW: f32 = 0.92;

/// The middle of the shell's wall, as a fraction of the radius: where the
/// capping rail runs.
const WALL: f32 = (1.0 + HOLLOW) * 0.5;

/// The poop's break, as a fraction of the length: the main deck forward of
/// it, the poop deck abaft it.
const POOP_ZF: f32 = -0.280;

/// How far the main deck lies under the sheer at the sheer's low point, as
/// a fraction of the length, and the share of the sheer's rise abaft the low
/// point that the deck does NOT climb - so the bulwark over it grows toward
/// the poop.
const BW_MAIN: f32 = 0.022;
const BW_TAKE: f32 = 0.55;

/// How far the poop deck lies under the sheer at the break, as a fraction of
/// the length: a clear step up from the main deck, its rail growing aft as
/// the sheer climbs round it.
const BW_POOP: f32 = 0.012;

/// The hull node's y-scale: her section depth over the polygon's reach.
fn node_section(hull: &HullProfile) -> f32 {
    hull.section / BOTTOM
}

/// The canoe body's depth under the sheer at `z`.
fn depth_at(hull: &HullProfile, z: f32) -> f32 {
    hull.half_beam_at(z) * hull.section
}

/// Half-width of the shell `depth` under the sheer at `z`, on the section
/// polygon grown by `grow` - [`HOLLOW`] for the bore's inner face, 1.0 for
/// the outer skin: the deck edge's own at or over the sheer, down the flared
/// side to the chine, and the flat bottom's half-width under it.
fn inner_half_width(hull: &HullProfile, z: f32, depth: f32, grow: f32) -> f32 {
    let hb = hull.half_beam_at(z) * grow;
    let chine = hb * hull.section;
    if depth <= 0.0 {
        hb
    } else if depth <= chine {
        hb + (hb * 0.5 - hb) * depth / chine
    } else {
        hb * 0.5
    }
}

/// The main deck's height at `z`: [`BW_MAIN`] under the sheer at its low
/// point, and abaft that climbing only part of the sheer's rise.
pub(super) fn main_deck(hull: &HullProfile, z: f32) -> f32 {
    let l = hull.loa;
    let take = if z < SHEER_LOW * l { BW_TAKE } else { 0.0 };
    hull.sheer_z(z) - l * BW_MAIN - take * (hull.sheer_z(z) - hull.freeboard).max(0.0)
}

/// The poop deck's height: LEVEL (see the module docs), [`BW_POOP`] under
/// the sheer at the break.
pub(super) fn poop_deck(hull: &HullProfile) -> f32 {
    let l = hull.loa;
    let t = hull.transom_z();
    let y = hull.sheer_z(POOP_ZF * l) - l * BW_POOP;
    // Level, it can be no lower than the stern's inner bottom, which climbs
    // with her sheer: the Poop law keeps it clear, and this is what says so
    // if a number moves.
    debug_assert!(
        y >= hull.sheer_z(t) - depth_at(hull, t) * HOLLOW + l * 0.008,
        "the poop deck ({y} m) is under the stern's inner bottom"
    );
    y
}

/// The deck at `z`: the poop's abaft the break, the main deck's forward.
pub(super) fn deck_level(hull: &HullProfile, z: f32) -> f32 {
    if z <= POOP_ZF * hull.loa {
        poop_deck(hull)
    } else {
        main_deck(hull, z)
    }
}

/// A sunk deck's half-width at `z` and height `y`: bedded halfway into the
/// wall at its depth (the tug's rule).
pub(super) fn deck_edge(hull: &HullProfile, z: f32, y: f32) -> f32 {
    let d = hull.sheer_z(z) - y;
    (inner_half_width(hull, z, d, HOLLOW) + inner_half_width(hull, z, d, 1.0)) * 0.5
}

/// The top of the crowned deck at `z`, `x` off the centreline.
pub(super) fn deck_y(hull: &HullProfile, z: f32, x: f32) -> f32 {
    let y = deck_level(hull, z);
    let r = deck_edge(hull, z, y);
    y + r * DECK_CROWN * (1.0 - (x / r).powi(2)).max(0.0).sqrt()
}

/// A point on the flared topsides at `z`, `f` of the way down the facet
/// from the deck edge to the chine, on `side` (`-1` port, `+1` starboard),
/// and the facet's outward normal there - where an eye or a window lies.
pub(super) fn side_point(hull: &HullProfile, z: f32, f: f32, side: f32) -> ([f32; 3], [f32; 3]) {
    let (hb, sh) = (hull.half_beam_at(z), hull.sheer_z(z));
    let (x0, y0) = (hb, sh);
    let (x1, y1) = (hb * 0.5, sh - depth_at(hull, z));
    let p = [side * (x0 + (x1 - x0) * f), y0 + (y1 - y0) * f, z];
    // Outward: perpendicular to the facet.
    let (nx, ny) = (y0 - y1, x0 - x1);
    let k = nx.hypot(ny);
    (p, [side * nx / k, -ny / k, 0.0])
}

/// The rotation that stands a part's local `+Y` along `normal` - a facet's,
/// athwartships and down - as one turn about `z`.
pub(super) fn lay_on(normal: [f32; 3]) -> [f32; 4] {
    quat_z((-normal[0]).atan2(normal[1]))
}

/// Where the bottom paint starts round the section: `path_cut` 0.5 is one
/// deck edge, and the flared side runs from there to the chine at 0.5 + 1/6
/// round the res-3 polygon - so the paint starts where the design waterline
/// crosses the side at the sheer's low point.
fn boot_f(hull: &HullProfile) -> f32 {
    let frac = hull.freeboard / depth_at(hull, SHEER_LOW * hull.loa);
    0.5 + frac.min(1.0) / 6.0
}

/// The structural root. `apply_travel_pose` OVERWRITES the root's
/// translation, so whatever the root is sits at the waterline centre - which
/// on a bored hull is the void under the deck. So the root is a SPINE, whose
/// points are its own: a keelson post from the shell's inner bottom up to the
/// main deck amidships, honestly touching both.
pub(super) fn root(hull: &HullProfile, c: &JunkColours) -> Generator {
    let l = hull.loa;
    let bottom = hull.sheer_z(0.0) - depth_at(hull, 0.0) * HOLLOW;
    line(
        &[
            ([0.0, bottom - l * 0.006, 0.0], l * 0.02),
            ([0.0, main_deck(hull, 0.0), 0.0], l * 0.02),
        ],
        6,
        &c.interior,
    )
}

/// The bored shell - its wall IS the bulwark - the bottom paint a hair proud
/// and bored the same, a solid plug whose cap is the transom face, a solid
/// plug at the bow whose cap is her flat headboard, and the capping rail
/// along the wall.
pub(super) fn skin(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let sc = [1.0, node_section(hull), 1.0];
    kids.push(sweep(
        &hull_path(hull, 1.0, 0.0, None),
        HULL_RES,
        sc,
        [0.5, 1.0],
        &c.hull,
        HOLLOW,
    ));
    // The bottom paint: a second sweep of the same stations a hair proud, cut
    // from the waterline round the flat bottom and stopped short of both
    // plugs, so its caps are buried in them.
    let (t, b) = (hull.transom_z(), hull.stem_z());
    let (aft, fwd) = (t + l * 0.004, b - l * 0.004);
    let mut bottom = vec![(
        [0.0, hull.sheer_z(aft), aft],
        hull.half_beam_at(aft) * 1.008,
    )];
    bottom.extend(
        hull.stations()
            .iter()
            .filter(|s| aft < s.z && s.z < fwd)
            .map(|s| ([0.0, s.sheer, s.z], s.half_beam * 1.008)),
    );
    bottom.push((
        [0.0, hull.sheer_z(fwd), fwd],
        hull.half_beam_at(fwd) * 1.008,
    ));
    let bf = boot_f(hull);
    kids.push(sweep(
        &bottom,
        HULL_RES,
        sc,
        [bf, 1.5 - bf],
        &c.antifoul,
        HOLLOW,
    ));
    // The plugs: short SOLID sweeps a hair inside the shell, each standing
    // 3 mm proud of its end - their caps ARE the transom face and the flat
    // headboard, so the bored shell's annulus never shows.
    for (z0, z1, m) in [
        (t - 0.003, t + l * 0.03, &c.transom),
        (b - l * 0.03, b + 0.003, &c.hull),
    ] {
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
        kids.push(sweep(&plug, HULL_RES, sc, [0.5, 1.0], m, 0.0));
    }
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

/// The stations strictly between `z0` and `z1`, bracketed by both ends -
/// `shape::run_z`'s, except that a station within a micrometre of an end
/// is taken as that end rather than drawn a second time on top of it. The
/// main deck's after end lies ON the -0.30 L station: a strict comparison
/// keeps or drops that station by the last bit of two roundings, and a kept
/// one is a zero-length segment the Catmull-Rom path loops back through.
fn run_z(hull: &HullProfile, z0: f32, z1: f32) -> Vec<f32> {
    let mut zs = vec![z0];
    zs.extend(
        hull.stations()
            .iter()
            .map(|s| s.z)
            .filter(|&z| z0 + 1e-6 < z && z < z1 - 1e-6),
    );
    zs.push(z1);
    zs
}

/// The main deck, crowned, from under the poop to the headboard, sunk under
/// the sheer and bedded into the wall at its depth; the poop deck the same
/// from the transom's plug to the break, a step higher and level; and the
/// break's bulkhead between them.
pub(super) fn decks(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let zb = POOP_ZF * l;
    let main: Vec<_> = run_z(hull, zb - l * 0.02, hull.stem_z() - l * 0.012)
        .into_iter()
        .map(|z| {
            let y = main_deck(hull, z);
            ([0.0, y, z], deck_edge(hull, z, y))
        })
        .collect();
    kids.push(sweep(
        &main,
        14,
        [1.0, DECK_CROWN, 1.0],
        [0.0, 0.5],
        &c.deck,
        0.0,
    ));
    let poop = poop_deck(hull);
    let aft: Vec<_> = run_z(hull, hull.transom_z() + l * 0.03, zb + l * 0.004)
        .into_iter()
        .map(|z| ([0.0, poop, z], deck_edge(hull, z, poop)))
        .collect();
    kids.push(sweep(
        &aft,
        14,
        [1.0, DECK_CROWN, 1.0],
        [0.0, 0.5],
        &c.poop,
        0.0,
    ));
    // The break: a bulkhead from under the main deck up to the poop deck's
    // crown, its edges in the wall.
    kids.push(board(
        hull,
        &c.bulkhead,
        &Board {
            z: zb,
            y0: main_deck(hull, zb) - l * 0.01,
            y1: poop + deck_edge(hull, zb, poop) * DECK_CROWN * 0.9,
            wall: 0.5,
            grow: 1.0,
            thick: 0.012,
            cap: None,
        },
    ));
}

/// An athwartships board across the hull, thin fore and aft: the break's
/// bulkhead, and the transom.
pub(super) struct Board {
    /// Where it stands along her, and its foot and head.
    pub(super) z: f32,
    pub(super) y0: f32,
    pub(super) y1: f32,
    /// How far into the shell's wall its edges are bedded at each height -
    /// 0 the bore's inner face, 1 the outer skin - and a growth on that.
    pub(super) wall: f32,
    pub(super) grow: f32,
    /// Its thickness, as a fraction of the length.
    pub(super) thick: f32,
    /// A station over its head, `(y, half-width)`: the transom's taffrail.
    pub(super) cap: Option<(f32, f32)>,
}

/// A [`Board`] as a [`flat_sweep`]: four stations from its foot up to its
/// head, each as wide as the section there, and its cap - flattened across
/// `z` to its thickness at its widest.
pub(super) fn board(hull: &HullProfile, m: &SovereignMaterialSettings, b: &Board) -> Generator {
    let mut pts: Vec<([f32; 3], f32)> = (0..4)
        .map(|k| {
            let y = b.y0 + (b.y1 - b.y0) * k as f32 / 3.0;
            let d = hull.sheer_z(b.z) - y;
            let hw = inner_half_width(hull, b.z, d, HOLLOW) * (1.0 - b.wall)
                + inner_half_width(hull, b.z, d, 1.0) * b.wall;
            ([0.0, y, b.z], hw * b.grow)
        })
        .collect();
    pts.extend(b.cap.map(|(y, hw)| ([0.0, y, b.z], hw)));
    let widest = pts.iter().map(|&(_, r)| r).fold(0.0f32, f32::max);
    flat_sweep(
        &pts,
        16,
        2,
        hull.loa * b.thick * 0.5 / widest,
        m,
        [0.0, 0.0, b.z],
    )
}
