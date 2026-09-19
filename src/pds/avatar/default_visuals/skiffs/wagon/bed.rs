//! What a wagon's bed is built from: the plank box, the sprung bench, the
//! driver's toe board, the canvas tilt and the lanterns.

use std::f32::consts::PI;

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;

use super::super::super::common::quat_x;
use super::super::dim;
use super::{WagonPlan, board, half_pipe, line, solid};

const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// What a box carries besides its floor and sides.
#[derive(Clone, Copy, Debug)]
pub(super) struct BoxBed {
    /// Side height (m).
    pub(super) side: f32,
    /// A tailboard across the back.
    pub(super) tail: bool,
    /// Three iron stakes down each side's outer face.
    pub(super) stakes: bool,
}

/// The floor, two sides, a front board and a tailboard - boards in the
/// scheme's colour - and an iron cap rail along each side, which is the line
/// that draws the box at 12 m.
pub(super) fn box_bed(kids: &mut Vec<Generator>, plan: &WagonPlan, b: BoxBed, c: &WagonColours) {
    let (l, hw) = (plan.length, plan.half_w());
    let (a, f) = plan.bed_z();
    let (ln, zc) = (f - a, (a + f) * 0.5);
    let ft = plan.floor_t();
    let h = b.side;
    let th = dim(l * 0.012);
    kids.push(board(
        [hw * 2.0, ft, ln],
        &c.timber,
        [0.0, 0.0, zc],
        UPRIGHT,
        l * 0.004,
    ));
    for s in [-1.0f32, 1.0] {
        let x = s * (hw - th * 0.5);
        kids.push(board(
            [th, h, ln],
            &c.boards,
            [x, h * 0.5, zc],
            UPRIGHT,
            th * 0.3,
        ));
        kids.push(line(
            &[
                ([x, h, a + th * 0.2], dim(th * 0.62)),
                ([x, h, f - th * 0.2], dim(th * 0.62)),
            ],
            6,
            &c.iron,
        ));
        if b.stakes {
            let xs = s * (hw + th * 0.25);
            for zf in [0.14f32, 0.50, 0.86] {
                let z = a + ln * zf;
                kids.push(line(
                    &[
                        ([xs, -ft * 0.5, z], dim(th * 0.45)),
                        ([xs, h * 1.02, z], dim(th * 0.45)),
                    ],
                    5,
                    &c.iron,
                ));
            }
        }
    }
    let inner = hw * 2.0 - th * 1.6;
    kids.push(board(
        [inner, h, th],
        &c.boards,
        [0.0, h * 0.5, f - th * 0.5],
        UPRIGHT,
        th * 0.3,
    ));
    if b.tail {
        kids.push(board(
            [inner, h * 0.92, th],
            &c.boards,
            [0.0, h * 0.46, a + th * 0.5],
            UPRIGHT,
            th * 0.3,
        ));
    }
}

/// Where a sprung bench stands.
#[derive(Clone, Copy, Debug)]
pub(super) struct Bench {
    /// Station (m).
    pub(super) z: f32,
    /// The top of whatever carries it - the side tops, a riser (m).
    pub(super) base_y: f32,
    /// The seat's top (m).
    pub(super) seat_y: f32,
    /// How far out the springs' feet stand (m).
    pub(super) base_x: f32,
    /// The seat's width (m).
    pub(super) width: f32,
    /// A back rest.
    pub(super) back: bool,
}

/// A bench on two S springs: the seat, its back and the springs, from the
/// springs' feet on whatever carries it up to the seat's underside.
pub(super) fn sprung_bench(
    kids: &mut Vec<Generator>,
    plan: &WagonPlan,
    b: Bench,
    c: &WagonColours,
) {
    let l = plan.length;
    let (st, sd) = (dim(l * 0.022), dim(l * 0.070));
    let (z, w) = (b.z, b.width);
    let y = b.seat_y - st;
    kids.push(board(
        [w, st, sd],
        &c.seat,
        [0.0, y + st * 0.5, z],
        UPRIGHT,
        l * 0.006,
    ));
    if b.back {
        let bh = l * 0.060;
        kids.push(board(
            [w, bh, dim(l * 0.014)],
            &c.seat,
            [0.0, y + st + bh * 0.45, z - sd * 0.5 - l * 0.004],
            quat_x(-0.22),
            l * 0.005,
        ));
    }
    let sr = dim(l * 0.0065);
    let rise = y - b.base_y;
    for s in [-1.0f32, 1.0] {
        let x = s * (w * 0.5 - l * 0.030);
        kids.push(line(
            &[
                ([s * b.base_x, b.base_y - l * 0.004, z - sd * 0.35], sr),
                ([x, b.base_y + rise * 0.35, z - sd * 0.05], sr),
                ([x, b.base_y + rise * 0.7, z + sd * 0.25], sr),
                ([x, y + st * 0.3, z + sd * 0.05], sr),
            ],
            6,
            &c.iron,
        ));
    }
}

/// The driver's toe board, sloping up and forward from `z0` at `lift` over
/// the floor, `reach` long at `angle` (rad) above level.
pub(super) fn footboard(
    kids: &mut Vec<Generator>,
    plan: &WagonPlan,
    z0: f32,
    lift: f32,
    reach: f32,
    angle: f32,
    c: &WagonColours,
) {
    let l = plan.length;
    let th = dim(l * 0.011);
    kids.push(board(
        [plan.half_w() * 1.9, th, reach],
        &c.timber,
        [
            0.0,
            lift + angle.sin() * reach * 0.5,
            z0 + angle.cos() * reach * 0.5,
        ],
        quat_x(-angle),
        th * 0.4,
    ));
}

/// The canvas tilt: ONE swept upper half-pipe over the bed from `a` to `f`,
/// bored thin so its ends are open, flared and raised at both ends - the
/// Conestoga's boat-shaped sheer - and scaled on y to put its crown at 1.45
/// times the blueprint height; a timber bow frames each open end, which is
/// the mouth the chase camera looks into.
pub(super) fn tilt(kids: &mut Vec<Generator>, plan: &WagonPlan, a: f32, f: f32, c: &WagonColours) {
    let (l, hw) = (plan.length, plan.half_w());
    let h = plan.depth();
    let rise = plan.height * 1.45 - plan.datum_height() - h;
    let sy = (rise / (hw * 1.02)).max(0.6);
    let pts: Vec<([f32; 3], f32)> = (0..7)
        .map(|i| {
            let t = i as f32 / 6.0;
            let e = (2.0 * t - 1.0).powi(4);
            (
                [0.0, h + l * 0.030 * e, a + (f - a) * t],
                hw * (1.02 + 0.14 * e),
            )
        })
        .collect();
    kids.push(half_pipe(&pts, sy, 0.93, &c.canvas));
    for &(p, r) in [pts[0], pts[6]].iter() {
        let arc: Vec<([f32; 3], f32)> = (0..7)
            .map(|k| {
                let ang = PI * k as f32 / 6.0;
                (
                    [
                        ang.cos() * r * 0.965,
                        p[1] + ang.sin() * r * 0.965 * sy,
                        p[2],
                    ],
                    dim(l * 0.0065),
                )
            })
            .collect();
        kids.push(line(&arc, 5, &c.timber));
    }
}

/// Where the near lantern hangs for a pair set at station `z`, on irons from
/// `y_base`, `x_out` out from the centreline (m).
pub(super) fn lantern_at(plan: &WagonPlan, z: f32, y_base: f32, x_out: f32) -> [f32; 3] {
    [x_out, y_base + plan.length * 0.07, z]
}

/// Two lanterns on iron brackets off the bed's corners: a turned lamp body in
/// the lit lamp material (no glass volume, rule 4) under a dark turned cap.
pub(super) fn lanterns(
    kids: &mut Vec<Generator>,
    plan: &WagonPlan,
    z: f32,
    y_base: f32,
    x_out: f32,
    c: &WagonColours,
) {
    let l = plan.length;
    let (lr, lh) = (dim(l * 0.020), l * 0.040);
    let body = [
        (0.0, -lh * 0.5),
        (lr * 0.8, -lh * 0.5),
        (lr, -lh * 0.3),
        (lr, lh * 0.3),
        (lr * 0.8, lh * 0.5),
        (0.0, lh * 0.5),
    ];
    let cap = [
        (0.0, 0.0),
        (lr * 1.15, 0.0),
        (lr * 0.5, lh * 0.35),
        (0.0, lh * 0.45),
    ];
    let iron = dim(l * 0.006);
    for s in [-1.0f32, 1.0] {
        let x0 = s * (plan.half_w() - l * 0.004);
        let [x1, yl, _] = lantern_at(plan, z, y_base, s * x_out);
        kids.push(line(
            &[
                ([x0, y_base, z], iron),
                ([x1, y_base + l * 0.02, z], iron),
                ([x1, yl - lh * 0.45, z], iron),
            ],
            5,
            &c.iron,
        ));
        kids.push(solid(&body, 14, false, &c.lamp, [x1, yl, z], UPRIGHT));
        kids.push(solid(
            &cap,
            12,
            false,
            &c.iron,
            [x1, yl + lh * 0.46, z],
            UPRIGHT,
        ));
    }
}
