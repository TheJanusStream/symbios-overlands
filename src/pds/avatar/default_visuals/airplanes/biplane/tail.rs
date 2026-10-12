//! Her tail: the stabiliser across the tail cone in her wings' hue, the fin
//! on it in her fuselage's with its top in the trim colour, and the white
//! tail light on the rudder's trailing edge.
//!
//! Both surfaces are flattened sweeps with rounded tips, the wings' way
//! ([`super::wings`]): the stabiliser drawn a tenth as thick as its chord,
//! the fin 0.09 as wide. They stand aft of the cockpits, so the cabin
//! stretch carries them rigidly aft rather than drawing them out.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, sphere, sweep, with_cut};
use super::super::{BiplaneColours, FusePlan, dim};

/// The stabiliser's station, how far it sits into the tail cone's crown,
/// its thickness over chord, and its stations along the half-span, `(x,
/// half-chord)`, closing in at the tip (prototype frame).
const STAB_Z: f32 = -1.24;
const STAB_SINK: f32 = 0.03;
const STAB_THICK: f32 = 0.10;
const STAB: [(f32, f32); 6] = [
    (0.0, 0.18),
    (0.24, 0.18),
    (0.44, 0.18),
    (0.56, 0.15),
    (0.63, 0.10),
    (0.66, 0.035),
];

/// The fin's root station, how far its root sits into the crown, its width
/// over chord, and its stations up its height, `(rise, station,
/// half-chord)` - raked aft and rounded at the top (prototype frame).
const FIN_ROOT_Z: f32 = -1.22;
const FIN_SINK: f32 = 0.03;
const FIN_WIDTH: f32 = 0.09;
const FIN: [(f32, f32, f32); 4] = [
    (0.0, -1.22, 0.20),
    (0.17, -1.28, 0.18),
    (0.32, -1.34, 0.13),
    (0.40, -1.36, 0.05),
];

/// How far up the fin its top in the trim colour starts, as a share of its
/// length - the high-wing's.
const FIN_CAP_FROM: f32 = 0.55;

/// How far up the rudder's trailing edge the tail light stands, and how
/// far its middle stands inside that edge (prototype frame): proud of it
/// aft and to both sides, the fin being 3.6 cm thick there.
const TAIL_LIGHT_RISE: f32 = 0.08;
const TAIL_LIGHT_IN: f32 = 0.008;

/// How far over the fin's top station an ornate machine's beacon stands
/// (prototype frame): its lowest point in the fin.
pub(super) const BEACON_OVER: f32 = 0.012;

/// The fin's root height (prototype frame): into the tail cone's crown.
fn fin_root(plan: &FusePlan) -> f32 {
    plan.crown(FIN_ROOT_Z) - FIN_SINK
}

/// The stabiliser: a flattened sweep across the tail cone, rounded at the
/// tips, in her wings' hue.
pub(super) fn stabiliser(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
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
    let mut g = prim(sweep(&points, 18, 5, c.wings.clone()), [0.0; 3], id_quat());
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

/// The fin, and its top in the trim colour: the fin's OWN stations a hair
/// proud, kept from [`FIN_CAP_FROM`] of the way up, as the high-wing's.
pub(super) fn fin(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
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

/// The white tail light on the rudder's trailing edge, [`TAIL_LIGHT_RISE`]
/// up it: the edge read off the fin's two lowest stations, as the
/// prototype read it.
pub(super) fn tail_light(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let ((r0, z0, c0), (r1, z1, c1)) = (FIN[0], FIN[1]);
    let t = (TAIL_LIGHT_RISE - r0) / (r1 - r0);
    let edge = z0 + (z1 - z0) * t - (c0 + (c1 - c0) * t);
    kids.push(prim(
        sphere(dim(plan.len(0.016)), 3, c.white.clone()),
        plan.at(0.0, fin_root(plan) + TAIL_LIGHT_RISE, edge + TAIL_LIGHT_IN),
        id_quat(),
    ));
}

/// The fin's top (prototype frame): where an ornate machine's beacon sits,
/// on the fin's own axis.
pub(super) fn fin_top(plan: &FusePlan) -> (f32, f32) {
    let (rise, z, _) = FIN[FIN.len() - 1];
    (fin_root(plan) + rise, z)
}
