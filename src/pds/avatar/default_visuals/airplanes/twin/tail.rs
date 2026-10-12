//! Her tail (#1625 Q-A): the stabiliser across the tail cone's top, square
//! at its tips where the fins stand on it, and an oval fin on each tip in
//! her body's hue with its rudder's top in the trim colour, the Beech 18's
//! twin fins; and the white tail light at the tail post.
//!
//! Every surface is a flattened sweep, the family's way: the stabiliser
//! drawn a tenth as thick as its chord, the fins 0.09 as wide. Each fin has
//! a station on the stabiliser's own plane, so the two meet in a sample:
//! a flattened sweep offers `touch` no axis to meet (#1624). They stand aft
//! of the cabin, so the cabin's stretch carries them rigidly aft.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, sphere, sweep, with_cut};
use super::super::{FusePlan, TwinColours, dim};

/// The stabiliser's station, how far it sits into the tail cone's crown,
/// its thickness over chord, and its stations along the half-span, `(x,
/// half-chord)`, its full chord out to the fins (prototype frame).
const STAB_Z: f32 = -1.44;
const STAB_SINK: f32 = 0.02;
const STAB_THICK: f32 = 0.10;
const STAB: [(f32, f32); 5] = [
    (0.0, 0.17),
    (0.25, 0.17),
    (0.50, 0.168),
    (0.68, 0.165),
    (0.72, 0.16),
];

/// The fins (prototype frame): their station at the stabiliser, how far in
/// from its tips they stand, their width over chord, and their stations up
/// their height, `(rise over the stabiliser, dz, half-chord)` - a third of
/// each under it, raked aft and rounded at both ends.
const FIN_Z: f32 = -1.446;
const FIN_IN: f32 = 0.02;
const FIN_WIDTH: f32 = 0.09;
const FIN: [(f32, f32, f32); 6] = [
    (-0.12, 0.02, 0.09),
    (-0.05, 0.01, 0.15),
    (0.0, 0.0, 0.17),
    (0.14, -0.02, 0.17),
    (0.28, -0.05, 0.14),
    (0.38, -0.07, 0.06),
];

/// How far up each fin its rudder's top in the trim colour starts, as a
/// share of its length - the family's - and how proud of the fin it lies.
const CAP_FROM: f32 = 0.55;
const CAP_PROUD: f32 = 0.004;

/// The airframe's tail: the fins' trailing edges - their trim caps', where
/// the fins' sweeps bulge aft between their stations - read back off the
/// drawn tree by `the_box_holds_the_drawn_airframe` (prototype frame).
pub(super) const TAIL_REACH: f32 = -1.645;

/// The tail light's middle (prototype frame): just aft of the tail post's
/// end, on the fuselage's own axis.
const TAIL_LIGHT_AFT: f32 = 0.01;

/// The stabiliser's centre height (prototype frame): into the tail cone's
/// crown.
pub(super) fn stab_y(plan: &FusePlan) -> f32 {
    plan.crown(STAB_Z) - STAB_SINK
}

/// The stabiliser: a flattened sweep across the tail cone, in her body's
/// hue.
pub(super) fn stabiliser(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let y = stab_y(plan);
    let point = |x: f32, r: f32| {
        let [px, py, pz] = plan.at(x, y, STAB_Z);
        ([px, py / STAB_THICK, pz], plan.len(r))
    };
    let mut points: Vec<([f32; 3], f32)> = STAB
        .iter()
        .skip(1)
        .rev()
        .map(|&(x, r)| point(-x, r))
        .collect();
    points.extend(STAB.iter().map(|&(x, r)| point(x, r)));
    let mut g = prim(sweep(&points, 18, 5, c.body.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, STAB_THICK, 1.0]);
    kids.push(g);
}

/// The `x` each fin stands at (prototype frame, `side` 1 port): just in
/// from the stabiliser's tip.
pub(super) fn fin_x(side: f32) -> f32 {
    side * (STAB[STAB.len() - 1].0 - FIN_IN)
}

/// A fin's sweep points on `side`, each radius `proud` (prototype metres)
/// over its own - the fin itself at `0`, its rudder's cap laid on it.
fn fin_points(plan: &FusePlan, side: f32, proud: f32) -> Vec<([f32; 3], f32)> {
    let (x, y) = (fin_x(side), stab_y(plan));
    FIN.iter()
        .map(|&(rise, dz, r)| {
            let [px, py, pz] = plan.at_off(x, y + rise, FIN_Z, dz);
            ([px / FIN_WIDTH, py, pz], plan.len(r + proud))
        })
        .collect()
}

/// Where an adorned machine's aerial wire is made fast on the `side` fin,
/// drawn: 30 cm up it, 5 cm aft of its station at the stabiliser - inside
/// it, 8 cm under its top (prototype frame).
const WIRE_RISE: f32 = 0.30;
const WIRE_DZ: f32 = -0.05;

pub(super) fn wire_end(plan: &FusePlan, side: f32) -> [f32; 3] {
    plan.at_off(fin_x(side), stab_y(plan) + WIRE_RISE, FIN_Z, WIRE_DZ)
}

/// The fins, port (`+X`) first: each in her body's hue, its rudder's top in
/// the trim colour.
pub(super) fn fins(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    for side in [1.0f32, -1.0] {
        let mut fin = prim(
            sweep(&fin_points(plan, side, 0.0), 18, 5, c.body.clone()),
            [0.0; 3],
            id_quat(),
        );
        fin.transform.scale = Fp3([FIN_WIDTH, 1.0, 1.0]);
        kids.push(fin);
        let mut cap = prim(
            with_cut(
                sweep(&fin_points(plan, side, CAP_PROUD), 18, 5, c.trim.clone()),
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
}

/// The white tail light, just aft of the tail post's end.
pub(super) fn tail_light(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let (z, y, _) = plan.stations().last().expect("the fuselage has stations");
    kids.push(prim(
        sphere(dim(plan.len(0.020)), 3, c.white.clone()),
        plan.at_off(0.0, y, z, -TAIL_LIGHT_AFT),
        id_quat(),
    ));
}
