//! The ladder of secondary masses by ornateness - #1623 Q-F, agreed: the
//! high-wing's shape, so the family reads alike by tier. An adorned machine
//! carries a pitot on her port interplane strut and a dark wing walk on
//! each lower wing root; an ornate one the high-wing's wheel spats, grown
//! with her tyre, and an anti-collision beacon on her fin as well. Wear has
//! no ladder of its own: it is the
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s grime on every
//! surface.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{cuboid, id_quat, prim, rod, sphere, sweep, with_cut};
use super::super::{BiplaneColours, FusePlan, MIN_DIM, dim};
use super::tail::{BEACON_OVER, fin_top};
use super::{CHORD, LOWER_LE, MAIN_Z, TRACK, UPPER_LE, WHEEL_R, half_thick, lower_y};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &FusePlan,
    c: &BiplaneColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    pitot(kids, plan, c);
    wing_walks(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        spats(kids, plan, c);
        beacon(kids, plan, c);
    }
}

/// How far up the port front interplane post the pitot's root stands, as a
/// share of the post from foot to head, and how far ahead of the upper
/// wing's leading edge its tip reaches (prototype frame).
const PITOT_UP: f32 = 0.85;
const PITOT_AHEAD: f32 = 0.12;

/// A pitot tube on the port (her left, `+X`) front interplane post, pointing
/// ahead past the upper wing's leading edge - its root in the post.
fn pitot(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let [foot, _, head, _] = super::wings::post_ends(plan, 1.0);
    let root: [f32; 3] = std::array::from_fn(|i| foot[i] + (head[i] - foot[i]) * PITOT_UP);
    let tip = [
        root[0],
        root[1],
        plan.at_off(0.0, 0.0, UPPER_LE, PITOT_AHEAD)[2],
    ];
    kids.push(rod(root, tip, dim(plan.len(MIN_DIM)), c.metal.clone()));
}

/// A wing walk's size (span, depth, chord), how far out of the flank it
/// stands, and how far it sinks into the lower wing's crown (prototype
/// frame).
const WALK: [f32; 3] = [0.26, 0.012, 0.36];
const WALK_OUT: f32 = 0.12;
const WALK_SINK: f32 = 0.004;

/// The flank's height a wing walk's inboard edge is read at, as the lower
/// wing's centre height at that span station (prototype frame).
const WALK_FLANK_AT: f32 = 0.2;

/// A dark wing walk on each lower wing root, beside the fuselage, sunk to
/// half its depth in the wing's crown at its mid-chord.
fn wing_walks(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    // The flank is read where the walk is drawn: rigidly off the lower
    // wing's stretched edge (`unstretch`).
    let z = plan.unstretch(plan.stretch(LOWER_LE) - CHORD * 0.5);
    let out = plan.flank(z, lower_y(plan, WALK_FLANK_AT)) + WALK_OUT;
    for side in [-1.0f32, 1.0] {
        kids.push(prim(
            cuboid(
                [plan.len(WALK[0]), dim(plan.len(WALK[1])), plan.len(WALK[2])],
                c.dark.clone(),
            ),
            plan.at_off(
                side * out,
                lower_y(plan, out) + half_thick() - WALK_SINK,
                LOWER_LE,
                -CHORD * 0.5,
            ),
            id_quat(),
        ));
    }
}

/// A wheel spat's stations along her, `(dz from the axle, radius)` - the
/// high-wing's, at her 0.155 m tyre - and how narrow it is drawn.
const SPAT: [(f32, f32); 5] = [
    (0.24, 0.03),
    (0.12, 0.15),
    (0.0, 0.19),
    (-0.14, 0.17),
    (-0.28, 0.04),
];
const SPAT_TYRE: f32 = 0.155;
const SPAT_WIDTH: f32 = 0.5;

/// A spat over each main tyre in her fuselage's hue: the high-wing's
/// teardrop grown with her tyre, kept above its axle - the tyre's lower half
/// shows, where it meets the ground. Not on the rolling hub: a spat stays
/// put while its tyre turns.
fn spats(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let grow = WHEEL_R / SPAT_TYRE;
    for side in [-1.0f32, 1.0] {
        let points: Vec<([f32; 3], f32)> = SPAT
            .iter()
            .map(|&(dz, r)| {
                let [x, y, z] = plan.at_off(side * TRACK, WHEEL_R, MAIN_Z, dz * grow);
                ([x / SPAT_WIDTH, y, z], plan.len(r * grow))
            })
            .collect();
        // The upper half: +X, the crown, -X - symmetric about the crown, so
        // the same whichever way round the sweep's frame turns.
        let mut g = prim(
            with_cut(
                sweep(&points, 20, 6, c.body.clone()),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        );
        g.transform.scale = Fp3([SPAT_WIDTH, 1.0, 1.0]);
        kids.push(g);
    }
}

/// The anti-collision beacon, its lowest point in the fin's top.
fn beacon(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let (y, z) = fin_top(plan);
    kids.push(prim(
        sphere(dim(plan.len(0.018)), 3, c.beacon.clone()),
        plan.at(0.0, y + BEACON_OVER, z),
        id_quat(),
    ));
}
