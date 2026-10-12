//! The two wings, what holds them - an N interplane strut each side, two N
//! cabane struts carrying the upper wing over the cockpits, and one X of
//! wires in each bay - and the lights they carry: red and green on the
//! upper wing's tips, a landing light in the lower port wing's leading edge.
//!
//! # Each wing is a flattened sweep
//!
//! The high-wing's way ([`super::super::high_wing`]): one sweep tip to tip,
//! its section a circle as round as half the chord and drawn [`THICK`] as
//! tall - an ellipse with rounded edges, which is what reads as a wing at
//! play distance - on her own eight stations a side, spaced evenly and
//! closing in toward the tip (#1616's loop trap), fifteen points inside the
//! sanitiser's sixteen. The upper wing stands 25 cm ahead of the lower,
//! [`GAP`](super::GAP) over the crown and flat across; the lower passes
//! through the fuselage under its longerons and rises with its dihedral.
//!
//! # Every strut's end is inside a wing
//!
//! A strut or a wire is a straight rod whose ends stand 1 to 2 cm inside the
//! wings' centre planes - at a spar's station, where each wing is thicker
//! than that - so each is one machine with both wings
//! (`her_struts_and_wires_end_inside_her_wings`), and a cabane foot is in
//! the fuselage's crown.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, rod, sphere, sweep};
use super::super::{BiplaneColours, FusePlan, MIN_DIM, dim};
use super::{CHORD, LOWER_LE, LOWER_SPAN, THICK, UPPER_LE, UPPER_SPAN, lower_y, upper_y};

/// The wing's stations along the half-span, `(x, half-chord)` - the
/// high-wing's own, at her 4.7 m span and 0.70 m chord, which both wings
/// read as shares of their own half-span and chord: spaced evenly out to
/// 87 % of it and closing in toward the tip, the last eighth drawing the
/// chord in. Eight a side, the middle shared.
const STATIONS: [(f32, f32); 8] = [
    (0.0, 0.35),
    (0.60, 0.35),
    (1.20, 0.35),
    (1.70, 0.35),
    (2.05, 0.35),
    (2.22, 0.29),
    (2.32, 0.18),
    (2.36, 0.05),
];

/// The interplane struts' span station, as a share of the lower wing's
/// half-span; how much further out their tops lean; and the two spars they
/// meet, a share of the chord back from each leading edge.
const STRUT_AT: f32 = 0.62;
const STRUT_LEAN: f32 = 1.04;
const SPARS: [f32; 2] = [0.25, 0.72];

/// How far each strut's end stands into its wing, off the wing's centre
/// plane: the interplane struts' and the cabane's 2 cm, the wires' 1 cm.
const STRUT_IN: f32 = 0.02;
const WIRE_IN: f32 = 0.01;

/// The struts' radii (prototype frame): the interplane posts, the cabane
/// posts, and a diagonal of either at the sanitiser's floor.
const POST_R: f32 = 0.014;
const CABANE_R: f32 = 0.013;

/// The cabane: its two posts' spars on the upper wing, the half-width its
/// tops stand at under it, how far its feet stand into the crown and how
/// far in from the flank there.
const CABANE_SPARS: [f32; 2] = [0.20, 0.75];
const CABANE_TOP_X: f32 = 0.17;
const CABANE_SINK: f32 = 0.03;
const CABANE_FOOT_IN: f32 = 0.8;

/// The wires' plane - the mid-chord, one X a side (#1623 Q-B) - and where
/// their roots stand: the lower wing's at a share of her girth, the upper
/// wing's at the cabane's half-width.
const WIRE_SPAR: f32 = 0.48;
const WIRE_ROOT_LOWER: f32 = 0.19;

/// The landing light's span station in the lower port wing.
const LANDING_X: f32 = 0.95;

/// A point at span station `x` and height `y`, a share `spar` of the chord
/// back from leading edge `le` - rigid off the stretched edge, as the wing
/// is drawn.
fn on_wing(plan: &FusePlan, x: f32, y: f32, le: f32, spar: f32) -> [f32; 3] {
    plan.at_off(x, y, le, -spar * CHORD)
}

/// One wing: a flattened sweep tip to tip, its leading edge at `le`, its
/// half-span `half`, its centre height `y_of(x)`.
fn wing(
    plan: &FusePlan,
    le: f32,
    half: f32,
    y_of: impl Fn(f32) -> f32,
    material: &SovereignMaterialSettings,
) -> Generator {
    let tip = STATIONS[STATIONS.len() - 1].0;
    let stations: Vec<(f32, f32)> = STATIONS
        .iter()
        .map(|&(x, r)| (x * half / tip, r * CHORD / 0.70))
        .collect();
    let point = |x: f32, r: f32| {
        let [px, py, pz] = on_wing(plan, x, y_of(x), le, 0.5);
        ([px, py / THICK, pz], plan.len(r))
    };
    let mut points: Vec<([f32; 3], f32)> = stations
        .iter()
        .skip(1)
        .rev()
        .map(|&(x, r)| point(-x, r))
        .collect();
    points.extend(stations.iter().map(|&(x, r)| point(x, r)));
    let mut g = prim(sweep(&points, 24, 4, material.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, THICK, 1.0]);
    g
}

/// The two wings, the lower one first.
pub(super) fn wings(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    kids.push(wing(
        plan,
        LOWER_LE,
        LOWER_SPAN * 0.5,
        |x| lower_y(plan, x),
        &c.wings,
    ));
    kids.push(wing(
        plan,
        UPPER_LE,
        UPPER_SPAN * 0.5,
        |_| upper_y(plan),
        &c.wings,
    ));
}

/// An interplane strut's four ends each side, `(lower front, lower rear,
/// upper front, upper rear)`: the posts stand on the two spars, leaning
/// out a little toward the upper wing's longer span.
pub(super) fn post_ends(plan: &FusePlan, side: f32) -> [[f32; 3]; 4] {
    let x = side * STRUT_AT * LOWER_SPAN * 0.5;
    let low = lower_y(plan, x) + STRUT_IN;
    let high = upper_y(plan) - STRUT_IN;
    [
        on_wing(plan, x, low, LOWER_LE, SPARS[0]),
        on_wing(plan, x, low, LOWER_LE, SPARS[1]),
        on_wing(plan, x * STRUT_LEAN, high, UPPER_LE, SPARS[0]),
        on_wing(plan, x * STRUT_LEAN, high, UPPER_LE, SPARS[1]),
    ]
}

/// The interplane struts: an N each side - a post on each spar and a
/// diagonal from the rear post's foot to the front post's head.
pub(super) fn interplane_struts(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let post = dim(plan.len(POST_R));
    let diagonal = dim(plan.len(MIN_DIM));
    for side in [-1.0f32, 1.0] {
        let [lf, lr, uf, ur] = post_ends(plan, side);
        kids.push(rod(lf, uf, post, c.metal.clone()));
        kids.push(rod(lr, ur, post, c.metal.clone()));
        kids.push(rod(lr, uf, diagonal, c.metal.clone()));
    }
}

/// The cabane: an N each side carrying the upper wing's centre section over
/// the front cockpit - its feet in the crown a little in from each flank,
/// its heads under the upper wing.
pub(super) fn cabane_struts(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let post = dim(plan.len(CABANE_R));
    let diagonal = dim(plan.len(MIN_DIM));
    let head_y = upper_y(plan) - STRUT_IN;
    for side in [-1.0f32, 1.0] {
        // A foot stands under its head, rigidly off the upper wing's edge,
        // so the fuselage is read where the foot is drawn (`unstretch`).
        let foot = |spar: f32| {
            let z = plan.unstretch(plan.stretch(UPPER_LE) - spar * CHORD);
            let y = plan.crown(z) - CABANE_SINK;
            plan.at(side * plan.flank(z, y) * CABANE_FOOT_IN, y, z)
        };
        let head = |spar: f32| on_wing(plan, side * CABANE_TOP_X, head_y, UPPER_LE, spar);
        let [front, rear] = CABANE_SPARS;
        kids.push(rod(foot(front), head(front), post, c.metal.clone()));
        kids.push(rod(foot(rear), head(rear), post, c.metal.clone()));
        kids.push(rod(foot(rear), head(front), diagonal, c.metal.clone()));
    }
}

/// The flying and landing wires (#1623 Q-B): one X a side in the mid-chord
/// plane, from each wing's root to the other wing at the interplane strut.
pub(super) fn wires(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let r = dim(plan.len(MIN_DIM));
    let g = plan.girth();
    for side in [-1.0f32, 1.0] {
        let x = side * STRUT_AT * LOWER_SPAN * 0.5;
        let root = WIRE_ROOT_LOWER * g;
        let lower_root = on_wing(
            plan,
            side * root,
            lower_y(plan, root) + WIRE_IN,
            LOWER_LE,
            WIRE_SPAR,
        );
        let upper_out = on_wing(
            plan,
            x * STRUT_LEAN,
            upper_y(plan) - WIRE_IN,
            UPPER_LE,
            WIRE_SPAR,
        );
        let upper_root = on_wing(
            plan,
            side * CABANE_TOP_X,
            upper_y(plan) - WIRE_IN,
            UPPER_LE,
            WIRE_SPAR,
        );
        let lower_out = on_wing(plan, x, lower_y(plan, x) + WIRE_IN, LOWER_LE, WIRE_SPAR);
        kids.push(wire(lower_root, upper_out, r, &c.metal));
        kids.push(wire(upper_root, lower_out, r, &c.metal));
    }
}

/// A wire: a straight rod at six facets, the prototype's.
fn wire(a: [f32; 3], b: [f32; 3], radius: f32, material: &SovereignMaterialSettings) -> Generator {
    prim(
        sweep(&[(a, radius), (b, radius)], 6, 2, material.clone()),
        [0.0; 3],
        id_quat(),
    )
}

/// The lights: red on the upper wing's port tip (her left, `+X`) and green
/// on its starboard one, seated in the tips; and a landing light in the
/// lower port wing's leading edge.
pub(super) fn lights(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    let half = UPPER_SPAN * 0.5;
    for (side, lamp) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        let [px, py, pz] = on_wing(plan, side * (half - 0.015), upper_y(plan), UPPER_LE, 0.5);
        kids.push(prim(
            sphere(dim(plan.len(0.022)), 3, lamp.clone()),
            [px, py, pz + plan.len(0.02)],
            id_quat(),
        ));
    }
    kids.push(prim(
        sphere(dim(plan.len(0.026)), 3, c.white.clone()),
        plan.at_off(LANDING_X, lower_y(plan, LANDING_X), LOWER_LE, -0.02),
        id_quat(),
    ));
}
