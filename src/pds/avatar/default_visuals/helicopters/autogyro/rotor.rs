//! Her rotor (#1624 Q-C): a free three-blade rotor on a tripod pylon over
//! the front cockpit - two legs from the crown's shoulders at the firewall
//! and one from between the cockpits - its housing at the pylon's top and
//! the hub on it, its axis leaning aft, turning with her speed as a free
//! rotor does: slowly at rest.

use std::f32::consts::TAU;

use bevy::math::{Quat, Vec3};

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::{Fp, Spin, SpinTerm};
use crate::world_builder::primitive_outline;

use super::super::super::common::{
    cuboid, cylinder, id_quat, prim, quat_mul, quat_x, quat_xyzw, quat_y, rod,
};
use super::super::{AutogyroColours, PodPlan, dim};
use super::{PYLON_FOOT_Z, PYLON_Z, hub, stance};

/// How far the housing's base stands over the crown at the pylon's station,
/// and how far the rotor's axis leans aft of her up (degrees) - eighteen
/// standing tail-down, the autogyro's tilted disc (prototype frame).
const HEAD_RISE: f32 = 0.60;
const TILT_DEG: f32 = 6.0;

/// The housing, a short cylinder along the rotor's axis, and the head on
/// its top that the blades turn on (prototype frame).
const HOUSING_R: f32 = 0.05;
const HOUSING_H: f32 = 0.12;
const HEAD_R: f32 = 0.06;
const HEAD_H: f32 = 0.04;

/// The blades: how many, the tips' radius, how far from the axis each root
/// stands - 2 cm into the head - and each one's thickness, chord and pitch
/// (degrees) - prototype frame.
pub(crate) const BLADES: u32 = 3;
const ROTOR_R: f32 = 1.95;
const BLADE_ROOT: f32 = 0.04;
const BLADE_THICK: f32 = 0.014;
const BLADE_CHORD: f32 = 0.14;
const BLADE_PITCH_DEG: f32 = 3.0;

/// The pylon's legs (prototype frame): their radius; how far under the
/// crown each foot stands and how far in from the flank the front feet
/// stand, as a share of it; the rear leg's station; and where the legs
/// meet under the housing - over its base, a centimetre either side of the
/// axis and fore and aft of it.
const LEG_R: f32 = 0.018;
const LEG_SINK: f32 = 0.03;
const LEG_SPREAD: f32 = 0.85;
const REAR_LEG_Z: f32 = 0.58;
const LEG_TOP_RISE: f32 = 0.02;
const LEG_TOP_X: f32 = 0.025;
const FRONT_TOP_DZ: f32 = 0.01;
const REAR_TOP_DZ: f32 = -0.03;

/// The rotor's spin (#1624 Q-C): a free rotor, turning at 120 degrees a
/// second at rest and 90 more for each metre a second she travels - about
/// 3.3 turns a second at her top speed, under the runtime's ceiling.
pub(crate) const ROTOR_IDLE_DEG_S: f32 = 120.0;
pub(crate) const ROTOR_GAIN_DEG_PER_M: f32 = 90.0;

/// The housing's base (prototype frame): [`HEAD_RISE`] over the crown at
/// the pylon's station.
fn head_base(plan: &PodPlan) -> f32 {
    plan.crown(PYLON_Z) + HEAD_RISE
}

/// The rotor's lean: [`TILT_DEG`] aft, about her lateral axis.
pub(super) fn lean() -> Quat {
    Quat::from_rotation_x(-TILT_DEG.to_radians())
}

/// A point `up` (prototype metres) along the rotor's leaning axis from the
/// housing's base, drawn level - rigid off the pylon's station.
fn up_axis(plan: &PodPlan, up: f32) -> [f32; 3] {
    let off = lean() * Vec3::new(0.0, up, 0.0);
    plan.at_off(0.0, head_base(plan) + off.y, PYLON_Z, off.z)
}

/// The housing's centre, drawn level: half its height up the axis.
pub(super) fn housing_at(plan: &PodPlan) -> [f32; 3] {
    up_axis(plan, HOUSING_H * 0.5)
}

/// The rotor's hub, drawn level: at the housing's top.
fn hub_at(plan: &PodPlan) -> [f32; 3] {
    up_axis(plan, HOUSING_H)
}

/// The housing at the pylon's top, along the rotor's leaning axis.
pub(super) fn housing(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    kids.push(prim(
        cylinder(
            plan.len(HOUSING_R),
            plan.len(HOUSING_H),
            14,
            c.metal.clone(),
        ),
        housing_at(plan),
        quat_xyzw(lean().to_array()),
    ));
}

/// The port (`side` 1) or starboard (`-1`) front leg's two ends, drawn
/// level: its foot on the crown's shoulder at the firewall, its top under
/// the housing.
pub(super) fn front_leg(plan: &PodPlan, side: f32) -> ([f32; 3], [f32; 3]) {
    let z = PYLON_FOOT_Z;
    let y = plan.crown(z) - LEG_SINK;
    (
        plan.at(side * plan.flank(z, y) * LEG_SPREAD, y, z),
        plan.at_off(
            side * LEG_TOP_X,
            head_base(plan) + LEG_TOP_RISE,
            PYLON_Z,
            FRONT_TOP_DZ,
        ),
    )
}

/// The tripod: the two front legs, and the rear leg from the crown between
/// the cockpits.
pub(super) fn pylon(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let r = dim(plan.len(LEG_R));
    for side in [-1.0f32, 1.0] {
        let (foot, top) = front_leg(plan, side);
        kids.push(rod(foot, top, r, c.metal.clone()));
    }
    let z = REAR_LEG_Z;
    kids.push(rod(
        plan.at(0.0, plan.crown(z) - LEG_SINK, z),
        plan.at_off(0.0, head_base(plan) + LEG_TOP_RISE, PYLON_Z, REAR_TOP_DZ),
        r,
        c.metal.clone(),
    ));
}

/// The rotor's head: the short cylinder the blades turn on, at its hub.
fn head(plan: &PodPlan, material: SovereignMaterialSettings) -> Generator {
    prim(
        cylinder(plan.len(HEAD_R), plan.len(HEAD_H), 16, material),
        [0.0; 3],
        id_quat(),
    )
}

/// The rotor: [`BLADES`] long blades on the head, on a hidden hub at the
/// housing's top that leans with the axis and turns about it with her
/// speed.
pub(super) fn rotor(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let mut parts = vec![head(plan, c.metal.clone())];
    // Each blade's middle halfway out from its root to its tip.
    let out = BLADE_ROOT + (ROTOR_R - BLADE_ROOT) * 0.5;
    for k in 0..BLADES {
        let th = k as f32 * TAU / BLADES as f32;
        let along = Quat::from_rotation_y(th) * Vec3::X;
        parts.push(prim(
            cuboid(
                [
                    plan.len(ROTOR_R - BLADE_ROOT),
                    dim(plan.len(BLADE_THICK)),
                    plan.len(BLADE_CHORD),
                ],
                c.wings.clone(),
            ),
            [along.x * plan.len(out), 0.0, along.z * plan.len(out)],
            quat_xyzw(quat_mul(quat_y(th), quat_x(BLADE_PITCH_DEG.to_radians()))),
        ));
    }
    let mut free = hub(plan, hub_at(plan), c);
    free.transform.rotation = quat_xyzw(lean().to_array());
    free.children = parts;
    free.spin = Some(Spin::about(
        [0.0, 1.0, 0.0],
        SpinTerm::Speed {
            idle: Fp(ROTOR_IDLE_DEG_S),
            gain: Fp(ROTOR_GAIN_DEG_PER_M),
        },
    ));
    kids.push(free);
}

/// The top of her rotor head, standing (drawn frame): the highest of the
/// mesher's own vertices of the head, through the hub as she stands -
/// leaning aft eighteen degrees, the rim ahead of its axis stands highest.
/// What her box is as tall as.
pub(super) fn head_top(plan: &PodPlan) -> f32 {
    let at = Vec3::from(stance::stood(plan, hub_at(plan)));
    let turn = stance::turn() * lean();
    primitive_outline(&head(plan, SovereignMaterialSettings::default()).kind)
        .into_iter()
        .map(|p| (at + turn * p).y)
        .fold(f32::MIN, f32::max)
}
