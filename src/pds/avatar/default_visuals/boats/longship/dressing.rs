//! What she carries besides her hull and her rig: the steering oar that IS
//! her draft, the crew's tent aft, the serpent prow, and the galley's ram
//! and bank of oars.
//!
//! ORNATENESS: Adorned carries the tent over her after quarter, and on a
//! NORSE_FEY theme the serpent at her stem; Ornate adds the masthead banner
//! (see [`rig`](super::rig)). WEAR is on her sail and her shield row, where
//! the camera looks: a replaced panel when she is worn, and a dead shield in
//! the row when she is battered.
//!
//! The STEERING OAR is on the starboard quarter, and her blade's foot is
//! MINUS the derived draft by construction - the profile's allowance IS its
//! depth under the canoe body, as the junk's rudder is hers - so her hover,
//! a quarter of a draft, clears her lowest part.

use crate::pds::generator::Generator;

use super::super::super::common::quat_x;
use super::super::profile::HullProfile;
use super::super::shape::{flat_sweep, line, sweep, turned};
use super::super::{LongshipColours, MIN_DIM};
use super::hull::{HOLLOW, depth_at, inner_half_width, lay_on, row_stations, run_z};

/// The steering oar's station and the chord of her blade, as fractions of
/// the length.
const STEER_ZF: f32 = -0.400;
const STEER_CHORD: f32 = 0.105;

/// The Adorned tent's after and fore ends, as fractions of the length: over
/// her after quarter, beside the sail.
const TENT: (f32, f32) = (-0.420, -0.130);

/// How much taller than half its width the tent's ridge stands.
const TENT_PITCH: f32 = 1.05;

/// The serpent's reach over the stem, as a fraction of the length.
const PROW_L: f32 = 0.170;

/// The galley's ram: how far she reaches past her forefoot, as a fraction of
/// the length.
const RAM_L: f32 = 0.135;

/// The galley's bank of oars, centre to centre as a fraction of the length -
/// half the shield row's pitch, which is what makes a BANK rather than a
/// scattering.
const BANK_PITCH: f32 = 0.080;

/// Her lowest point (m, under her waterline): the steering oar's blade foot,
/// which IS minus her derived draft. See the module docs.
pub(super) fn blade_foot(hull: &HullProfile) -> f32 {
    -hull.draft
}

/// The steering oar on the STARBOARD quarter: a stock over the gunwale, a
/// tiller inboard, and a broad blade abaft the stock whose foot is her draft.
///
/// The blade is a flattened board on four stations down the stock's own
/// line, each a half-chord wide, and the deepest is lifted by its OWN radius
/// so the board's bottom is the draft rather than a half-chord under it.
pub(super) fn steering_oar(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
    let l = hull.loa;
    let z = STEER_ZF * l;
    let d = depth_at(hull, z) * 0.22;
    let hw = inner_half_width(hull, z, d, 1.0);
    let foot = blade_foot(hull);
    let ch = l * STEER_CHORD;
    let head = [hw + l * 0.018, hull.sheer_z(z) + l * 0.022, z];
    let heel = [hw + l * 0.050, foot + ch * 0.20, z - l * 0.026];
    kids.push(line(&[(head, l * 0.0085), (heel, l * 0.0068)], 6, &c.oar));
    kids.push(line(
        &[
            (head, l * 0.0060),
            ([hw * 0.10, head[1] + l * 0.004, z - l * 0.078], l * 0.0052),
        ],
        4,
        &c.oar,
    ));
    let pts: Vec<([f32; 3], f32)> = (0..4)
        .map(|k| {
            let f = 0.34 + 0.66 * k as f32 / 3.0;
            let p: [f32; 3] = std::array::from_fn(|i| head[i] + (heel[i] - head[i]) * f);
            let w = (ch * (0.42 + 0.58 * (std::f32::consts::PI * (f * 1.10).min(1.0)).sin()) * 0.5)
                .max(l * 0.006);
            ([p[0], p[1].max(foot + w), p[2] - ch * 0.40], w)
        })
        .collect();
    let widest = pts.iter().map(|&(_, r)| r).fold(0.0f32, f32::max);
    kids.push(flat_sweep(
        &pts,
        12,
        0,
        l * 0.0068 / widest,
        &c.blade,
        [head[0] + l * 0.016, 0.0, 0.0],
    ));
}

/// ADORNED: the crew's TENT over her after quarter - what a longship carried
/// for her crew ashore, and the one mass that reads over an OPEN boat at
/// 12 m from the chase camera, which looks straight at her quarter while her
/// sail covers everything amidships.
///
/// ONE sweep at resolution 4 under its upper half-cut: four facets round the
/// section means the upper half is two sloping planes meeting at a ridge, so
/// a ridge tent is a half-pipe and costs one node.
///
/// TWO THINGS THE FIRST DRAW GOT WRONG, and both are general. Its stations
/// read the hull's own inner half-width, which TAPERS TO NOTHING at a
/// double-ender's fine stern, so the tent came out a wedge lying in the
/// boat; and its path followed the SHEER, which climbs 0.24 m over its run,
/// and a crowned or pitched sweep cannot climb (#1371 finding B). It is now
/// ONE half-width for the whole ridge - taken where the hull is narrowest
/// over the run - on a LEVEL path a hair over the highest gunwale on it.
pub(super) fn awning(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
    let l = hull.loa;
    let zs = run_z(hull, TENT.0 * l, TENT.1 * l);
    let hw = zs
        .iter()
        .map(|&z| inner_half_width(hull, z, depth_at(hull, z) * 0.10, HOLLOW))
        .fold(f32::INFINITY, f32::min)
        * 1.06;
    let hw = hw.max(l * 0.055);
    let y = zs
        .iter()
        .map(|&z| hull.sheer_z(z))
        .fold(f32::NEG_INFINITY, f32::max)
        - l * 0.012;
    let pts: Vec<_> = zs.iter().map(|&z| ([0.0, y, z], hw)).collect();
    kids.push(sweep(
        &pts,
        4,
        [1.0, TENT_PITCH, 1.0],
        [0.0, 0.5],
        &c.tent,
        0.0,
    ));
    // The ridge pole, standing a hair proud of the cloth along its crown.
    let ridge = y + hw * TENT_PITCH * 1.03;
    kids.push(line(
        &[
            ([0.0, ridge, zs[0]], l * 0.0060),
            ([0.0, ridge, zs[zs.len() - 1]], l * 0.0060),
        ],
        6,
        &c.spar,
    ));
}

/// ADORNED, on a NORSE_FEY theme: the SERPENT PROW, re-authored at hull
/// scale on the stem.
///
/// The legacy part (`git show e9462b9:.../kits.rs`, `fn bow_serpent`) was a
/// Catmull-Rom Spine neck of five stations, a CONE lower jaw, two CONE horns
/// and two SPHERE eyes, at about 0.4 m for the legacy boat scale. Cones and
/// spheres are not the swept-and-turned vocabulary rule 2 asks for, so the
/// neck stays a Spine and the head is TURNED - and every one of her turned
/// profiles starts and ends ON THE AXIS, which is how a snout closes rather
/// than being capped with a full disc (the disc-cap trap, #1364).
pub(super) fn serpent(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
    let l = hull.loa;
    let u = l * PROW_L;
    let base = [0.0, hull.sheer_z(hull.stem_z()), hull.stem_z()];
    // The neck: five stations off the stem, rising and reaching forward,
    // then curling back over itself the way a stem-head beast does.
    let neck = [
        ([0.0, -u * 0.10, -u * 0.06], u * 0.190),
        ([0.0, u * 0.30, u * 0.10], u * 0.165),
        ([0.0, u * 0.62, u * 0.30], u * 0.150),
        ([0.0, u * 0.83, u * 0.66], u * 0.175),
        ([0.0, u * 0.78, u * 1.02], u * 0.072),
    ];
    let path: Vec<_> = neck
        .iter()
        .map(|&(p, r)| (std::array::from_fn(|k| base[k] + p[k]), r))
        .collect();
    kids.push(line(&path, 8, &c.scale));
    let head = [base[0], base[1] + u * 0.80, base[2] + u * 0.93];
    // The snout: a turned profile that starts and ends on the axis.
    kids.push(turned(
        &[
            (0.0, 0.0),
            (u * 0.155, u * 0.12),
            (u * 0.125, u * 0.36),
            (u * 0.065, u * 0.52),
            (0.0, u * 0.60),
        ],
        10,
        true,
        &c.scale,
        head,
        quat_x(1.42),
        0.0,
    ));
    // The lower jaw, a second turned snout under it and a shade shorter.
    kids.push(turned(
        &[
            (0.0, 0.0),
            (u * 0.100, u * 0.08),
            (u * 0.072, u * 0.28),
            (0.0, u * 0.42),
        ],
        8,
        true,
        &c.scale,
        [head[0], head[1] - u * 0.115, head[2] - u * 0.02],
        quat_x(1.62),
        0.0,
    ));
    for s in [-1.0f32, 1.0] {
        // The horns, swept back off the crown - turned, and closed on the
        // axis.
        kids.push(turned(
            &[
                (0.0, 0.0),
                (u * 0.060, u * 0.05),
                (u * 0.028, u * 0.26),
                (0.0, u * 0.36),
            ],
            8,
            true,
            &c.horn,
            [
                head[0] + s * u * 0.090,
                head[1] + u * 0.115,
                head[2] - u * 0.17,
            ],
            quat_x(-0.62),
            0.0,
        ));
        // The eyes: small turned domes, in the palette's own accent.
        kids.push(turned(
            &[(0.0, 0.0), (u * 0.052, u * 0.014), (0.0, u * 0.040)],
            8,
            true,
            &c.eye,
            [
                head[0] + s * u * 0.098,
                head[1] + u * 0.045,
                head[2] + u * 0.040,
            ],
            lay_on([s, 0.15, 0.30]),
            0.0,
        ));
    }
}

/// THE GALLEY'S RAM: a proper BEAK swept off her forefoot at the waterline -
/// the brief's own words, and the warning with them, "not a gun barrel".
///
/// So it is a FLATTENED sweep, broad across and thin in section: a bronze
/// blade growing out of her forefoot, rooted where her keel is still under
/// water and reading the hull's own half-beam there. A round tube of the
/// same length IS the gun barrel, and at 12 m the two are nearly the same
/// silhouette - which is why the bank of oars, not the ram, is what makes
/// her a galley.
pub(super) fn ram(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
    let l = hull.loa;
    let u = l * RAM_L;
    let (_aft, fwd) = hull.waterline();
    let zr = fwd - l * 0.055;
    let y0 = hull.keel_at(zr) + l * 0.008;
    let root_w = (hull.half_beam_at(zr) * 0.55).max(l * 0.014);
    let pts = [
        ([0.0, y0, zr], root_w),
        ([0.0, y0 - u * 0.04, zr + u * 0.42], u * 0.30),
        ([0.0, y0 - u * 0.07, zr + u * 0.86], u * 0.22),
        ([0.0, y0 - u * 0.08, zr + u * 1.06], u * 0.105),
    ];
    let widest = pts.iter().map(|&(_, r)| r).fold(0.0f32, f32::max);
    let thick = (l * 0.011).max(MIN_DIM * 1.1);
    kids.push(flat_sweep(
        &pts,
        12,
        1,
        thick / (2.0 * widest),
        &c.iron,
        [0.0, y0, 0.0],
    ));
}

/// THE GALLEY'S BANK: oars shipped along the gunwale at half the shield
/// row's pitch, their looms inboard and their blades standing out over the
/// water - each one thin Spine through its own port.
///
/// The bank is what makes a galley a galley, and it is why the longship
/// herself carries none: an oar PORT painted on her topsides does not read
/// at 12 m, and a longship under sail has her oars inboard and out of sight.
pub(super) fn oars(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours) {
    let l = hull.loa;
    for z in row_stations(hull, BANK_PITCH) {
        let d = depth_at(hull, z) * 0.42;
        let hw = inner_half_width(hull, z, d, 1.0);
        for side in [-1.0f32, 1.0] {
            let at = [side * hw, hull.sheer_z(z) - d, z];
            let inb = [side * hw * 0.32, hull.sheer_z(z) + l * 0.012, z + l * 0.055];
            let out = [
                side * (hw + l * 0.115),
                hull.sheer_z(z) - l * 0.030,
                z - l * 0.085,
            ];
            kids.push(line(
                &[(inb, l * 0.0075), (at, l * 0.0070), (out, l * 0.0060)],
                6,
                &c.oar,
            ));
        }
    }
}
