//! The fuselage: the one sweep, nose tip to tail cone; the skins laid on
//! it, which are the wrap-round canopy and the chin windows in dark glass,
//! the cabin's long lit pane down each side and the cheat line nose to tail;
//! and the transmission fairing on the roof with its two exhausts.
//!
//! Every skin is a second sweep over the fuselage's OWN stations, a few
//! millimetres proud, trimmed fore and aft by arc length and round the
//! section by angle (`0` is `+X`, `0.25` the crown, `0.5` is `-X`) - each set
//! of bands mirror-symmetric, so the same whichever way round a sweep's frame
//! turns: the utility's skins (#1613).

use bevy::math::{Quat, Vec3};

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::super::common::{
    cylinder, id_quat, prim, quat_mul, quat_x, quat_xyzw, quat_y, spine, sweep, with_cut,
};
use super::super::{CoaxialColours, PodPlan, dim};

/// The facets round the fuselage's section, and round a skin's - a skin's
/// are finer, so its cut edges stay crisp against the fuselage.
const FUSE_RES: u32 = 28;
const SKIN_RES: u32 = 32;

/// A sweep over the fuselage's own stations, `proud` (prototype metres)
/// over it, with the fuselage's section scale.
fn over_fuselage(
    plan: &PodPlan,
    proud: f32,
    res: u32,
    material: SovereignMaterialSettings,
) -> Generator {
    let mut g = prim(
        spine(&plan.pod_sweep(proud), res, material),
        [0.0; 3],
        id_quat(),
    );
    g.transform.scale = Fp3([1.0, plan.section(), 1.0]);
    g
}

/// The fuselage: one sweep, nose tip to the tail cone.
pub(super) fn fuselage(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    kids.push(over_fuselage(plan, 0.0, FUSE_RES, c.body.clone()));
}

/// A skin laid on the fuselage between prototype stations `z0 < z1`, one
/// sweep per angular band.
fn skin(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    (z0, z1): (f32, f32),
    bands: &[(f32, f32)],
    material: &SovereignMaterialSettings,
    proud: f32,
) {
    // The sweep runs nose first, so the forward station is the smaller
    // fraction.
    let (t0, t1) = (plan.along(z1), plan.along(z0));
    for &band in bands {
        let mut g = over_fuselage(plan, proud, SKIN_RES, material.clone());
        g.kind = with_cut(g.kind, [band.0, band.1], [t0, t1], 0.0);
        kids.push(g);
    }
}

/// Every skin, in the prototype's order.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    // The canopy: one wrap round the nose and over the cockpit, its frame at
    // the crown left in the body colour.
    skin(
        kids,
        plan,
        (0.93, 1.42),
        &[(0.02, 0.23), (0.27, 0.48)],
        &c.glass,
        0.006,
    );
    // The chin windows under it, a keel strip left between.
    skin(
        kids,
        plan,
        (1.00, 1.32),
        &[(0.56, 0.68), (0.82, 0.94)],
        &c.glass,
        0.007,
    );
    // The cabin's lit panes, a long one down each side.
    skin(
        kids,
        plan,
        (0.38, 0.86),
        &[(0.05, 0.16), (0.34, 0.45)],
        &c.lamp,
        0.007,
    );
    // The cheat line, nose to tail just under each flank's widest - the
    // seed's accent, the fleet's identity slot.
    skin(
        kids,
        plan,
        (-1.40, 1.32),
        &[(0.975, 0.995), (0.505, 0.525)],
        &c.stripe,
        0.004,
    );
}

/// The transmission fairing on the roof (prototype frame): its stations
/// `(z, radius)` nose to tail, each centred [`FAIRING_SINK`] under the
/// crown, its section drawn [`FAIRING_SQUASH`] as tall as it is wide.
pub(super) const FAIRING: [(f32, f32); 8] = [
    (0.98, 0.035),
    (0.86, 0.095),
    (0.70, 0.14),
    (0.50, 0.155),
    (0.30, 0.145),
    (0.12, 0.11),
    (-0.02, 0.06),
    (-0.10, 0.025),
];
pub(super) const FAIRING_SINK: f32 = 0.035;
pub(super) const FAIRING_SQUASH: f32 = 0.85;

/// The fairing: a low hump along the roof that the mast rises out of.
pub(super) fn fairing(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    let pts: Vec<([f32; 3], f32)> = FAIRING
        .iter()
        .map(|&(z, r)| {
            let [x, y, z] = plan.at(0.0, plan.crown(z) - FAIRING_SINK, z);
            ([x, y / FAIRING_SQUASH, z], plan.len(r))
        })
        .collect();
    let mut g = prim(sweep(&pts, 20, 6, c.body.clone()), [0.0; 3], id_quat());
    g.transform.scale = Fp3([1.0, FAIRING_SQUASH, 1.0]);
    kids.push(g);
}

/// The exhausts (prototype frame): each one's `x` either side, station, rise
/// over the crown, radius and length, and its turn - raked 80 degrees from
/// upright to point aft, then yawed 25 degrees outboard.
const EXHAUST_X: f32 = 0.10;
const EXHAUST_Z: f32 = 0.16;
const EXHAUST_RISE: f32 = 0.02;
const EXHAUST_R: f32 = 0.030;
const EXHAUST_LEN: f32 = 0.10;
const EXHAUST_RAKE_DEG: f32 = -80.0;
const EXHAUST_YAW_DEG: f32 = 25.0;

/// An exhaust's turn on the `side` (`1` port, `-1` starboard): its `+Y`
/// laid aft by the rake, then yawed outboard.
fn exhaust_turn(side: f32) -> [f32; 4] {
    quat_mul(
        quat_y((-side * EXHAUST_YAW_DEG).to_radians()),
        quat_x(EXHAUST_RAKE_DEG.to_radians()),
    )
}

/// The two exhausts at the fairing's tail, starboard (`-X`) first, each
/// pointing aft and outboard.
pub(super) fn exhausts(kids: &mut Vec<Generator>, plan: &PodPlan, c: &CoaxialColours) {
    for side in [-1.0f32, 1.0] {
        kids.push(prim(
            cylinder(
                dim(plan.len(EXHAUST_R)),
                plan.len(EXHAUST_LEN),
                12,
                c.dark.clone(),
            ),
            plan.at(
                side * EXHAUST_X,
                plan.crown(EXHAUST_Z) + EXHAUST_RISE,
                EXHAUST_Z,
            ),
            quat_xyzw(exhaust_turn(side)),
        ));
    }
}

/// The port exhaust's mouth, in the drawn frame: its `+Y` end, the
/// exhaust's own half-length along its turned axis from its centre - where
/// an exhaust aura leaves her (`Coaxial::fx_mount`). Rigid off the
/// exhaust's station: the mouth points aft past the cabin's rear, where the
/// boom stretch starts, and the exhaust is drawn whole.
pub(super) fn exhaust_mouth(plan: &PodPlan) -> [f32; 3] {
    let out = Quat::from_array(exhaust_turn(1.0)) * Vec3::Y * (EXHAUST_LEN * 0.5);
    plan.at_off(
        EXHAUST_X + out.x,
        plan.crown(EXHAUST_Z) + EXHAUST_RISE + out.y,
        EXHAUST_Z,
        out.z,
    )
}
