//! The pod: the cabin sweep, the skins laid on it - the two-tone band, the
//! windscreen and chin glass, the lit door panes - and the engine fairing
//! and exhaust stack on its roof.
//!
//! # Skins are laid on the pod's own stations
//!
//! Every skin is a second sweep over the pod's OWN stations, a few
//! millimetres proud, with the pod's own section scale - trimmed fore and
//! aft by arc length (`profile_cut`, [`PodPlan::along`]) and round the
//! section by angle (`path_cut`). Laid on stations of its own it followed a
//! different Catmull-Rom from the pod's, and the pod poked through between
//! them as smears (#1613). Each angular band is symmetric about the crown
//! (`0.25`) or the keel (`0.75`), so it is the same whichever way round a
//! sweep's frame turns.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::super::common::{cylinder, id_quat, prim, quat_x, quat_xyzw, spine, with_cut};
use super::super::{HelicopterColours, PodPlan, dim};

/// The facets round the pod's section, and round a skin's - a skin's are
/// finer, so its cut edges stay crisp against the pod.
const POD_RES: u32 = 28;
const SKIN_RES: u32 = 32;

/// The exhaust stack's station, its rise over the crown, its rake aft
/// (degrees, about `+X`; negative tips its top aft), and its length.
const EXHAUST_Z: f32 = -0.40;
const EXHAUST_RISE: f32 = 0.05;
const EXHAUST_RAKE_DEG: f32 = -35.0;
const EXHAUST_LEN: f32 = 0.12;

/// A sweep over the pod's own stations, `proud` (prototype metres) over
/// it, with the pod's section scale.
fn over_pod(
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

/// The cabin: one sweep, nose to rear.
pub(super) fn pod(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    kids.push(over_pod(plan, 0.0, POD_RES, c.body.clone()));
}

/// A skin laid on the pod between prototype stations `z0 < z1`, one sweep
/// per angular band.
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
        let mut g = over_pod(plan, proud, SKIN_RES, material.clone());
        g.kind = with_cut(g.kind, [band.0, band.1], [t0, t1], 0.0);
        kids.push(g);
    }
}

/// Every skin, in the prototype's order.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    // The two-tone: the lower body in the scheme's second hue, nose to tail
    // cone.
    skin(kids, plan, (-0.55, 1.10), &[(0.56, 0.94)], &c.lower, 0.004);
    // The windscreen wrapping the nose, a roof frame strip left at the crown.
    skin(
        kids,
        plan,
        (0.64, 1.17),
        &[(0.03, 0.22), (0.28, 0.47)],
        &c.glass,
        0.007,
    );
    // The chin windows under it, a keel strip left between.
    skin(
        kids,
        plan,
        (0.84, 1.12),
        &[(0.55, 0.69), (0.81, 0.95)],
        &c.glass,
        0.008,
    );
    // The lit door panes, front and rear, behind a door pillar.
    for run in [(0.33, 0.57), (0.05, 0.29)] {
        skin(
            kids,
            plan,
            run,
            &[(0.05, 0.17), (0.33, 0.45)],
            &c.lamp,
            0.007,
        );
    }
}

/// The engine and transmission fairing on the roof, read off the crown:
/// `(station, radius)` nose to rear, each centred 6 cm under the crown.
const FAIRING: [(f32, f32); 5] = [
    (0.32, 0.05),
    (0.18, 0.115),
    (-0.10, 0.125),
    (-0.36, 0.10),
    (-0.52, 0.06),
];

pub(super) fn fairing(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    let pts: Vec<([f32; 3], f32)> = FAIRING
        .iter()
        .map(|&(z, r)| (plan.at(0.0, plan.crown(z) - 0.06, z), plan.len(r)))
        .collect();
    kids.push(prim(spine(&pts, 20, c.body.clone()), [0.0; 3], id_quat()));
}

/// The exhaust stack at the fairing's rear, raked aft.
pub(super) fn exhaust(kids: &mut Vec<Generator>, plan: &PodPlan, c: &HelicopterColours) {
    kids.push(prim(
        cylinder(
            dim(plan.len(0.035)),
            plan.len(EXHAUST_LEN),
            14,
            c.dark.clone(),
        ),
        plan.at(0.0, plan.crown(EXHAUST_Z) + EXHAUST_RISE, EXHAUST_Z),
        quat_xyzw(quat_x(EXHAUST_RAKE_DEG.to_radians())),
    ));
}

/// The stack's mouth, in the prototype frame: its top end, the stack's own
/// half-length along its raked axis from its centre - where an exhaust
/// aura leaves her (`Utility::fx_mount`).
pub(super) fn exhaust_mouth(plan: &PodPlan) -> (f32, f32, f32) {
    let rake = EXHAUST_RAKE_DEG.to_radians();
    let half = EXHAUST_LEN * 0.5;
    // A turn of `rake` about +X carries the stack's +Y toward +Z by its sine.
    (
        0.0,
        plan.crown(EXHAUST_Z) + EXHAUST_RISE + half * rake.cos(),
        EXHAUST_Z + half * rake.sin(),
    )
}
