//! Her tail: the stabiliser across the tail cone, the fin on it with its
//! top in the scheme's second hue, and the white tail light on the rudder.
//!
//! Both surfaces are flattened sweeps with rounded tips, the wing's way
//! ([`super::wing`]): the stabiliser drawn a tenth as thick as its chord,
//! the fin a ninth as wide. They stand aft of the cabin, so the cabin
//! stretch carries them rigidly aft rather than drawing them out.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, sphere, with_cut};
use super::super::{AirplaneColours, FusePlan, dim};
use super::sweep;

/// The stabiliser's station and chord, how far it sits into the tail
/// cone's crown, and its stations along the half-span, `(x, half-chord)`,
/// closing in at the tip (prototype frame).
const STAB_Z: f32 = -1.29;
const STAB_SINK: f32 = 0.035;
const STAB_THICK: f32 = 0.10;
const STAB: [(f32, f32); 6] = [
    (0.0, 0.20),
    (0.30, 0.20),
    (0.52, 0.20),
    (0.64, 0.17),
    (0.71, 0.11),
    (0.74, 0.04),
];

/// The fin's root station, how far its root sits into the crown, and its
/// stations up its height, `(rise, station, half-chord)` - raked aft and
/// rounded at the top (prototype frame).
const FIN_ROOT_Z: f32 = -1.30;
const FIN_SINK: f32 = 0.03;
const FIN_WIDTH: f32 = 0.09;
const FIN: [(f32, f32, f32); 4] = [
    (0.0, -1.30, 0.22),
    (0.20, -1.38, 0.19),
    (0.38, -1.44, 0.13),
    (0.46, -1.46, 0.05),
];

/// How far up the fin its top in the trim colour starts, as a share of its
/// length.
const FIN_CAP_FROM: f32 = 0.55;

/// The fin's root height (prototype frame): into the tail cone's crown.
fn fin_root(plan: &FusePlan) -> f32 {
    plan.crown(FIN_ROOT_Z) - FIN_SINK
}

/// The stabiliser: a flattened sweep across the tail cone, rounded at the
/// tips.
pub(super) fn stabiliser(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    let y = plan.crown(STAB_Z) - STAB_SINK;
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

/// The fin's sweep points, each radius `proud` (prototype metres) over its
/// own - the fin itself at `0`, its trim cap laid on it.
fn fin_points(plan: &FusePlan, proud: f32) -> Vec<([f32; 3], f32)> {
    let root = fin_root(plan);
    FIN.iter()
        .map(|&(rise, z, r)| {
            let [x, y, z] = plan.at(0.0, root + rise, z);
            ([x / FIN_WIDTH, y, z], plan.len(r + proud))
        })
        .collect()
}

/// The fin, and its top in the scheme's second hue: the fin's OWN stations
/// a hair proud, kept from [`FIN_CAP_FROM`] of the way up - a cap on
/// stations of its own drew a ragged edge across the fin (#1616).
pub(super) fn fin(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    let mut g = prim(
        sweep(&fin_points(plan, 0.0), 18, 5, c.body.clone()),
        [0.0; 3],
        id_quat(),
    );
    g.transform.scale = Fp3([FIN_WIDTH, 1.0, 1.0]);
    kids.push(g);
    let mut cap = prim(
        with_cut(
            sweep(&fin_points(plan, 0.004), 18, 5, c.trim.clone()),
            [0.0, 1.0],
            [FIN_CAP_FROM, 1.0],
            0.0,
        ),
        [0.0; 3],
        id_quat(),
    );
    cap.transform.scale = Fp3([FIN_WIDTH, 1.0, 1.0]);
    kids.push(cap);
}

/// The white tail light on the rudder's trailing edge.
pub(super) fn tail_light(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    kids.push(prim(
        sphere(dim(plan.len(0.016)), 3, c.white.clone()),
        plan.at(0.0, fin_root(plan) + 0.10, -1.53),
        id_quat(),
    ));
}

/// The fin's top (prototype frame): where an ornate machine's beacon sits,
/// down into it on the fin's own axis.
pub(super) fn fin_top(plan: &FusePlan) -> (f32, f32) {
    let (rise, z, _) = FIN[FIN.len() - 1];
    (fin_root(plan) + rise, z)
}
