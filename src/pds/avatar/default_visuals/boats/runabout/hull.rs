//! The planing hull every runabout variant is built on, and the fittings they
//! share - all of it read off the [`HullProfile`] and the ONE polygon her
//! section is swept as.
//!
//! # The section is a polygon, on purpose
//!
//! A Spine's `resolution` counts segments over the KEPT arc
//! (`world_builder/prim/sweeps.rs`), so the lower half-pipe swept at
//! [`HULL_RES`] = 4 has exactly five vertices round its section: the deck
//! edge, the CHINE at 45 degrees round, the keel, the other chine, the other
//! deck edge. That is a hard-chine V bottom under flared topsides, which is a
//! planing hull, and the `path_cut` cap at her after end is a flat
//! V-bottomed transom. Every line below sits on a vertex or a face of that
//! polygon, which is why [`CHINE`] is `cos 45` rather than a tuned fraction.

use crate::pds::generator::Generator;

use super::super::super::common::quat_x;
use super::super::RunaboutColours;
use super::super::profile::HullProfile;
use super::super::shape::hull_path;
pub(super) use super::super::shape::{
    DECK_CROWN, UPRIGHT, box_at, deck_run, line, panel, run_z, sweep, turned, underbody,
};

/// Segments over the hull's kept half-section - see the module docs.
pub(super) const HULL_RES: u32 = 4;

/// The chine vertex as a fraction of the half-beam across and of the
/// section's depth down: the polygon's second vertex, 45 degrees round.
pub(super) const CHINE: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// The shell is BORED, so the cockpit is a real well you look down into -
/// the roadster tub's idiom (#1364). A solid sweep's cut face is a flat lid at
/// the sheer, and any well under it is invisible. The wall is a tenth of the
/// half-beam.
pub(super) const HULL_HOLLOW: f32 = 0.90;

// ---------------------------------------------------------------------------
// Reads off the polygon
// ---------------------------------------------------------------------------

/// The chine vertex at `z` on `side`, on a hull centred at `x0`.
pub(super) fn chine(hull: &HullProfile, z: f32, side: f32, x0: f32) -> [f32; 3] {
    let hb = hull.half_beam_at(z);
    [
        x0 + side * hb * CHINE,
        hull.sheer_z(z) - hb * hull.section * CHINE,
        z,
    ]
}

/// A point on the flat topsides face at `z`, fraction `f` of the way from
/// the deck edge (0) down to the chine (1).
pub(super) fn topsides_at(hull: &HullProfile, z: f32, side: f32, f: f32, x0: f32) -> [f32; 3] {
    let hb = hull.half_beam_at(z);
    [
        x0 + side * hb * (1.0 - f * (1.0 - CHINE)),
        hull.sheer_z(z) - f * hb * hull.section * CHINE,
        z,
    ]
}

/// Half-width of the bored shell's INNER face at `depth` under the sheer -
/// the same polygon at `hollow` of the radius.
pub(super) fn inner_half_width(hull: &HullProfile, z: f32, depth: f32, hollow: f32) -> f32 {
    let hb = hull.half_beam_at(z) * hollow;
    let s = hull.section;
    let chine_d = hb * s * CHINE;
    if depth <= chine_d {
        return hb * (1.0 - (1.0 - CHINE) * depth / chine_d);
    }
    hb * CHINE * (1.0 - (depth - chine_d) / (hb * s - chine_d)).max(0.0)
}

// ---------------------------------------------------------------------------
// The hull and what every variant carries
// ---------------------------------------------------------------------------

/// The shell, the bottom paint under the chines, the transom plug, and the
/// three lines along her: the spray rail on the chine, the cove line on the
/// topsides face and the rub rail on the deck edge.
pub(super) fn skin(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    x0: f32,
    hollow: f32,
) {
    let l = hull.loa;
    let sc = [1.0, hull.section, 1.0];
    kids.push(sweep(
        &hull_path(hull, 1.0, x0, None),
        HULL_RES,
        sc,
        [0.5, 1.0],
        &c.topsides,
        hollow,
    ));
    // Bottom paint, chine to chine: a hair proud, bored like the shell so
    // its cut bands lie in the wall, and stopped short of the transom so its
    // cap is buried in the transom plug (the sloop's defect 2, #1366).
    let aft = hull.transom_z() + l * 0.006;
    kids.push(sweep(
        &hull_path(hull, 1.008, x0, Some(aft)),
        HULL_RES,
        sc,
        [0.625, 0.875],
        &c.antifoul,
        hollow,
    ));
    // The transom plug: a short SOLID sweep of the after stations a hair
    // inside the shell, standing 3 mm proud astern - its after cap IS the
    // transom face, so the bored shell's annulus never shows.
    let t = hull.transom_z();
    let fwd = t + l * 0.03;
    let plug = [
        (
            [x0, hull.sheer_z(t), t - 0.003],
            hull.half_beam_at(t) * 0.998,
        ),
        ([x0, hull.sheer_z(fwd), fwd], hull.half_beam_at(fwd) * 0.998),
    ];
    kids.push(sweep(&plug, HULL_RES, sc, [0.5, 1.0], &c.topsides, 0.0));
    let st = hull.stations();
    for side in [-1.0f32, 1.0] {
        let pts: Vec<_> = st
            .iter()
            .map(|s| (chine(hull, s.z, side, x0), l * 0.0060))
            .collect();
        kids.push(line(&pts, 8, &c.boot));
    }
    for side in [-1.0f32, 1.0] {
        let pts: Vec<_> = st
            .iter()
            .map(|s| (topsides_at(hull, s.z, side, 0.30, x0), l * 0.0045))
            .collect();
        kids.push(line(&pts, 8, &c.boot));
    }
    for side in [-1.0f32, 1.0] {
        let pts: Vec<_> = st
            .iter()
            .map(|s| ([x0 + side * s.half_beam * 0.995, s.sheer, s.z], l * 0.0068))
            .collect();
        kids.push(line(&pts, 8, &c.rail));
    }
}

/// A wraparound windscreen FRAME, no glass (#1359 rule 4): one spine from
/// the port deck edge up, across a top rail that curves FORWARD to the
/// centreline, and down to the starboard deck edge; with `post`, a centre bar
/// down to the deck's crown.
#[allow(clippy::too_many_arguments)]
pub(super) fn windscreen(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    z: f32,
    height: f32,
    rake: f32,
    reach: f32,
    post: bool,
) {
    let l = hull.loa;
    let hb = hull.half_beam_at(z) * reach;
    let dy = hull.sheer_z(z);
    let r = l * 0.0052;
    let crest = dy + height * 1.03 + l * 0.004;
    let pts = [
        ([-hb, dy - l * 0.004, z - rake * 0.2], r),
        ([-hb * 0.97, dy + height, z - rake], r),
        ([-hb * 0.55, dy + height * 1.02, z - rake * 0.35], r),
        ([0.0, crest, z + rake * 0.15], r),
        ([hb * 0.55, dy + height * 1.02, z - rake * 0.35], r),
        ([hb * 0.97, dy + height, z - rake], r),
        ([hb, dy - l * 0.004, z - rake * 0.2], r),
    ];
    kids.push(line(&pts, 8, &c.chrome));
    if post {
        let crown = hull.sheer_z(z + rake * 0.15) + hull.half_beam_at(z) * DECK_CROWN * 0.985;
        kids.push(line(
            &[
                ([0.0, crown - l * 0.004, z + rake * 0.15], r),
                ([0.0, crest, z + rake * 0.15], r),
            ],
            6,
            &c.chrome,
        ));
    }
}

/// A bench across the cockpit: a cushion on the sole and a back leaning aft
/// by `lean` radians, bedded into the cushion's after edge. The cockpit's one
/// pale mass at 12 m.
#[allow(clippy::too_many_arguments)]
pub(super) fn bench(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    z: f32,
    depth: f32,
    sole_y: f32,
    x_half: f32,
    seat_h: f32,
    back_h: f32,
    lean: f32,
) {
    let l = hull.loa;
    let cushion = l * 0.028;
    kids.push(panel(
        [x_half * 2.0, seat_h, depth],
        &c.upholstery,
        [0.0, sole_y + seat_h * 0.5, z],
        UPRIGHT,
        l * 0.010,
    ));
    let thick = l * 0.024;
    let by = sole_y + seat_h + back_h * 0.5 * lean.cos() - cushion * 0.3;
    let bz = z - depth * 0.5 + thick * 0.5 - back_h * 0.5 * lean.sin();
    kids.push(panel(
        [x_half * 2.0, back_h, thick],
        &c.upholstery,
        [0.0, by, bz],
        quat_x(-lean),
        l * 0.008,
    ));
}

/// A wheel rim - a Lathe band bored open - raked aft, on a column running
/// forward and down into the dash.
pub(super) fn steering_wheel(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    at: [f32; 3],
    r: f32,
) {
    let l = hull.loa;
    let [x, y, z] = at;
    kids.push(turned(
        &[(r, -l * 0.004), (r, l * 0.004)],
        16,
        false,
        &c.wheel,
        at,
        quat_x(std::f32::consts::FRAC_PI_2 - 0.55),
        0.80,
    ));
    kids.push(line(
        &[
            ([x, y - r * 0.9, z + r * 0.9], l * 0.006),
            ([x, y, z], l * 0.0045),
        ],
        6,
        &c.chrome,
    ));
}

/// How deep under the sheer the cockpit sole lies: a fraction of the
/// section's depth amidships.
pub(super) fn sole_depth(hull: &HullProfile) -> f32 {
    hull.half_beam_at(0.0) * hull.section * 0.55
}

/// The sole inside the bored shell, from `za` to `zf`, let into the wall at
/// both sides. Returns the sole's top.
pub(super) fn cockpit_sole(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    (za, zf): (f32, f32),
) -> f32 {
    let l = hull.loa;
    let depth = sole_depth(hull);
    let wall = hull.half_beam * (1.0 - HULL_HOLLOW);
    let zm = (za + zf) * 0.5;
    let sole_y = hull.sheer_z(zm) - depth;
    let w = inner_half_width(hull, za, depth, HULL_HOLLOW).min(inner_half_width(
        hull,
        zf,
        depth,
        HULL_HOLLOW,
    ));
    kids.push(panel(
        [2.0 * (w + wall * 0.5), l * 0.012, zf - za],
        &c.sole,
        [0.0, sole_y - l * 0.006, zm],
        UPRIGHT,
        0.0,
    ));
    sole_y
}

/// The half-width a bench spans across the cockpit at the sole: the shell's
/// inner face a little above it, let into the wall.
pub(super) fn bench_half_width(hull: &HullProfile, (za, zf): (f32, f32), sole_y: f32) -> f32 {
    inner_half_width(
        hull,
        (za + zf) * 0.5,
        hull.sheer_z(0.0) - sole_y - hull.loa * 0.05,
        HULL_HOLLOW,
    ) + hull.half_beam * 0.03
}

/// A chrome stem band down the cutwater, from the forefoot to the stemhead:
/// the bow's one bright line.
pub(super) fn stem_band(kids: &mut Vec<Generator>, hull: &HullProfile, c: &RunaboutColours) {
    let l = hull.loa;
    let mut pts: Vec<_> = hull
        .stations()
        .iter()
        .filter(|s| s.zf >= 0.34)
        .map(|s| ([0.0, s.keel + l * 0.002, s.z], l * 0.0055))
        .collect();
    pts.push((
        [
            0.0,
            hull.sheer_z(hull.stem_z()) + l * 0.004,
            hull.stem_z() + l * 0.002,
        ],
        l * 0.0055,
    ));
    kids.push(line(&pts, 6, &c.chrome));
}
