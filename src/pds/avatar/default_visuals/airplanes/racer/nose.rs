//! Her nose (#1629 Q-A): the pointed electric cowl is the fuselage's own
//! front; under it the cooling scoop, a flattened sweep along the keel with a
//! dark mouth; and ahead of it the airscrew - a spinner in the trim colour
//! running back to the cowl's face, and two dark blades - on a hidden hub
//! that idles at rest and turns up with her speed.
//!
//! The scoop rides the cockpit's stations, as the prototype's does; the
//! airscrew stands ahead of the firewall, where no stretch reaches.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::generator::Generator;
use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
use crate::pds::types::Fp3;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z, sweep,
    with_shape,
};
use super::super::{FusePlan, RacerColours, dim};
use super::hub;

/// The cooling scoop along her keel (prototype frame): its stations `(z,
/// rise over the belly, radius)` nose to tail, drawn [`SCOOP_WIDTH`] times
/// as wide as it is deep; and its mouth, a dark disc across its front - its
/// station, radius and depth, and how far under the belly it hangs.
const SCOOP: [(f32, f32, f32); 5] = [
    (1.30, 0.010, 0.026),
    (1.20, -0.012, 0.040),
    (1.06, -0.016, 0.044),
    (0.94, -0.006, 0.036),
    (0.84, 0.012, 0.018),
];
const SCOOP_WIDTH: f32 = 1.6;
const MOUTH_Z: f32 = 1.27;
const MOUTH_R: f32 = 0.030;
const MOUTH_H: f32 = 0.012;
const MOUTH_DOWN: f32 = 0.004;

/// Where an ember aura leaves her (prototype frame): the scoop's tail, and
/// how far under the belly there.
const SCOOP_TAIL_Z: f32 = 0.84;
const SCOOP_TAIL_DOWN: f32 = 0.02;

/// Where the airscrew's hub turns: 4.5 cm ahead of the cowl's face; and its
/// spinner's silhouette, `(share of the cowl face's radius, height)` base to
/// tip along its axis, its base that far behind the hub, on the face
/// (prototype frame). The prototype hung the hub 3 cm ahead of the face, and
/// on her fattest fuselage the face's ring - square to her centreline, which
/// climbs aft steeply there - leant its rim 8 mm into the disc her blades
/// sweep: the hub moved forward, the spinner where it was.
const PROP_Z: f32 = 1.545;
const SPINNER: [(f32, f32); 4] = [(1.0, 0.0), (0.92, 0.03), (0.55, 0.08), (0.0, 0.12)];
const SPINNER_BACK: f32 = 0.045;

/// The spinner's tip: the airframe's nose, and her box's (prototype frame).
pub(super) const SPINNER_TIP: f32 = PROP_Z - SPINNER_BACK + SPINNER[SPINNER.len() - 1].1;

/// A blade's tip radius and where it starts - inside the spinner - its chord
/// and thickness, how far its chord narrows toward its tip, and its pitch
/// (degrees); all prototype frame, her full proportion's (#1629 Q-I).
const PROP_R: f32 = 0.38;
const PROP_ROOT: f32 = 0.04;
const BLADE_CHORD: f32 = 0.055;
const BLADE_THICK: f32 = 0.012;
const BLADE_TAPER: f32 = 0.45;
const BLADE_PITCH_DEG: f32 = 20.0;

/// The airscrew's spin (#1629 Q-G): idling at 540 degrees a second, and 36
/// more for each metre a second she travels - so it reaches the runtime's
/// ceiling, four turns a second, at 25 m/s, under her level-flight speed of
/// 26, and holds there.
pub(crate) const PROP_IDLE_DEG_S: f32 = 540.0;
pub(crate) const PROP_GAIN_DEG_PER_M: f32 = 36.0;

/// The airscrew's blades.
const PROP_BLADES: f32 = 2.0;

/// How many times a second a blade passes a point flying level, where her
/// airscrew turns at the ceiling: what her voice's thrum is gated at
/// (`fx::racing_whine`), so what is heard is what is seen.
pub(crate) const BLADE_PASS_HZ: f32 = PROP_BLADES * MAX_SPIN_RATE_DEG / 360.0;

/// The cooling scoop under her nose: a flattened sweep along the keel, its
/// mouth a dark disc across its front.
pub(super) fn scoop(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    // Drawn wider than deep by its node's x scale: the x is divided out,
    // and on her keel it is nothing.
    let points: Vec<([f32; 3], f32)> = SCOOP
        .iter()
        .map(|&(z, rise, r)| (plan.at(0.0, plan.sill(z) + rise, z), plan.len(r)))
        .collect();
    let mut g = prim(sweep(&points, 16, 5, c.body.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([SCOOP_WIDTH, 1.0, 1.0]);
    kids.push(g);
    // A quarter turn about +X lays a cylinder's +Y along her.
    kids.push(prim(
        cylinder(
            plan.len(MOUTH_R),
            dim(plan.len(MOUTH_H)),
            16,
            c.dark.clone(),
        ),
        plan.at(0.0, plan.sill(MOUTH_Z) - MOUTH_DOWN, MOUTH_Z),
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
}

/// Where an ember aura leaves her (drawn frame): under her cooling scoop's
/// tail, where her motor's heat does (`Racer::fx_mount`).
pub(super) fn scoop_tail(plan: &FusePlan) -> [f32; 3] {
    plan.at(0.0, plan.sill(SCOOP_TAIL_Z) - SCOOP_TAIL_DOWN, SCOOP_TAIL_Z)
}

/// The airscrew on a hidden hub ahead of the cowl's face: its spinner, as
/// wide at its base as the face, and two dark blades along `y`, turning
/// about `z` with her speed and idling at rest.
pub(super) fn airscrew(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let (_, axis_y, face_r) = plan.stations().next().expect("the fuselage has stations");
    // A quarter turn about +X lays a lathe's +Y along her.
    let cap: Vec<(f32, f32)> = SPINNER
        .iter()
        .map(|&(share, h)| (plan.len(share * face_r), plan.len(h)))
        .collect();
    let mut parts = vec![prim(
        lathe(&cap, 18, true, c.trim.clone()),
        [0.0, 0.0, -plan.len(SPINNER_BACK)],
        quat_xyzw(quat_x(FRAC_PI_2)),
    )];
    let out = (PROP_ROOT + PROP_R) * 0.5;
    for k in 0..PROP_BLADES as u32 {
        // Each blade built pointing +Y and narrowing toward that tip,
        // turned about the shaft to its own place - the second half round,
        // its tip down.
        let turn = k as f32 * PI;
        let (s, cs) = turn.sin_cos();
        let blade = with_shape(
            cuboid(
                [
                    plan.len(BLADE_CHORD),
                    plan.len(PROP_R - PROP_ROOT),
                    dim(plan.len(BLADE_THICK)),
                ],
                c.dark.clone(),
            ),
            [BLADE_TAPER, 0.0],
            [0.0; 3],
            [0.0; 2],
        );
        parts.push(prim(
            blade,
            [s * plan.len(out), cs * plan.len(out), 0.0],
            quat_xyzw(quat_mul(
                quat_z(-turn),
                quat_y(BLADE_PITCH_DEG.to_radians()),
            )),
        ));
    }
    let mut screw = hub(plan, plan.at(0.0, axis_y, PROP_Z), c);
    screw.children = parts;
    screw.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Speed {
            idle: Fp(PROP_IDLE_DEG_S),
            gain: Fp(PROP_GAIN_DEG_PER_M),
        },
    ));
    kids.push(screw);
}
