//! The buggy's engine, the hero mass from behind: an air-cooled flat four
//! hung behind the transaxle, as a VW's is - the transaxle between the rear
//! wheels, a cast case, two turned cylinder banks laid outboard with their
//! heads, the fan shroud (the one Superellipsoid: a pressed panel), a chrome
//! air-cleaner pot on its crown, headers down to a collector under the case,
//! and the STINGER swept up behind the engine to a flared mouth over the
//! shroud, raked aft - the line the chase camera sees.
//!
//! At a stock flat four's proportions it was a small black box between two
//! tyres each a third of the machine's width; drawn at 1.3 times them, with a
//! wide engine guard round it and the stinger raised over the shroud, it is
//! what the brief asked for.

use crate::pds::avatar::livery::BuggyColours;
use crate::pds::generator::Generator;

use super::super::super::common::{id_quat, prim, superellipsoid};
use super::frame::Frame;
use super::{AxleLine, ENGINE, UPRIGHT, along_z, board, line, outboard, solid};

/// Where the engine's parts sit, off the rear axle line (m).
pub(super) struct Engine {
    l: f32,
    /// The engine's own unit: the machine's length times [`ENGINE`].
    k: f32,
    /// The crankcase's centre.
    z: f32,
    y: f32,
    /// The case's full size.
    case: [f32; 3],
    /// The fan shroud's half extents, and its centre's height.
    shroud: [f32; 3],
    shroud_y: f32,
    /// The air cleaner's radius, height and foot.
    cleaner_r: f32,
    cleaner_h: f32,
    cleaner_y: f32,
    /// A cylinder bank's radius and length.
    cyl_r: f32,
    cyl_l: f32,
    /// The case's after face.
    rear: f32,
    /// The frame's tail, which the stinger sweeps up past.
    tail_z: f32,
}

impl Engine {
    /// The engine hung behind the rear axle.
    pub(super) fn new(l: f32, axle: AxleLine, tail_z: f32) -> Self {
        let k = ENGINE * l;
        let (z, y) = (axle.z - 0.112 * l, axle.y + 0.010 * l);
        let case = [0.090 * k, 0.075 * k, 0.110 * k];
        let shroud = [0.074 * k, 0.034 * k, 0.058 * k];
        let shroud_y = y + case[1] * 0.5 + shroud[1] * 0.80;
        Self {
            l,
            k,
            z,
            y,
            case,
            shroud,
            shroud_y,
            cleaner_r: 0.042 * k,
            cleaner_h: 0.036 * k,
            cleaner_y: shroud_y + shroud[1] * 0.85,
            cyl_r: 0.029 * k,
            cyl_l: 0.074 * k,
            rear: z - case[2] * 0.5,
            tail_z,
        }
    }

    /// The top of the engine: the air cleaner's lid (m over the datum). The
    /// engine guard stands over it.
    pub(super) fn peak(&self) -> f32 {
        self.cleaner_y + self.cleaner_h
    }

    /// The stinger's path: out of the collector under the case, back, and
    /// swept UP behind the engine to a flared mouth over the shroud's crown,
    /// raked aft.
    pub(super) fn stinger(&self) -> [([f32; 3], f32); 7] {
        let (l, t) = (self.l, self.tail_z);
        let low = self.y - self.case[1] * 0.5 - 0.012 * l;
        let crown = self.shroud_y + self.shroud[1];
        [
            ([0.0, low, self.z - 0.030 * l], 0.012 * l),
            ([0.0, low - 0.004 * l, self.rear - 0.012 * l], 0.012 * l),
            ([0.0, low + 0.016 * l, t + 0.016 * l], 0.013 * l),
            ([0.0, self.y + 0.030 * l, t + 0.002 * l], 0.015 * l),
            ([0.0, crown + 0.012 * l, t - 0.006 * l], 0.017 * l),
            ([0.0, crown + 0.052 * l, t - 0.020 * l], 0.021 * l),
            ([0.0, crown + 0.078 * l, t - 0.034 * l], 0.027 * l),
        ]
    }

    /// The stinger's mouth (root-local, m).
    pub(super) fn mouth(&self) -> [f32; 3] {
        let path = self.stinger();
        path[path.len() - 1].0
    }
}

/// The transaxle, the case, the cylinder banks, the shroud, the air cleaner,
/// the headers and the stinger. Battered: the exhaust gone to rust and a
/// replacement shroud in primer - wear where the camera looks.
pub(super) fn build(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours, battered: bool) {
    let (l, e) = (f.l, &f.engine);
    let pipe = if battered { &c.rust } else { &c.bright };
    // The transaxle, turned along z between the rear wheels, its bell to the
    // engine.
    let (from, to) = (f.rear.z - 0.050 * l, f.rear.z + 0.075 * l);
    let (tr, run) = (0.030 * l, to - from);
    let transaxle = [
        (0.0, 0.0),
        (tr * 1.35, 0.0),
        (tr * 1.30, 0.030 * l),
        (tr, 0.050 * l),
        (tr * 0.95, run - 0.012 * l),
        (tr * 0.55, run),
        (0.0, run),
    ];
    kids.push(solid(
        &transaxle,
        16,
        false,
        &c.alloy,
        [0.0, f.rear.y, from],
        along_z(),
    ));
    // The crankcase, bedded into the bell.
    kids.push(board(
        e.case,
        &c.alloy,
        [0.0, e.y, e.z + 0.004 * l],
        UPRIGHT,
        0.018 * e.k,
    ));
    // The two cylinder banks, turned outboard, their heads at their ends.
    let (cl, cr) = (e.cyl_l, e.cyl_r);
    let bank = [
        (0.0, 0.0),
        (cr * 0.92, 0.0),
        (cr, 0.012 * e.k),
        (cr, cl * 0.62),
        (cr * 1.14, cl * 0.66),
        (cr * 1.14, cl * 0.96),
        (cr * 0.70, cl),
        (0.0, cl),
    ];
    for s in [-1.0f32, 1.0] {
        kids.push(solid(
            &bank,
            16,
            false,
            &c.tin,
            [s * e.case[0] * 0.36, e.y + 0.003 * l, e.z],
            outboard(s),
        ));
    }
    // The fan shroud - the doghouse - a pressed panel on the case.
    let shroud = if battered { &c.primer } else { &c.tin };
    kids.push(prim(
        superellipsoid(e.shroud, 0.45, 0.50, 12, 20, shroud.clone()),
        [0.0, e.shroud_y, e.z],
        id_quat(),
    ));
    // The air cleaner, a turned pot on the shroud's crown.
    let (ar, ah) = (e.cleaner_r, e.cleaner_h);
    let pot = [
        (0.0, 0.0),
        (ar * 0.45, 0.0),
        (ar * 0.45, ah * 0.30),
        (ar, ah * 0.36),
        (ar, ah * 0.86),
        (ar * 0.30, ah),
        (0.0, ah),
    ];
    kids.push(solid(
        &pot,
        20,
        false,
        &c.bright,
        [0.0, e.cleaner_y, e.z + 0.006 * l],
        UPRIGHT,
    ));
    // The headers: out of each head, down and back in to the collector under
    // the case, where the stinger starts.
    let hx = e.case[0] * 0.36 + e.cyl_l * 0.80;
    let low = e.y - e.case[1] * 0.5 - 0.012 * l;
    for s in [-1.0f32, 1.0] {
        kids.push(line(
            &[
                ([s * hx, e.y - e.cyl_r * 0.6, e.z], 0.009 * l),
                ([s * hx * 0.80, low + 0.004 * l, e.z - 0.012 * l], 0.009 * l),
                ([s * 0.010 * l, low, e.z - 0.030 * l], 0.010 * l),
            ],
            6,
            pipe,
        ));
    }
    // The stinger, swept up behind the engine - the hero line from behind.
    kids.push(line(&e.stinger(), 10, pipe));
}
