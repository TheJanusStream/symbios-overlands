//! The wing, low through her fuselage under the cabin's floor, and the nav
//! lights on its tips.
//!
//! One flattened sweep tip to tip, as every airplane's wing is: its section
//! an ellipse [`THICK`] of its chord, its stations spaced evenly along the
//! span and closing in at the tips, so it ends rounded in plan (#1616). The
//! chord tapers to half at the tip about a quarter-chord line straight
//! across, and the wing rises with its dihedral from the root.
//!
//! Hung RIGIDLY off its own leading edge, which stands ahead of the
//! cockpit's back wall at every span station: a longer cabin carries her
//! tail aft under it and leaves the wing where it is.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{id_quat, prim, sphere, sweep};
use super::super::{FusePlan, TwinColours, dim};
use super::dressing::{TANK, TANK_X};

/// The wing's chord at the root and its share of it at the tip, its
/// quarter-chord line's station, its half-span - a 4.33 m span, the Beech
/// 18's 1.36 of her length - its thickness over chord, its dihedral
/// (degrees) and its mid-plane's height at the root, through the fuselage
/// under the cabin's floor (prototype frame).
const ROOT_CHORD: f32 = 0.80;
const TIP_SHARE: f32 = 0.50;
const QUARTER_Z: f32 = 0.75;
pub(super) const HALF_SPAN: f32 = 2.165;
const THICK: f32 = 0.13;
const DIHEDRAL_DEG: f32 = 5.0;
const ROOT_Y: f32 = 0.38;

/// The wing's stations, `(share of the half-span, share of the taper's
/// chord)` root to tip - evenly spaced, closing in over the last tenth.
/// Eight a side, the middle shared: fifteen, inside the sanitiser's sixteen
/// ([`MAX_SWEEP_POINTS`](crate::pds::sanitize::limits::MAX_SWEEP_POINTS)).
const STATIONS: [(f32, f32); 8] = [
    (0.0, 1.0),
    (0.22, 1.0),
    (0.44, 1.0),
    (0.64, 1.0),
    (0.80, 1.0),
    (0.91, 0.90),
    (0.965, 0.62),
    (1.0, 0.18),
];

/// How far in from the wingtip a nav light's middle stands, and a light's
/// radius (prototype frame).
const NAV_IN: f32 = 0.02;
const NAV_R: f32 = 0.024;

/// The wing's chord at span station `x`: tapering to [`TIP_SHARE`].
pub(super) fn chord_at(x: f32) -> f32 {
    let s = (x.abs() / HALF_SPAN).min(1.0);
    ROOT_CHORD * (1.0 - (1.0 - TIP_SHARE) * s)
}

/// The wing's mid-plane height at span station `x`, rising with its
/// dihedral (prototype frame).
pub(super) fn wing_y(x: f32) -> f32 {
    ROOT_Y + x.abs() * DIHEDRAL_DEG.to_radians().tan()
}

/// The wing's leading edge's station at span station `x`: a quarter of the
/// chord ahead of the quarter-chord line (prototype frame).
pub(super) fn wing_le(x: f32) -> f32 {
    QUARTER_Z + 0.25 * chord_at(x)
}

/// A point `y` high at span station `x`, `share` of the chord back from the
/// leading edge, drawn - rigid off the leading edge's station.
pub(super) fn on_chord(plan: &FusePlan, x: f32, y: f32, share: f32) -> [f32; 3] {
    plan.at_off(x, y, wing_le(x), -share * chord_at(x))
}

/// Where her lift is centred (drawn `z`): the wing's quarter chord at its
/// root - the station her centre of mass is set over.
pub(super) fn quarter_chord(plan: &FusePlan) -> f32 {
    on_chord(plan, 0.0, 0.0, 0.25)[2]
}

/// The wing: one flattened sweep tip to tip, low through the fuselage.
pub(super) fn wing(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let point = |x: f32, chord_share: f32| {
        let [px, py, pz] = on_chord(plan, x, wing_y(x), 0.5);
        (
            [px, py / THICK, pz],
            plan.len(chord_at(x) * chord_share * 0.5),
        )
    };
    let rows: Vec<(f32, f32)> = STATIONS.iter().map(|&(s, c)| (s * HALF_SPAN, c)).collect();
    let mut points: Vec<([f32; 3], f32)> = rows
        .iter()
        .skip(1)
        .rev()
        .map(|&(x, c)| point(-x, c))
        .collect();
    points.extend(rows.iter().map(|&(x, c)| point(x, c)));
    let mut g = prim(sweep(&points, 24, 4, c.body.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, THICK, 1.0]);
    kids.push(g);
}

/// Her nav lights: red on the port tip (her left, `+X`), green on the
/// starboard - on an ornate machine's tip tanks, outboard, where the tanks
/// would bury them.
pub(super) fn lights(
    kids: &mut Vec<Generator>,
    plan: &FusePlan,
    c: &TwinColours,
    ornateness: OrnatenessTier,
) {
    let out = if ornateness == OrnatenessTier::Ornate {
        let widest = TANK.iter().map(|&(_, r)| r).fold(0.0, f32::max);
        TANK_X + widest - 0.01
    } else {
        HALF_SPAN - NAV_IN
    };
    for (side, lamp) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        let [_, y, z] = on_chord(plan, HALF_SPAN, wing_y(HALF_SPAN), 0.5);
        kids.push(prim(
            sphere(dim(plan.len(NAV_R)), 3, lamp.clone()),
            [plan.len(side * out), y, z],
            id_quat(),
        ));
    }
}
