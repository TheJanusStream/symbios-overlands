//! The buggy's cockpit: the small seat pod inside the frame and the two
//! high-back buckets standing in it, whose backs are the first thing in the
//! cockpit from the chase camera.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::livery::BuggyColours;
use crate::pds::generator::Generator;

use super::super::super::common::quat_x;
use super::frame::Frame;
use super::{NEAR, SECTION, UPRIGHT, board, sweep};

/// The pod's half-width inside the frame's.
const POD_W: f32 = 0.94;
/// How deep the pod is bored: a real tub, the seats standing in its floor.
const POD_HOLLOW: f32 = 0.80;

/// The pod, then the seats.
pub(super) fn build(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours, battered: bool) {
    pod(kids, f, c);
    seats(kids, f, c, battered);
}

/// The roadster's tub idiom: one bored LOWER half-pipe on the plan's own
/// stations, its cut rim on the datum, from the main hoop to the dash - its
/// ends rounded in a short station from each end, so the bore closes.
fn pod(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let (aft, fwd) = (f.main_z + 0.004 * l, f.dash_z - 0.004 * l);
    let run = f.plan.run(aft, fwd);
    let last = run.len() - 1;
    let mut points: Vec<([f32; 3], f32)> = run
        .iter()
        .enumerate()
        .map(|(i, &(p, r))| {
            let end = if i == 0 || i == last { 0.45 } else { 1.0 };
            (p, r * (POD_W * end))
        })
        .collect();
    let near_aft = aft + 0.035 * l;
    points.insert(1, ([0.0, 0.0, near_aft], f.plan.hw(near_aft) * POD_W));
    let near_fwd = fwd - 0.035 * l;
    points.insert(
        points.len() - 1,
        ([0.0, 0.0, near_fwd], f.plan.hw(near_fwd) * POD_W),
    );
    kids.push(sweep(
        &points,
        24,
        [1.0, SECTION / POD_W, 1.0],
        [0.5, 1.0],
        POD_HOLLOW,
        c.pod.clone(),
    ));
}

/// Two high-back buckets side by side in the pod, backs to the main hoop and
/// leaning back, on one cushion across both. A battered buggy's near seat
/// is taped across its back - wear where the camera looks.
fn seats(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours, taped: bool) {
    let (l, d) = (f.l, f.depth);
    let hw = f.plan.hw(f.main_z);
    let floor_y = -d * POD_HOLLOW * 0.96;
    let z = f.main_z + 0.040 * l;
    let lean = quat_x(FRAC_PI_2 - 0.22);
    let h = 0.150 * l;
    for s in [-1.0f32, 1.0] {
        let x = s * hw * 0.48;
        kids.push(board(
            [hw * 0.78, 0.018 * l, h],
            &c.seat,
            [x, floor_y + h * 0.47, z],
            lean,
            hw * 0.30,
        ));
        if taped && s == NEAR {
            kids.push(board(
                [hw * 0.80, 0.022 * l, 0.030 * l],
                &c.tape,
                [x, floor_y + h * 0.62, z - h * 0.15 * 0.22f32.sin()],
                lean,
                0.004 * l,
            ));
        }
    }
    kids.push(board(
        [hw * 1.70, 0.028 * l, 0.090 * l],
        &c.seat,
        [0.0, floor_y + 0.012 * l, z + 0.060 * l],
        UPRIGHT,
        0.012 * l,
    ));
}
