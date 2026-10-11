//! The tail: the boom and the identity stripes along it, the raked fin and
//! the ventral fin, the stabiliser with its end plates and nav lights, and
//! the anti-collision beacon on the fin's top.
//!
//! Everything here stands on boom stations, so the blueprint's boom stretch
//! carries it ([`PodPlan::at`]); sizes are the prototype's, scaled.

use crate::pds::generator::Generator;

use super::super::super::common::{
    cuboid, id_quat, prim, quat_x, quat_xyzw, sphere, spine, with_cut, with_shape,
};
use super::super::{HelicopterColours, PodPlan, dim};

/// The boom, `(station, centreline height, radius)` from inside the pod's
/// rear to the fin.
const BOOM: [(f32, f32, f32); 4] = [
    (-0.50, 0.64, 0.080),
    (-0.95, 0.66, 0.065),
    (-1.45, 0.69, 0.048),
    (-1.66, 0.70, 0.040),
];

/// How far each stripe stands proud of the boom, and the angular band it
/// keeps: centred on a flank (`0.5` round a sweep's section from its
/// `0`), a fifth of the way round.
const STRIPE_PROUD: f32 = 0.003;
const STRIPE_BAND: [f32; 2] = [0.40, 0.60];

pub(super) fn boom(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let pts: Vec<([f32; 3], f32)> = BOOM
        .iter()
        .map(|&(z, y, r)| (plan.at(0.0, y, z), plan.len(r)))
        .collect();
    kids.push(prim(spine(&pts, 18, c.body.clone()), [0.0; 3], id_quat()));
    // The identity stripes, from the boom's first station aft. Drawn nose to
    // tail, a sweep's angle `0.5` stands on `+X`; drawn tail to nose, on
    // `-X` - the frame is transported along the path, so reversing the path
    // mirrors it. One band, both flanks.
    let stripe: Vec<([f32; 3], f32)> = BOOM[1..]
        .iter()
        .map(|&(z, y, r)| (plan.at(0.0, y, z), plan.len(r + STRIPE_PROUD)))
        .collect();
    let reversed: Vec<([f32; 3], f32)> = stripe.iter().rev().copied().collect();
    for path in [stripe, reversed] {
        kids.push(prim(
            with_cut(
                spine(&path, 18, c.stripe.clone()),
                STRIPE_BAND,
                [0.0, 1.0],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
}

/// The fin's station and height, its span, and its rake aft (degrees about
/// `+X`; negative tips its top aft).
pub(super) const FIN: (f32, f32) = (-1.66, 0.86);
const FIN_SPAN: f32 = 0.34;
const FIN_RAKE_DEG: f32 = -18.0;

pub(super) fn fins(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    // The vertical fin, raked aft, its chord tapering to 55 % at the top.
    kids.push(prim(
        with_shape(
            cuboid(
                [dim(plan.len(0.018)), plan.len(FIN_SPAN), plan.len(0.16)],
                c.body.clone(),
            ),
            [0.0, 0.45],
            [0.0; 3],
            [0.0; 2],
        ),
        plan.at(0.0, FIN.1, FIN.0),
        quat_xyzw(quat_x(FIN_RAKE_DEG.to_radians())),
    ));
    // The short ventral fin under the boom's end, raked aft the other way,
    // fixed to the fin's station.
    kids.push(prim(
        cuboid(
            [dim(plan.len(0.016)), plan.len(0.14), plan.len(0.10)],
            c.body.clone(),
        ),
        plan.at_off(0.0, 0.60, FIN.0, 0.02),
        quat_xyzw(quat_x(20f32.to_radians())),
    ));
}

/// The stabiliser's station and half-span.
const STABILISER_Z: f32 = -1.28;
const STABILISER_HALF_SPAN: f32 = 0.23;

pub(super) fn stabiliser(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    kids.push(prim(
        cuboid(
            [
                plan.len(2.0 * STABILISER_HALF_SPAN),
                dim(plan.len(0.014)),
                plan.len(0.10),
            ],
            c.body.clone(),
        ),
        plan.at(0.0, 0.67, STABILISER_Z),
        id_quat(),
    ));
    // An end plate at each tip, in the scheme's second hue, and a nav light
    // on its outboard face: red to port - her left, `+X`, as she is drawn
    // nose `+Z` - and green to starboard, as on every aircraft. Both are
    // fixed to the stabiliser's station.
    for (side, light) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        kids.push(prim(
            cuboid(
                [dim(plan.len(0.012)), plan.len(0.08), plan.len(0.09)],
                c.lower.clone(),
            ),
            plan.at_off(side * STABILISER_HALF_SPAN, 0.69, STABILISER_Z, -0.01),
            id_quat(),
        ));
        kids.push(prim(
            sphere(dim(plan.len(0.012)), 3, light.clone()),
            plan.at_off(side * 0.237, 0.72, STABILISER_Z, 0.01),
            id_quat(),
        ));
    }
}

/// The beacon's radius, and how far its centre stands over the fin's top
/// face along the fin's raked span: half its radius, so it sits down into
/// the fin rather than balancing on it.
const BEACON_R: f32 = 0.016;
const BEACON_SEAT: f32 = BEACON_R * 0.5;

/// The red anti-collision beacon on the fin's top, seated on the fin's own
/// raked axis and fixed to its station - so it rides the fin wherever the
/// boom stretch takes it. (The prototype hung it at a station of its own,
/// 6 cm aft of the fin's, and at the longest boom the stretch drew it off.)
pub(super) fn beacon(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    // Up the fin's span, turned by its rake: a turn about +X carries +Y
    // toward +Z by its sine, so a negative rake carries it aft.
    let rake = FIN_RAKE_DEG.to_radians();
    let up = FIN_SPAN * 0.5 + BEACON_SEAT;
    kids.push(prim(
        sphere(dim(plan.len(BEACON_R)), 3, c.beacon.clone()),
        plan.at_off(0.0, FIN.1 + up * rake.cos(), FIN.0, up * rake.sin()),
        id_quat(),
    ));
}
