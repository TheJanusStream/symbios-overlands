//! The longship's open shell, her bottom boards, her clinker strakes and the
//! SHIELD ROW that is her identity - all read off the one [`HullProfile`].
//!
//! # The section is a shallow round bilge, and its rim is her gunwale
//!
//! A Spine's `resolution` counts segments over the KEPT arc, so the lower
//! half-pipe swept at [`HULL_RES`] = 8 has nine vertices round its section:
//! a round bilge, which is what a wide, shallow, keel-less hull has. An EVEN
//! resolution puts a vertex at 90 degrees, so the polygon reaches its whole
//! swept radius down ([`BOTTOM`] is 1) and the node's y-scale IS the
//! profile's `section` - the junk and the scow both divide by theirs because
//! theirs is odd (#1371, #1373).
//!
//! The shell is BORED ([`HOLLOW`]), and that is the whole point of her: a
//! solid sweep's cut face is a flat lid at the sheer, and a longship is an
//! OPEN boat. Bored, the wall's cut rim IS her gunwale - the line the
//! shields hang on, the strakes run under and the rail caps - and you look
//! down into a boat instead of onto a deck. Her two ends are closed by
//! solid plugs so the bore is not open to the sky through the stem and the
//! stern post, and her bottom boards ([`floor_boards`]) are what you look
//! down at.

use std::f32::consts::PI;

use crate::pds::generator::Generator;
use crate::seeded_defaults::WearTier;

use super::super::super::common::quat_z;
use super::super::LongshipColours;
use super::super::profile::HullProfile;
use super::super::shape::{self, hull_path, line, sweep, turned};

/// Segments over the hull's kept half-section: a shallow round bilge.
pub(super) const HULL_RES: u32 = 8;

/// How far down the res-8 polygon reaches per unit of its swept radius. An
/// EVEN resolution has a vertex at 90 degrees, so it reaches the whole
/// radius and the hull node's y-scale is the profile's `section` unchanged -
/// see the module docs.
const BOTTOM: f32 = 1.0;

/// The shell is BORED to this fraction of its radius, and its wall's cut rim
/// is her gunwale.
pub(super) const HOLLOW: f32 = 0.88;

/// The middle of the shell's wall, as a fraction of the radius: where the
/// capping rail runs.
const WALL: f32 = (1.0 + HOLLOW) * 0.5;

/// The bottom boards, as a fraction of the shell's own bored depth amidships.
pub(super) const FLOOR: f32 = 0.72;

/// The run the shield row and the galley's oars lie along, as fractions of
/// the length: her rowing stations.
pub(super) const OAR_ZF: (f32, f32) = (-0.330, 0.330);

/// Six clinker strakes a side, each a thin Spine at `(k + 0.5) / N` of the
/// section's depth, and its radius as a fraction of the length.
const STRAKES: usize = 6;
const STRAKE_R: f32 = 0.0042;

/// How many points a strake is drawn on. A strake is a LINE ALONG the form,
/// not a station list: five points carry the same curve as eleven for less
/// than half the bytes, because a Spine's path is Catmull-Rom and the shape
/// between them is the hull's own.
const STRAKE_PTS: usize = 5;

/// A shield's radius and the row's pitch, centre to centre, as fractions of
/// the length - and which shield in the row goes bare timber on a BATTERED
/// kit.
const SHIELD_R: f32 = 0.048;
const SHIELD_PITCH: f32 = 0.125;
const ODD_SHIELD: usize = 2;

/// The hull node's y-scale: her section depth over the polygon's reach,
/// which on an even resolution is the section itself.
fn node_section(hull: &HullProfile) -> f32 {
    hull.section / BOTTOM
}

/// The canoe body's depth under the sheer at `z`.
pub(super) fn depth_at(hull: &HullProfile, z: f32) -> f32 {
    hull.half_beam_at(z) * hull.section
}

/// Half-width of the round section `depth` under the sheer at `z`, on the
/// polygon grown by `grow` - [`HOLLOW`] for the bore's inner face, 1.0 for
/// the outer skin.
///
/// The tug's walk (#1370): the polygon's vertices down to its DEEPEST, and
/// no further. Walking one vertex past the keel, as the scow does for its
/// odd resolution, answers with a NEGATIVE half-width wherever a part is
/// asked for deeper than the section goes.
pub(super) fn inner_half_width(hull: &HullProfile, z: f32, depth: f32, grow: f32) -> f32 {
    let hb = hull.half_beam_at(z) * grow;
    let pts: Vec<(f32, f32)> = (0..=HULL_RES / 2)
        .map(|k| {
            let a = PI * k as f32 / HULL_RES as f32;
            (hb * a.cos(), hb * hull.section * a.sin())
        })
        .collect();
    for w in pts.windows(2) {
        let ((x0, d0), (x1, d1)) = (w[0], w[1]);
        if d0 <= depth && depth <= d1 && d1 > d0 {
            return x0 + (x1 - x0) * (depth - d0) / (d1 - d0);
        }
    }
    let last = pts[pts.len() - 1];
    if depth > last.1 { last.0 } else { hb }
}

/// The rotation that stands a turned part's local `+Y` along `normal` -
/// athwartships and down - as one turn about `z`. The junk's, and the
/// shields and the serpent's eyes are laid on with it.
pub(super) fn lay_on(normal: [f32; 3]) -> [f32; 4] {
    quat_z((-normal[0]).atan2(normal[1]))
}

/// Where the bottom paint starts round the section: `path_cut` 0.5 is one
/// deck edge and 0.75 the keel, so on a ROUND section the waterline's own
/// depth arrives through an arcsine - the junk's `boot_f` for a resolution
/// that is not 3.
fn boot_f(hull: &HullProfile) -> f32 {
    let zl = super::super::profile::SHEER_LOW * hull.loa;
    let frac = hull.freeboard / depth_at(hull, zl);
    0.5 + frac.min(1.0).asin() / PI
}

/// [`shape::run_z`], with the junk's duplicate-station trap as a debug
/// assertion rather than a silent fix-up (#1371 finding D).
///
/// A run that ENDS on a station keeps or drops that station by the last bit
/// of two roundings, and a kept one is a second point on top of the first -
/// a zero-length segment the Catmull-Rom path loops back through. None of
/// the longship's three runs ends on one: the bottom boards stop at
/// +-0.40 L, the strakes at +-0.455 L and the tent over -0.42..-0.13 L,
/// against a symmetric plan whose stations are 0, +-0.120, +-0.255,
/// +-0.370, +-0.445 and +-0.500. This is what says so if a number moves.
pub(super) fn run_z(hull: &HullProfile, z0: f32, z1: f32) -> Vec<f32> {
    let zs = shape::run_z(hull, z0, z1);
    debug_assert!(
        zs.windows(2).all(|w| (w[1] - w[0]).abs() > hull.loa * 1e-5),
        "a run from {z0} to {z1} ends on a station and would draw it twice"
    );
    zs
}

/// The structural root. `apply_travel_pose` OVERWRITES the root's
/// translation, so whatever the root is sits at the waterline centre - which
/// on a bored open hull is the void inside her. So the root is a SPINE whose
/// points are its own: a keelson from the shell's inner bottom up to the
/// bottom boards amidships, honestly touching both.
pub(super) fn root(hull: &HullProfile, c: &LongshipColours) -> Generator {
    let l = hull.loa;
    let bottom = hull.sheer_z(0.0) - depth_at(hull, 0.0) * HOLLOW;
    let top = hull.sheer_z(0.0) - depth_at(hull, 0.0) * HOLLOW * FLOOR;
    line(
        &[
            ([0.0, bottom - l * 0.004, 0.0], l * 0.018),
            ([0.0, top, 0.0], l * 0.018),
        ],
        6,
        &c.interior,
    )
}

/// The bored shell - its wall's rim IS her gunwale - the bottom paint a hair
/// proud and bored the same, a solid plug at each end so she is not open to
/// the sky through her own stem and stern post, and the capping rail along
/// the wall at both sheers.
pub(super) fn skin(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
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
    // The bottom paint: the same stations a hair proud, cut from the
    // waterline round the bottom and stopped short of both plugs so its caps
    // are buried in them. Both ends are bracketed, which `hull_path` only
    // does forward, so the path is laid here.
    let (t, b) = (hull.transom_z(), hull.stem_z());
    let (aft, fwd) = (t + l * 0.004, b - l * 0.004);
    let mut bottom = vec![(
        [0.0, hull.sheer_z(aft), aft],
        hull.half_beam_at(aft) * 1.008,
    )];
    bottom.extend(
        hull.stations()
            .iter()
            .filter(|s| aft + 1e-6 < s.z && s.z < fwd - 1e-6)
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
    // The two end plugs: short SOLID sweeps a hair inside the shell, each
    // standing 3 mm proud of its end. A double-ender's are the same part
    // twice, which is what she is.
    for (z0, z1) in [(t - 0.003, t + l * 0.030), (b - l * 0.030, b + 0.003)] {
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
        kids.push(sweep(&plug, HULL_RES, sc, [0.5, 1.0], &c.hull, 0.0));
    }
    let st = hull.stations();
    for side in [-1.0f32, 1.0] {
        let pts: Vec<_> = st
            .iter()
            .map(|s| {
                (
                    [side * s.half_beam * WALL, s.sheer + l * 0.0025, s.z],
                    l * 0.0070,
                )
            })
            .collect();
        kids.push(line(&pts, 8, &c.rail));
    }
}

/// The bottom boards inside the bored shell: a shallow crowned sweep low in
/// her, so you look down into a boat rather than through to her antifoul.
///
/// LEVEL, because a crowned deck cannot climb (#1371 finding B): the crown's
/// y-scale makes a pre-divided path many times steeper than the boards, so a
/// crowned sole laid up a rising sheer comes out a dome bulging past its own
/// ends. Set at a fraction of the shell's bored depth amidships.
pub(super) fn floor_boards(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
    let l = hull.loa;
    let y = hull.sheer_z(0.0) - depth_at(hull, 0.0) * HOLLOW * FLOOR;
    let pts: Vec<([f32; 3], f32)> =
        run_z(hull, hull.transom_z() + l * 0.10, hull.stem_z() - l * 0.10)
            .into_iter()
            .filter_map(|z| {
                let hw = inner_half_width(hull, z, hull.sheer_z(z) - y, HOLLOW);
                (hw > l * 0.008).then_some(([0.0, y, z], hw * 0.97))
            })
            .collect();
    if pts.len() >= 2 {
        kids.push(sweep(
            &pts,
            14,
            [1.0, shape::DECK_CROWN, 1.0],
            [0.0, 0.5],
            &c.sole,
            0.0,
        ));
    }
}

/// The clinker read: six thin Spines a side, each following the hull's own
/// stations at a constant fraction of the section's depth under the sheer.
///
/// A strake is a LINE ALONG the form, unlike the buggy's tread lugs (rings
/// round one) and unlike a Lathe's turned grooves, whose radial normals show
/// nothing (#1378). It lands on the shell by construction: its centre is the
/// polygon's own surface point at that depth, a strake radius proud.
pub(super) fn strakes(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
    let l = hull.loa;
    let r = l * STRAKE_R;
    let ends = run_z(
        hull,
        hull.transom_z() + l * 0.045,
        hull.stem_z() - l * 0.045,
    );
    // Five points spread evenly over the run, deduplicated. The rounding is
    // HALF-TO-EVEN, which is python's `round` and so the twin's: over this
    // plan's eleven-point run the two half cases are 2.5 and 7.5, and
    // rounding them away from zero instead would move two of the five
    // control points a station along the hull.
    let mut idx: Vec<usize> = (0..STRAKE_PTS)
        .map(|i| {
            ((i * (ends.len() - 1)) as f32 / (STRAKE_PTS - 1) as f32).round_ties_even() as usize
        })
        .collect();
    idx.dedup();
    let zs: Vec<f32> = idx.into_iter().map(|i| ends[i]).collect();
    for k in 0..STRAKES {
        let f = (k as f32 + 0.5) / STRAKES as f32;
        for side in [-1.0f32, 1.0] {
            let pts: Vec<_> = zs
                .iter()
                .map(|&z| {
                    let d = depth_at(hull, z) * f;
                    let hw = inner_half_width(hull, z, d, 1.0);
                    ([side * (hw + r * 0.35), hull.sheer_z(z) - d, z], r)
                })
                .collect();
            kids.push(line(&pts, 6, &c.strake));
        }
    }
}

/// THE SHIELD ROW - her identity slot: turned discs hung along the gunwale on
/// both sides, in the livery's accent, six a side on a 0.125 L pitch.
///
/// No iron boss: a Norse shield is built round one, and at 12 m it is a 6 px
/// dome for twelve more nodes, which the phase-1 renders rejected. On a
/// BATTERED kit one shield on the NEAR (starboard) side is drawn in bare
/// timber instead - a dead shield in a row of painted ones, the cyclecar's
/// odd-rim rule, and the one thing in the row that reads at play distance.
pub(super) fn shield_row(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &LongshipColours,
    wear: WearTier,
) {
    let l = hull.loa;
    let r = l * SHIELD_R;
    let n = row_stations(hull, SHIELD_PITCH);
    for (k, &z) in n.iter().enumerate() {
        let d = depth_at(hull, z) * 0.26;
        let hw = inner_half_width(hull, z, d, 1.0);
        for side in [-1.0f32, 1.0] {
            let at = [side * hw, hull.sheer_z(z) - d, z];
            let odd = wear == WearTier::Battered && side > 0.0 && k == ODD_SHIELD % n.len();
            kids.push(turned(
                &[(0.0, 0.0), (r, l * 0.003), (0.0, l * 0.011)],
                8,
                false,
                if odd { &c.bare } else { &c.shield },
                at,
                lay_on([side, 0.0, 0.0]),
                0.0,
            ));
        }
    }
}

/// The stations of a row laid along [`OAR_ZF`] at `pitch` of the length,
/// centre to centre: the shield row's, and the galley's bank of oars at half
/// that pitch. At least two, so a short hull still carries a row.
pub(super) fn row_stations(hull: &HullProfile, pitch: f32) -> Vec<f32> {
    let l = hull.loa;
    let (z0, z1) = (OAR_ZF.0 * l, OAR_ZF.1 * l);
    let n = (((z1 - z0) / (l * pitch)) as usize + 1).max(2);
    let step = (z1 - z0) / (n - 1) as f32;
    (0..n).map(|k| z0 + step * k as f32).collect()
}
