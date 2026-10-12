//! Her tail: the stabiliser across the tail cone in her wings' hue with an
//! end plate turned up at each tip, the fin over the tail post in her
//! fuselage's with its top in the trim colour, and her lights - red and
//! green on the end plates, white on the rudder's trailing edge.
//!
//! Every surface is a flattened sweep with rounded tips, the biplane's way:
//! the stabiliser drawn a tenth as thick as its chord, the end plates and
//! the fin 0.09 as wide. They stand aft of the boom's root, so each rides
//! its own station's stretch rigidly, rather than being drawn out with it.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, sphere, sweep, with_cut};
use super::super::{AutogyroColours, PodPlan, dim};

/// The stabiliser's station, how far it sits into the tail cone's crown,
/// its thickness over chord, its half-span, and its stations along the
/// half-span, `(x, half-chord)`, closing in at the tip (prototype frame).
const STAB_Z: f32 = -1.50;
const STAB_SINK: f32 = 0.03;
const STAB_THICK: f32 = 0.10;
const STAB_HALF: f32 = 0.56;
const STAB: [(f32, f32); 5] = [
    (0.0, 0.16),
    (0.20, 0.16),
    (0.38, 0.16),
    (0.50, 0.14),
    (STAB_HALF, 0.11),
];

/// The end plates (prototype frame): how far in from each tip they stand,
/// their width over chord, and their stations up their height, `(rise over
/// the stabiliser, dz from its station, half-chord)` - raked aft and
/// rounded at the top.
const PLATE_IN: f32 = 0.01;
const PLATE_WIDTH: f32 = 0.09;
const PLATE: [(f32, f32, f32); 4] = [
    (-0.05, 0.01, 0.12),
    (0.07, -0.02, 0.12),
    (0.17, -0.05, 0.08),
    (0.21, -0.06, 0.03),
];

/// The fin's root station, how far its root sits into the crown, its width
/// over chord, and its stations up its height, `(rise, dz from the root
/// station, half-chord)` - raked aft and rounded at the top (prototype
/// frame).
const FIN_ROOT_Z: f32 = -1.46;
const FIN_SINK: f32 = 0.03;
const FIN_WIDTH: f32 = 0.09;
const FIN: [(f32, f32, f32); 4] = [
    (0.0, 0.0, 0.19),
    (0.16, -0.06, 0.17),
    (0.30, -0.11, 0.12),
    (0.37, -0.13, 0.045),
];

/// How far up the fin its top in the trim colour starts, as a share of its
/// length - the biplane's.
const FIN_CAP_FROM: f32 = 0.55;

/// A nav light's middle (prototype frame): outboard of its end plate's
/// middle, up it and aft of the stabiliser's station.
const NAV_OUT: f32 = 0.008;
const NAV_RISE: f32 = 0.09;
const NAV_AFT: f32 = 0.03;

/// How far up the rudder's trailing edge the tail light stands, and how
/// far its middle stands inside that edge (prototype frame) - the
/// biplane's.
const TAIL_LIGHT_RISE: f32 = 0.08;
const TAIL_LIGHT_IN: f32 = 0.008;

/// The stabiliser's centre height (prototype frame): into the tail cone's
/// crown.
fn stab_y(plan: &PodPlan) -> f32 {
    plan.crown(STAB_Z) - STAB_SINK
}

/// The fin's root height (prototype frame): into the tail cone's crown.
fn fin_root(plan: &PodPlan) -> f32 {
    plan.crown(FIN_ROOT_Z) - FIN_SINK
}

/// The stabiliser: a flattened sweep across the tail cone, rounded at the
/// tips, in her wings' hue.
pub(super) fn stabiliser(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
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
    let mut g = prim(sweep(&points, 18, 5, c.wings.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, STAB_THICK, 1.0]);
    kids.push(g);
}

/// The end plates: one turned up at each tip, starboard first - flattened
/// sweeps in her wings' hue.
pub(super) fn end_plates(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let y = stab_y(plan);
    for side in [-1.0f32, 1.0] {
        let x = side * (STAB_HALF - PLATE_IN);
        let points: Vec<([f32; 3], f32)> = PLATE
            .iter()
            .map(|&(rise, dz, r)| {
                let [px, py, pz] = plan.at_off(x, y + rise, STAB_Z, dz);
                ([px / PLATE_WIDTH, py, pz], plan.len(r))
            })
            .collect();
        let mut g = prim(sweep(&points, 14, 4, c.wings.clone()), [0.0; 3], id_quat());
        g.transform.scale = Fp3([PLATE_WIDTH, 1.0, 1.0]);
        kids.push(g);
    }
}

/// The fin's sweep points, each radius `proud` (prototype metres) over its
/// own - the fin itself at `0`, its trim cap laid on it.
fn fin_points(plan: &PodPlan, proud: f32) -> Vec<([f32; 3], f32)> {
    let root = fin_root(plan);
    FIN.iter()
        .map(|&(rise, dz, r)| {
            let [x, y, z] = plan.at_off(0.0, root + rise, FIN_ROOT_Z, dz);
            ([x / FIN_WIDTH, y, z], plan.len(r + proud))
        })
        .collect()
}

/// The fin, and its top in the trim colour: the fin's OWN stations a hair
/// proud, kept from [`FIN_CAP_FROM`] of the way up, as the biplane's.
pub(super) fn fin(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
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

/// Her lights: red on the port (her left, `+X`) end plate and green on the
/// starboard, each a little outboard of its plate; and the white tail light
/// on the rudder's trailing edge, [`TAIL_LIGHT_RISE`] up it - the edge read
/// off the fin's two lowest stations, as the prototype read it.
pub(super) fn lights(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let y = stab_y(plan);
    for (side, lamp) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        kids.push(prim(
            sphere(dim(plan.len(0.02)), 3, lamp.clone()),
            plan.at_off(
                side * (STAB_HALF - PLATE_IN + NAV_OUT),
                y + NAV_RISE,
                STAB_Z,
                -NAV_AFT,
            ),
            id_quat(),
        ));
    }
    let ((r0, z0, c0), (r1, z1, c1)) = (FIN[0], FIN[1]);
    let t = (TAIL_LIGHT_RISE - r0) / (r1 - r0);
    let edge = z0 + (z1 - z0) * t - (c0 + (c1 - c0) * t);
    kids.push(prim(
        sphere(dim(plan.len(0.016)), 3, c.white.clone()),
        plan.at_off(
            0.0,
            fin_root(plan) + TAIL_LIGHT_RISE,
            FIN_ROOT_Z,
            edge + TAIL_LIGHT_IN,
        ),
        id_quat(),
    ));
}
