//! The buggy's masses: what makes a variant, and the ladder of what dresses
//! a tier - masses that read at play distance, never trinkets (#1359 F6).
//!
//! On every tier the beach buggy wears her striped canopy, and the raider her
//! spare on the roll bar and two jerrycans on the near nerf bar (her diagonal
//! is part of the frame). Then, cumulatively:
//!
//! - **rail**: Adorned the light bar, four round lamps on the front hoop's
//!   top bar; Ornate the dune whip and its pennant as well;
//! - **beach**: Adorned the whip, Ornate the light bar as well;
//! - **raider**: Adorned the light bar, Ornate a rusted plate across the
//!   nose hoop as well.
//!
//! Wear is every variant's and is drawn where it belongs: the mismatched rim
//! by the wheels, the rusted exhaust and the primer shroud by the engine,
//! the tape by the seats.

use std::f32::consts::FRAC_PI_2;

use bevy::math::Vec3;
use bevy::math::cubic_splines::{CubicCardinalSpline, CubicGenerator};

use crate::pds::avatar::livery::BuggyColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BuggyVariant, OrnatenessTier};

use super::super::super::common::{bevel, id_quat, prim, quat_x};
use super::frame::Frame;
use super::running_gear::spare;
use super::{NEAR, RAIL, TUBE, along_z, board, line, solid, sweep};

/// One mass: it draws itself onto the kids.
type Mass = fn(&mut Vec<Generator>, &Frame, &BuggyColours);

/// Rule 6's air draft (m): nothing a seeded craft draws may stand higher
/// over the ground. The dune whip is the first skiff part near it.
const AIR_CAP: f32 = 2.80;

/// The canopy's stripes, each one node, and its crown over its half-width.
const STRIPES: usize = 6;
const CANOPY_RISE: f32 = 0.34;

/// The hoop's top bar is its fourth Catmull-Rom segment, between its two top
/// corners: see [`on_top_bar`].
const TOP_BAR: f32 = 3.0;

/// The variant's own masses, on every tier.
fn identity(v: BuggyVariant) -> &'static [Mass] {
    match v {
        BuggyVariant::Rail => &[],
        BuggyVariant::Beach => &[canopy],
        BuggyVariant::Raider => &[roll_bar_spare, jerrycans],
    }
}

/// What an ornateness tier adds, cumulatively.
fn ladder(v: BuggyVariant, o: OrnatenessTier) -> &'static [Mass] {
    match (v, o) {
        (_, OrnatenessTier::Plain) => &[],
        (BuggyVariant::Rail, OrnatenessTier::Adorned) => &[light_bar],
        (BuggyVariant::Rail, OrnatenessTier::Ornate) => &[light_bar, whip],
        (BuggyVariant::Beach, OrnatenessTier::Adorned) => &[whip],
        (BuggyVariant::Beach, OrnatenessTier::Ornate) => &[whip, light_bar],
        (BuggyVariant::Raider, OrnatenessTier::Adorned) => &[light_bar],
        (BuggyVariant::Raider, OrnatenessTier::Ornate) => &[light_bar, nose_plate],
    }
}

/// The variant's masses, then the tier's.
pub(super) fn dress(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours, o: OrnatenessTier) {
    let v = f.plan.variant;
    for mass in identity(v).iter().chain(ladder(v, o)) {
        mass(kids, f, c);
    }
}

/// A point ON a hoop's drawn top bar, `u` of the way across from its `-x`
/// top corner to its `+x` one - so a part hung between joints is hung on the
/// tube's own centreline rather than on the chord.
///
/// It has to be bevy's own Catmull-Rom, the curve the mesher draws the tube
/// with: bevy mirrors a spline's end points (the sloop's lesson, #1366), and
/// a segment depends only on its four neighbours, so the top bar - segment
/// three of eight points - never reaches the mirrored ends at all.
fn on_top_bar(hoop: &[[f32; 3]; 8], u: f32) -> [f32; 3] {
    let ctrl: Vec<Vec3> = hoop.iter().map(|&p| Vec3::from(p)).collect();
    let curve = CubicCardinalSpline::new_catmull_rom(ctrl)
        .to_curve()
        .expect("eight points make a curve");
    curve.position(TOP_BAR + u).into()
}

/// Four round lamps along the front hoop's top bar, chrome backs to the chase
/// camera over the open roof, lenses lit ahead.
fn light_bar(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let lr = 0.021 * l;
    let can = [
        (0.0, -0.030 * l),
        (lr * 0.75, -0.030 * l),
        (lr, -0.012 * l),
        (lr, 0.004 * l),
        (0.0, 0.004 * l),
    ];
    let lens = [(0.0, 0.0), (lr * 0.88, 0.0), (lr * 0.88, 0.006 * l)];
    let hoop = f.front_hoop();
    for u in [0.12f32, 0.37, 0.63, 0.88] {
        let p = on_top_bar(&hoop, u);
        let at = [p[0], p[1] + TUBE * l * 0.40 + lr * 0.80, p[2] + 0.006 * l];
        kids.push(solid(&can, 14, true, &c.bright, at, along_z()));
        kids.push(solid(
            &lens,
            14,
            false,
            &c.lamp,
            [at[0], at[1], at[2] + 0.002 * l],
            along_z(),
        ));
    }
}

/// The dune whip: a thin rod off the main hoop's near top corner and a
/// safety-orange pennant at its tip - held under [`AIR_CAP`] at every size.
fn whip(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let base = [NEAR * f.top_x, f.top_y, f.main_z];
    // The rod's full length, or what the cap leaves over the cage's top once
    // the pennant hangs off its tip - which binds only at the largest seeds.
    let over_ground = f.top_y + f.plan.datum_height();
    let h = (0.26 * l).min(AIR_CAP - 0.06 - over_ground - 0.06 * l);
    let tip = [base[0] * 1.03, base[1] + h, base[2] - 0.030 * l];
    kids.push(line(
        &[
            (base, 0.004 * l),
            (
                [base[0] * 1.015, base[1] + h * 0.5, base[2] - 0.012 * l],
                0.003 * l,
            ),
            (tip, 0.003 * l),
        ],
        6,
        &c.whip,
    ));
    let flag = [
        ([tip[0], tip[1] - 0.012 * l, tip[2]], 0.026 * l),
        ([tip[0], tip[1] - 0.016 * l, tip[2] - 0.050 * l], 0.018 * l),
        ([tip[0], tip[1] - 0.020 * l, tip[2] - 0.100 * l], 0.004 * l),
    ];
    kids.push(sweep(
        &flag,
        8,
        [0.22, 1.0, 1.0],
        [0.0, 1.0],
        0.0,
        c.pennant.clone(),
    ));
}

/// The raider's spare, hung on the roll bar behind the seats, face to the
/// chase camera: a front-size wheel on a bracket off the main hoop's top bar.
fn roll_bar_spare(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let (r, w) = (f.front.r, f.front.w);
    let hub = [0.0, (f.depth + f.top_y) * 0.52, f.main_z - w - 0.004 * l];
    let top = on_top_bar(&f.main_hoop(), 0.5);
    kids.push(line(
        &[
            (top, 0.009 * l),
            (
                [0.0, (top[1] + hub[1]) * 0.5, f.main_z - 0.020 * l],
                0.009 * l,
            ),
            (hub, 0.009 * l),
        ],
        6,
        &c.frame,
    ));
    spare(kids, c, hub, r, w, quat_x(-FRAC_PI_2));
}

/// Two jerrycans standing on the near nerf bar, lashed to the frame: an olive
/// can and a red one, real size (the roadster's first third-size can read as
/// a matchbox).
fn jerrycans(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let (l, d) = (f.l, f.depth);
    let size = [0.036 * l, 0.100 * l, 0.070 * l];
    let x = NEAR * (f.plan.hw(f.main_z) + 0.028 * l);
    let y = -d * 0.92 + RAIL * l * 0.40 + size[1] * 0.5;
    for (i, m) in [&c.can, &c.can_red].into_iter().enumerate() {
        let z = f.main_z + 0.085 * l + i as f32 * 0.078 * l;
        kids.push(prim(
            bevel(size, 0.004 * l, 4, m.clone()),
            [x, y, z],
            id_quat(),
        ));
    }
}

/// A rusted plate bolted across the nose hoop - the raider's Ornate mass,
/// where the brush bar is.
fn nose_plate(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let xn = f.plan.hw(f.nose_z);
    let (lo, hi) = (f.plan.floor(f.nose_z), f.plan.shoulder(f.nose_z));
    kids.push(board(
        [xn * 2.5, 0.012 * l, (hi - lo) + 0.030 * l],
        &c.rust,
        [0.0, (lo + hi) * 0.5, f.nose_z + 0.010 * l],
        quat_x(FRAC_PI_2),
        0.006 * l,
    ));
}

/// A striped surrey top on the cage roof: ONE upper half-pipe swept fore and
/// aft over the roof bars, its cut edges on them, cut into angular bands -
/// one node a stripe, white and the scheme's colour alternating (no texture
/// draws a stripe).
fn canopy(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let (aft, fwd) = (f.main_z - 0.030 * l, f.rake_z + 0.030 * l);
    let points: Vec<([f32; 3], f32)> = (0..5)
        .map(|i| {
            let u = i as f32 / 4.0;
            let z = aft + (fwd - aft) * u;
            let t = ((z - f.main_z) / (f.rake_z - f.main_z)).clamp(0.0, 1.0);
            let y = f.top_y + (f.rake_y - f.top_y) * t;
            (
                [0.0, y, z],
                f.top_x * (1.0 + 0.04 * (2.0 * u - 1.0).powi(2)),
            )
        })
        .collect();
    for k in 0..STRIPES {
        let band = [
            0.5 * k as f32 / STRIPES as f32,
            0.5 * (k + 1) as f32 / STRIPES as f32,
        ];
        let m = if k % 2 == 0 { &c.canvas } else { &c.stripe };
        kids.push(sweep(
            &points,
            8,
            [1.0, CANOPY_RISE, 1.0],
            band,
            0.90,
            m.clone(),
        ));
    }
}
