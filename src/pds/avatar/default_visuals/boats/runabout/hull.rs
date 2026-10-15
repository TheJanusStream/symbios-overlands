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
use crate::pds::types::Fp2;

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
///
/// Each part spans the cockpit as wide as the shell lets it AT ITS OWN
/// stations and heights ([`span_half_width`]), and the cushion's sides
/// follow the topsides' flare - its foot narrowed to the floor's width, its
/// top the full width a hand higher (#1655). One width for both, read a hand
/// above the floor at the cockpit's middle, stood the cushion's lower ends
/// out through the skin, worst toward the bow where the hull narrows.
#[allow(clippy::too_many_arguments)]
pub(super) fn bench(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    z: f32,
    depth: f32,
    sole_y: f32,
    seat_h: f32,
    back_h: f32,
    lean: f32,
) {
    let l = hull.loa;
    let cushion = l * 0.028;
    let span = (z - depth * 0.5, z + depth * 0.5);
    let top = span_half_width(hull, span, sole_y + seat_h);
    let foot = span_half_width(hull, span, sole_y);
    let mut seat = panel(
        [top * 2.0, seat_h, depth],
        &c.upholstery,
        [0.0, sole_y + seat_h * 0.5, z],
        UPRIGHT,
        l * 0.010,
    );
    if let Some(t) = seat.kind.torture_mut() {
        t.taper_bottom = Fp2([1.0 - foot / top, 0.0]);
    }
    kids.push(seat);
    let thick = l * 0.024;
    let by = sole_y + seat_h + back_h * 0.5 * lean.cos() - cushion * 0.3;
    let bz = z - depth * 0.5 + thick * 0.5 - back_h * 0.5 * lean.sin();
    // The back leans aft, so its foot is its lowest and its forward-most
    // edge and its head its aftmost: it spans the stations between.
    let reach = 0.5 * (back_h * lean.sin() + thick * lean.cos());
    let back_foot = by - 0.5 * back_h * lean.cos() - 0.5 * thick * lean.sin();
    kids.push(panel(
        [
            span_half_width(hull, (bz - reach, bz + reach), back_foot) * 2.0,
            back_h,
            thick,
        ],
        &c.upholstery,
        [0.0, by, bz],
        quat_x(-lean),
        l * 0.008,
    ));
}

/// The half-width a part spanning the cockpit may have at height `y` between
/// stations `za` and `zf` (#1655): the bored shell's inner face there, at the
/// narrower station, let half way into the wall - so the part meets the wall
/// and never stands out through the skin. The topsides flare, so the lower a
/// part reaches the narrower it must be.
fn span_half_width(hull: &HullProfile, (za, zf): (f32, f32), y: f32) -> f32 {
    let at = |z: f32| {
        let depth = hull.sheer_z(z) - y;
        let inner = inner_half_width(hull, z, depth, HULL_HOLLOW);
        let outer = inner_half_width(hull, z, depth, 1.0);
        (inner + outer) * 0.5
    };
    at(za).min(at(zf))
}

/// A wheel rim - a Lathe band bored open - on a column running forward and
/// down into the dash, SQUARE TO THE COLUMN (#1655): the rim's axis is the
/// column's own direction, read off its two points, so the wheel faces the
/// helm up the column and its top leans forward, as a real one's does. It
/// was turned by the roadster's old hand-set angle, which tipped it 76
/// degrees off square to this column, its top leaning back at the helm
/// (#1651 fixed the roadster's).
pub(super) fn steering_wheel(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    at: [f32; 3],
    r: f32,
) {
    let l = hull.loa;
    let [x, y, z] = at;
    let foot = [x, y - r * 0.9, z + r * 0.9];
    // One turn about x carries the rim's axis - the lathe's +Y - up the
    // column, which stands in one x-plane.
    let up_the_column = (z - foot[2]).atan2(y - foot[1]);
    kids.push(turned(
        &[(r, -l * 0.004), (r, l * 0.004)],
        16,
        false,
        &c.wheel,
        at,
        quat_x(up_the_column),
        0.80,
    ));
    kids.push(line(
        &[(foot, l * 0.006), ([x, y, z], l * 0.0045)],
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

#[cfg(test)]
mod tests {
    use bevy::math::{Quat, Vec3};

    use super::super::profile_of;
    use super::*;
    use crate::pds::avatar::livery::runabout_colours;
    use crate::pds::avatar::parts::PartCtx;
    use crate::pds::generator::GeneratorKind;
    use crate::seeded_defaults::{BoatBlueprint, RunaboutVariant, VehicleStance};

    /// The wheel is square to its column (#1655): the rim's axis - the
    /// lathe's +Y through every turn on it - runs along the column's own two
    /// points, toward the helm, and its top leans forward toward the bow.
    /// It was turned 76 degrees off square to this column, its top leaning
    /// back at the helm - the roadster's mistake (#1651).
    #[test]
    fn the_wheel_is_square_to_its_column_and_leans_its_top_forward() {
        let loa = 2.8;
        let bp = BoatBlueprint {
            stance: VehicleStance::Sleek,
            hull_len: loa,
            beam: loa / 3.5,
            freeboard: loa * 0.11,
            sheer_bow: loa * 0.060,
            sheer_stern: loa * 0.025,
            draft: loa * 0.100,
        };
        for v in RunaboutVariant::ALL {
            let hull = profile_of(&bp, v);
            let c = runabout_colours(&PartCtx::for_seed(0), v);
            let mut kids = Vec::new();
            let at = [-hull.half_beam * 0.38, hull.sheer_z(0.0) + loa * 0.018, 0.0];
            steering_wheel(&mut kids, &hull, &c, at, loa * 0.028);
            let [rim, column] = kids.as_slice() else {
                panic!("{v:?}: a wheel is a rim and a column");
            };
            let GeneratorKind::Spine { points, .. } = &column.kind else {
                panic!("{v:?}: the column is a swept line");
            };
            let along = (Vec3::from(points[points.len() - 1].position.0)
                - Vec3::from(points[0].position.0))
            .normalize();
            let axis = (Quat::from_array(rim.transform.rotation.0) * Vec3::Y).normalize();
            assert!(
                axis.dot(along) > 0.1f32.to_radians().cos(),
                "{v:?}: the rim's axis {axis} is {:.1} degrees off its column's {along}",
                axis.angle_between(along).to_degrees()
            );
            let top = (Vec3::Y - axis * axis.dot(Vec3::Y)).normalize();
            assert!(
                top.z > 0.0,
                "{v:?}: the wheel's top leans back at the helm ({top})"
            );
        }
    }
}
