//! The wing, low through her fuselage under the cockpit, and the nav lights
//! on its tips - on an ornate machine's winglets.
//!
//! One flattened sweep tip to tip, as every airplane's wing is: its section
//! an ellipse [`THICK`] of its chord, its stations spaced evenly along the
//! span and closing in at the tips, so it ends rounded in plan (#1616). The
//! chord tapers to half at the tip about a quarter-chord line straight
//! across, and the wing rises with its dihedral from the root. Her full
//! proportion's (#1629 Q-I): 3.24 m across where a true racer's is 2.9.
//!
//! Hung RIGIDLY off the firewall's station ([`on_wing`]): her wing's root
//! lies under the cockpit's run, and the prototype draws it at its own
//! stations whatever the cockpit does - a longer cockpit carries her tail
//! aft over it and leaves the wing where it is.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{id_quat, prim, sphere, sweep};
use super::super::{FusePlan, RacerColours, dim};
use super::PROFILE;
use super::dressing::{WINGLET_LIGHT, WINGLET_X};

/// The wing's chord at the root and its share of it at the tip, its
/// quarter-chord line's station, its half-span, its thickness over chord,
/// its dihedral (degrees) and its mid-plane's height at the root, 2 cm over
/// her belly (prototype frame).
const ROOT_CHORD: f32 = 0.78;
const TIP_SHARE: f32 = 0.50;
const QUARTER_Z: f32 = 0.62;
pub(super) const HALF_SPAN: f32 = 1.62;
const THICK: f32 = 0.12;
const DIHEDRAL_DEG: f32 = 5.0;
const ROOT_Y: f32 = 0.29;

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
const NAV_R: f32 = 0.022;

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

/// The middle of the wing's chord at span station `x` (prototype frame).
pub(super) fn wing_mid(x: f32) -> f32 {
    wing_le(x) - 0.5 * chord_at(x)
}

/// A prototype point on or off the wing, drawn - its station read RIGIDLY
/// off the firewall's, which no stretch moves, so the wing stands where the
/// prototype draws it whatever her cockpit's run.
pub(super) fn on_wing(plan: &FusePlan, x: f32, y: f32, z: f32) -> [f32; 3] {
    plan.at_off(x, y, PROFILE.firewall, z - PROFILE.firewall)
}

/// Where her lift is centred (drawn `z`): the wing's quarter chord at its
/// root - the station her centre of mass is set over.
pub(super) fn quarter_chord(plan: &FusePlan) -> f32 {
    on_wing(plan, 0.0, 0.0, QUARTER_Z)[2]
}

/// The wing: one flattened sweep tip to tip, low through the fuselage.
pub(super) fn wing(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let point = |x: f32, chord_share: f32| {
        let [px, py, pz] = on_wing(plan, x, wing_y(x), wing_mid(x));
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
/// starboard - on an ornate machine's winglets, on their outer faces half
/// way up, where a winglet's root would bury them.
pub(super) fn lights(
    kids: &mut Vec<Generator>,
    plan: &FusePlan,
    c: &RacerColours,
    ornateness: OrnatenessTier,
) {
    let (y, z) = (wing_y(HALF_SPAN), wing_mid(HALF_SPAN));
    for (side, lamp) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        let at = if ornateness == OrnatenessTier::Ornate {
            let [out, rise, dz] = WINGLET_LIGHT;
            on_wing(plan, side * (WINGLET_X + out), y + rise, z + dz)
        } else {
            on_wing(plan, side * (HALF_SPAN - NAV_IN), y, z)
        };
        kids.push(prim(
            sphere(dim(plan.len(NAV_R)), 3, lamp.clone()),
            at,
            id_quat(),
        ));
    }
}
