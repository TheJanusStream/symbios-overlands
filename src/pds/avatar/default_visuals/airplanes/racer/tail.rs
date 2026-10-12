//! Her tail: a tapered stabiliser through the tail cone, its tips swept a
//! little aft, and over it a swept fin in her body's hue with its top in the
//! trim colour; and the white tail light at the tail post. Her full
//! proportion's (#1629 Q-I): the stabiliser 1.2 m across and the fin 8
//! percent larger than a true racer's.
//!
//! Every surface is a flattened sweep, the family's way: the stabiliser
//! drawn a tenth as thick as its chord, the fin 0.09 as wide. Both stand aft
//! of the cockpit's run, so its stretch carries them rigidly aft.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, sphere, sweep, with_cut};
use super::super::{FusePlan, RacerColours, dim};

/// The stabiliser's station - through the tail cone's centreline there -
/// its thickness over chord, and its stations along the half-span, `(x,
/// half-chord, dz)`, a true racer's: drawn [`STAB_SCALE`] as wide and as deep
/// in her full proportion, the tips' sweep as it was (prototype frame).
const STAB_Z: f32 = -1.30;
const STAB_THICK: f32 = 0.10;
const STAB: [(f32, f32, f32); 6] = [
    (0.0, 0.15, 0.0),
    (0.18, 0.145, -0.01),
    (0.34, 0.13, -0.025),
    (0.46, 0.11, -0.04),
    (0.52, 0.08, -0.05),
    (0.55, 0.035, -0.055),
];
const STAB_SCALE: f32 = 0.60 / 0.55;

/// The fin (prototype frame): its root's station, how far its root sits into
/// the tail cone's crown, its width over chord, and its stations up its
/// height, `(rise over the root, station, half-chord)` - a true racer's,
/// swept, rounded at its top - drawn [`FIN_SCALE`] as tall and as deep in her
/// full proportion.
const FIN_Z: f32 = -1.30;
const FIN_SINK: f32 = 0.035;
const FIN_WIDTH: f32 = 0.09;
const FIN: [(f32, f32, f32); 5] = [
    (0.0, -1.30, 0.20),
    (0.12, -1.36, 0.17),
    (0.24, -1.42, 0.14),
    (0.34, -1.47, 0.10),
    (0.40, -1.50, 0.04),
];
const FIN_SCALE: f32 = 1.08;

/// How far up the fin its top in the trim colour starts, as a share of its
/// length, and how proud of the fin it lies.
const CAP_FROM: f32 = 0.60;
const CAP_PROUD: f32 = 0.004;

/// The airframe's tail: the fin's trailing edge - its trim cap's, at its
/// fourth station, where the sweep's section is a ring square to a line
/// raked 25 degrees aft, so it reaches 0.91 of its radius aft - read back
/// off the drawn tree by `the_box_holds_the_drawn_airframe` (prototype
/// frame).
pub(super) const TAIL_REACH: f32 = -1.5723;

/// The tail light (prototype frame): its radius, and how far aft of the tail
/// post's end its middle stands, on the fuselage's own axis.
const TAIL_LIGHT_R: f32 = 0.018;
const TAIL_LIGHT_AFT: f32 = 0.005;

/// The stabiliser: a flattened sweep through the tail cone, in her body's
/// hue.
pub(super) fn stabiliser(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let y = plan.fuse_at(STAB_Z).0;
    let point = |x: f32, r: f32, dz: f32| {
        let [px, py, pz] = plan.at_off(x * STAB_SCALE, y, STAB_Z, dz);
        ([px, py / STAB_THICK, pz], plan.len(r * STAB_SCALE))
    };
    let mut points: Vec<([f32; 3], f32)> = STAB
        .iter()
        .skip(1)
        .rev()
        .map(|&(x, r, dz)| point(-x, r, dz))
        .collect();
    points.extend(STAB.iter().map(|&(x, r, dz)| point(x, r, dz)));
    let mut g = prim(sweep(&points, 18, 5, c.body.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, STAB_THICK, 1.0]);
    kids.push(g);
}

/// The fin's sweep points, each radius `proud` (prototype metres) over its
/// own - the fin itself at `0`, its top's cap laid on it.
fn fin_points(plan: &FusePlan, proud: f32) -> Vec<([f32; 3], f32)> {
    let root = plan.crown(FIN_Z) - FIN_SINK;
    FIN.iter()
        .map(|&(rise, z, r)| {
            let [px, py, pz] = plan.at(0.0, root + rise * FIN_SCALE, z);
            ([px / FIN_WIDTH, py, pz], plan.len(r * FIN_SCALE + proud))
        })
        .collect()
}

/// The fin in her body's hue, and its top in the trim colour.
pub(super) fn fin(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let mut fin = prim(
        sweep(&fin_points(plan, 0.0), 18, 5, c.body.clone()),
        [0.0; 3],
        id_quat(),
    );
    fin.transform.scale = Fp3([FIN_WIDTH, 1.0, 1.0]);
    kids.push(fin);
    let mut cap = prim(
        with_cut(
            sweep(&fin_points(plan, CAP_PROUD), 18, 5, c.trim.clone()),
            [0.0, 1.0],
            [CAP_FROM, 1.0],
            0.0,
        ),
        [0.0; 3],
        id_quat(),
    );
    cap.transform.scale = Fp3([FIN_WIDTH, 1.0, 1.0]);
    kids.push(cap);
}

/// The white tail light, just aft of the tail post's end.
pub(super) fn tail_light(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let (z, y, _) = plan.stations().last().expect("the fuselage has stations");
    kids.push(prim(
        sphere(dim(plan.len(TAIL_LIGHT_R)), 3, c.white.clone()),
        plan.at_off(0.0, y, z, -TAIL_LIGHT_AFT),
        id_quat(),
    ));
}
