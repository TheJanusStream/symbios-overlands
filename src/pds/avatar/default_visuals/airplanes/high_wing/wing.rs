//! The wing on her cabin roof, the V of struts under each half, and the
//! lights it carries: red and green on the tips, a landing light in the
//! port leading edge.
//!
//! # A wing is a flattened sweep
//!
//! One sweep tip to tip, its section a circle as round as half the chord
//! and drawn [`THICK`] as tall - an ellipse with a rounded leading and
//! trailing edge, which is what reads as a wing at play distance. Its
//! stations are spaced EVENLY along the span and close in gradually at the
//! tips: a sweep's curve is a Catmull-Rom parameterised by station, not by
//! distance, and the prototype's first wing - its stations bunched at the
//! tips beside a 4 m gap - looped the curve 8 cm past each tip and back,
//! a full-chord blob that drew the tips square (#1616). The chord is drawn in
//! over the last 30 cm, so the wing ends rounded in plan.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, sphere};
use super::super::{AirplaneColours, FusePlan, dim};
use super::{CHORD, MAIN_Z, SPAN, THICK, WING_LE, rod, sweep, wing_under, wing_y};

/// The wing's stations along the half-span, `(x, half-chord)` at the
/// prototype's 4.7 m - spaced evenly out to 2.05 m and closing in toward the
/// tip at 2.36, the last 30 cm drawing the chord in. Eight a side, the
/// middle shared: fifteen, inside the sanitiser's sixteen
/// ([`MAX_SWEEP_POINTS`](crate::pds::sanitize::limits::MAX_SWEEP_POINTS)),
/// which dropped the prototype's seventeenth tip.
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

/// The span station the struts meet the wing at, as a share of the
/// half-span, and the two spars they meet - a share of the chord back from
/// the leading edge.
const STRUT_AT: f32 = 0.60;
const SPARS: [f32; 2] = [0.18, 0.70];
const STRUT_R: f32 = 0.013;

/// The wing's chord-wise centre: its leading edge on the cabin, the chord
/// rigid behind it.
fn mid_chord(plan: &FusePlan, x: f32, y: f32) -> [f32; 3] {
    plan.at_off(x, y, WING_LE, -CHORD * 0.5)
}

/// The wing: one flattened sweep tip to tip on the cabin roof.
pub(super) fn wing(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    let half = SPAN * 0.5;
    let tip = STATIONS[STATIONS.len() - 1].0;
    let stations: Vec<(f32, f32)> = STATIONS
        .iter()
        .map(|&(x, r)| (x * half / tip, r * CHORD / 0.70))
        .collect();
    let point = |x: f32, r: f32| {
        let [px, py, pz] = mid_chord(plan, x, wing_y(plan, x));
        ([px, py / THICK, pz], plan.len(r))
    };
    let mut points: Vec<([f32; 3], f32)> = stations
        .iter()
        .skip(1)
        .rev()
        .map(|&(x, r)| point(-x, r))
        .collect();
    points.extend(stations.iter().map(|&(x, r)| point(x, r)));
    let mut g = prim(sweep(&points, 24, 4, c.body.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, THICK, 1.0]);
    kids.push(g);
}

/// The struts: a V each side from the lower longeron by the gear to the
/// front and rear spars at [`STRUT_AT`] of the half-span.
pub(super) fn struts(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    let half = SPAN * 0.5;
    let foot_z = MAIN_Z - 0.06;
    let foot_y = plan.sill(foot_z) + 0.08;
    let foot_x = plan.flank(foot_z, foot_y);
    for side in [1.0f32, -1.0] {
        let x = side * STRUT_AT * half;
        for spar in SPARS {
            let top = plan.at_off(x, wing_under(plan, x) + 0.01, WING_LE, -spar * CHORD);
            kids.push(rod(
                plan.at(side * foot_x, foot_y, foot_z),
                top,
                dim(plan.len(STRUT_R)),
                c.metal.clone(),
            ));
        }
    }
}

/// The wing's lights: red on the port tip (her left, `+X`) and green on the
/// starboard, seated in the tips; and a landing light in the port leading
/// edge.
pub(super) fn lights(kids: &mut Vec<Generator>, plan: &FusePlan, c: &AirplaneColours) {
    let half = SPAN * 0.5;
    for (side, lamp) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        let x = side * (half - 0.015);
        let [px, py, pz] = mid_chord(plan, x, wing_y(plan, half));
        kids.push(prim(
            sphere(dim(plan.len(0.022)), 3, lamp.clone()),
            [px, py, pz + plan.len(0.02)],
            id_quat(),
        ));
    }
    const LANDING_X: f32 = 1.05;
    kids.push(prim(
        sphere(dim(plan.len(0.026)), 3, c.white.clone()),
        plan.at_off(LANDING_X, wing_y(plan, LANDING_X), WING_LE, -0.02),
        id_quat(),
    ));
}
