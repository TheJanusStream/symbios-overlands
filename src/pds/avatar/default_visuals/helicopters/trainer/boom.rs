//! The truss boom and what hangs on it: three longerons and a Warren
//! zig-zag on each face, the tail rotor's gearbox at its end and the guard
//! stirrup under the tail rotor, the elevator two thirds aft with a nav
//! light on each tip, the white light at the boom's end and the beacon
//! behind the mast.
//!
//! Everything here stands on boom stations, so the blueprint's boom stretch
//! carries it ([`PodPlan::at`]); a part fixed off the boom's end rides it
//! rigidly ([`PodPlan::at_off`]).
//!
//! # Each face's diagonals are one sweep
//!
//! A uniform Catmull-Rom through a zig-zag's corners, sampled twice a
//! segment, draws each strut through its corners and its middle - and puts
//! that middle on the strut's chord whenever `P0 + P3 = P1 + P2` round it,
//! since the middle is `0.5625 (P1 + P2) - 0.0625 (P0 + P3)`. So a face's
//! six struts are one sweep at a sixth of the bytes of a rod each. Two
//! things keep them straight. The truss TAPERS, so an inner strut's middle
//! stands `0.125 h |L' - U'|` off its chord - 5.7 mm on the side faces at
//! the nominal machine, under the strut's own 8 mm radius. And a sweep's
//! ends are mirrored, which bowed each face's first strut 4 cm off its
//! chord: so each sweep runs one corner further at each end, the fourth
//! corner of the parallelogram its end struts make, and is trimmed back to
//! its real corners by arc length (`her_truss_struts_are_drawn_straight`).

use std::f32::consts::FRAC_PI_2;

use bevy::math::Vec3;

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::super::super::common::{
    cuboid, cylinder, id_quat, prim, quat_xyzw, quat_z, sphere, sweep, with_cut,
};
use super::super::{PodPlan, TrainerColours, dim};
use super::PROFILE;

/// The truss's bays.
pub(super) const BAYS: usize = 6;

/// The longerons' and the struts' radii (m, prototype frame). A strut is
/// floored at [`MIN_DIM`](super::super::MIN_DIM) - the prototype drew it at
/// 8 mm, under the sanitiser's 1 cm floor.
pub(super) const LONGERON_R: f32 = 0.013;
const STRUT_R: f32 = 0.008;

/// Where a lead corner is trimmed: three quarters of the way along it, which
/// is between its middle sample and the real corner whatever the lead's
/// bow - so the trim keeps exactly the real corners' stations.
const LEAD_TRIM: f32 = 0.75;

/// The truss at a fraction `t` of the boom, root to end: its lower longerons'
/// half-spread and height, its upper longeron's height, and the station - a
/// prototype `z`, not yet stretched.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub(super) lower_x: f32,
    pub(super) lower_y: f32,
    pub(super) upper_y: f32,
    pub(super) z: f32,
}

/// The truss's frame at `t`: wide and deep at the engine bay, narrowing to
/// the tail rotor's gearbox, its lower longerons rising to meet it.
pub(super) fn frame_at(t: f32) -> Frame {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    Frame {
        lower_x: lerp(0.115, 0.026),
        lower_y: lerp(0.300, 0.505),
        upper_y: lerp(0.640, 0.585),
        z: lerp(PROFILE.boom_root, PROFILE.boom_end),
    }
}

/// The truss's frames, root to end.
fn frames() -> [Frame; BAYS + 1] {
    std::array::from_fn(|k| frame_at(k as f32 / BAYS as f32))
}

/// The tail rotor gearbox's height: halfway up the truss's end.
pub(super) fn gear_y() -> f32 {
    let end = frame_at(1.0);
    (end.lower_y + end.upper_y) * 0.5
}

/// One face's zig-zag: a sweep through `corners` (drawn frame), run on one
/// corner past each end and trimmed back - see the module header.
fn zigzag(corners: &[[f32; 3]], radius: f32, material: SovereignMaterialSettings) -> Generator {
    let v: Vec<Vec3> = corners.iter().map(|&c| Vec3::from(c)).collect();
    let n = v.len();
    let lead_in = v[0] + v[1] - v[2];
    let lead_out = v[n - 1] + v[n - 2] - v[n - 3];
    let mut path = vec![lead_in];
    path.extend(&v);
    path.push(lead_out);
    let total: f32 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
    let trim = [
        LEAD_TRIM * lead_in.distance(v[0]) / total,
        1.0 - LEAD_TRIM * lead_out.distance(v[n - 1]) / total,
    ];
    let pts: Vec<([f32; 3], f32)> = path.iter().map(|p| (p.to_array(), radius)).collect();
    prim(
        with_cut(sweep(&pts, 6, 2, material), [0.0, 1.0], trim, 0.0),
        [0.0; 3],
        id_quat(),
    )
}

/// The truss: the two lower longerons and the upper one, then a zig-zag
/// down each side face and across the bottom - in the body colour (#1621
/// Q-C), so she reads in her livery at 12 m.
pub(super) fn truss(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let frames = frames();
    let r = plan.len(LONGERON_R);
    let lower = |f: &Frame, side: f32| plan.at(side * f.lower_x, f.lower_y, f.z);
    let upper = |f: &Frame| plan.at(0.0, f.upper_y, f.z);
    for side in [-1.0f32, 1.0] {
        let pts: Vec<([f32; 3], f32)> = frames.iter().map(|f| (lower(f, side), r)).collect();
        kids.push(prim(sweep(&pts, 8, 2, c.body.clone()), [0.0; 3], id_quat()));
    }
    let pts: Vec<([f32; 3], f32)> = frames.iter().map(|f| (upper(f), r)).collect();
    kids.push(prim(sweep(&pts, 8, 2, c.body.clone()), [0.0; 3], id_quat()));
    // Each face alternates its two longerons, the first corner on the first.
    let strut = dim(plan.len(STRUT_R));
    let face = |a: &dyn Fn(&Frame) -> [f32; 3], b: &dyn Fn(&Frame) -> [f32; 3]| {
        frames
            .iter()
            .enumerate()
            .map(|(k, f)| if k.is_multiple_of(2) { a(f) } else { b(f) })
            .collect::<Vec<_>>()
    };
    for side in [-1.0f32, 1.0] {
        let corners = face(&|f| lower(f, side), &upper);
        kids.push(zigzag(&corners, strut, c.body.clone()));
    }
    let corners = face(&|f| lower(f, -1.0), &|f| lower(f, 1.0));
    kids.push(zigzag(&corners, strut, c.body.clone()));
}

/// The tail rotor's gearbox at the boom's end, its shaft across her.
pub(super) fn gearbox(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    kids.push(prim(
        cylinder(plan.len(0.030), plan.len(0.07), 12, c.metal.clone()),
        plan.at(0.0, gear_y(), PROFILE.boom_end),
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
}

/// The guard's tube radius (m, prototype frame): over the sanitiser's floor
/// at every blueprint, so it is the same share of her in all of them and
/// her box's tail, which it sets, can be stated exactly. The prototype drew
/// it at 1 cm.
const STIRRUP_R: f32 = 0.0125;

/// The guard: a stirrup from the bottom zig-zag's last corner but one, down
/// under the tail rotor's disc - 6 cm below its lowest blade - and back up
/// into the gearbox from behind. Both ends held; the disc clear of it to the
/// side. Fixed off the boom's end, so the stretch carries it whole, and its
/// aft curve - which sets her box's tail ([`PROFILE`]'s `tail_reach`) - is
/// the same at every boom.
pub(super) fn stirrup(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    // The bottom face's corner at the last frame but one: on the +X
    // longeron, the zig-zag starting on the -X one at the root.
    let k = BAYS - 1;
    let f = frame_at(k as f32 / BAYS as f32);
    let side = if k.is_multiple_of(2) { -1.0 } else { 1.0 };
    let (gy, end) = (gear_y(), PROFILE.boom_end);
    let r = dim(plan.len(STIRRUP_R));
    let pts = [
        (plan.at(side * f.lower_x, f.lower_y, f.z), r),
        (plan.at_off(side * 0.02, gy - 0.22, end, 0.14), r),
        (plan.at_off(0.0, gy - 0.33, end, -0.02), r),
        (plan.at_off(0.0, gy - 0.29, end, -0.20), r),
        (plan.at_off(0.0, gy - 0.12, end, -0.30), r),
        (plan.at_off(0.0, gy - 0.015, end, -0.025), r),
    ];
    kids.push(prim(sweep(&pts, 8, 5, c.body.clone()), [0.0; 3], id_quat()));
}

/// How far along the truss the elevator stands.
const ELEVATOR_AT: f32 = 0.62;
const ELEVATOR_HALF_SPAN: f32 = 0.22;

/// The elevator across the lower longerons, an end plate on each tip and a
/// nav light on its outboard face: red to port - her left, `+X`, as she is
/// drawn nose `+Z` - and green to starboard.
pub(super) fn elevator(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let f = frame_at(ELEVATOR_AT);
    kids.push(prim(
        cuboid(
            [
                plan.len(2.0 * ELEVATOR_HALF_SPAN),
                dim(plan.len(0.012)),
                plan.len(0.085),
            ],
            c.body.clone(),
        ),
        plan.at(0.0, f.lower_y + 0.005, f.z),
        id_quat(),
    ));
    for (side, light) in [(1.0f32, &c.port), (-1.0, &c.starboard)] {
        kids.push(prim(
            cuboid(
                [dim(plan.len(0.010)), plan.len(0.06), plan.len(0.07)],
                c.body.clone(),
            ),
            plan.at(side * ELEVATOR_HALF_SPAN, f.lower_y + 0.02, f.z),
            id_quat(),
        ));
        kids.push(prim(
            sphere(dim(plan.len(0.012)), 3, light.clone()),
            plan.at(side * 0.228, f.lower_y + 0.055, f.z),
            id_quat(),
        ));
    }
}

/// The white light at the boom's end, on the upper longeron's tip, and the
/// red beacon on the upper longeron behind the mast.
pub(super) fn lights(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let end = frame_at(1.0);
    kids.push(prim(
        sphere(dim(plan.len(0.013)), 3, c.white.clone()),
        plan.at_off(0.0, end.upper_y + 0.02, end.z, -0.01),
        id_quat(),
    ));
    let first = frame_at(1.0 / BAYS as f32);
    kids.push(prim(
        sphere(dim(plan.len(0.016)), 3, c.beacon.clone()),
        plan.at(0.0, first.upper_y + 0.02, first.z),
        id_quat(),
    ));
}
