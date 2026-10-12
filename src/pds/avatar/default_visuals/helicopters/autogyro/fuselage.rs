//! The fuselage: the one sweep and the skins laid on it - the cheat line
//! down each flank, and the two open cockpits, each a lit well with a
//! leather lip at its back and a windscreen ahead - the biplane's, on her
//! own stations.
//!
//! The skins are the family's idiom (`trainer::bubble`): each a second sweep
//! over the fuselage's OWN stations, a few millimetres proud, with its
//! section scale, trimmed fore and aft by arc length ([`PodPlan::along`])
//! and round the section by angle (`0.25` is the crown).

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::super::common::{cuboid, id_quat, prim, quat_x, quat_xyzw, spine, with_cut};
use super::super::{AutogyroColours, MIN_DIM, PodPlan, dim};

/// The facets round the fuselage's section, and round a skin's - a skin's
/// are finer, so its cut edges stay crisp.
const FUSE_RES: u32 = 28;
const SKIN_RES: u32 = 32;

/// The two cockpits' runs, front then rear (prototype frame): the front
/// one under the pylon.
const COCKPITS: [(f32, f32); 2] = [(0.66, 0.94), (0.26, 0.54)];

/// A windscreen's width and height, how far ahead of its cockpit it stands,
/// how far its middle stands over the crown, and how far it leans back
/// (degrees) - its bottom face's middle 1.2 cm into the crown (prototype
/// frame).
const WIND_W: f32 = 0.16;
const WIND_H: f32 = 0.06;
const WIND_AHEAD: f32 = 0.03;
const WIND_OVER: f32 = 0.012;
const WIND_TILT_DEG: f32 = 35.0;

/// A sweep over the fuselage's own stations, `proud` (prototype metres)
/// over it, with its section scale.
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
    for &(a, b) in bands {
        let mut g = over_fuselage(plan, proud, SKIN_RES, material.clone());
        g.kind = with_cut(g.kind, [a, b], [t0, t1], 0.0);
        kids.push(g);
    }
}

/// The fuselage: one sweep, crankcase to tail post.
pub(super) fn fuselage(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    kids.push(over_fuselage(plan, 0.0, FUSE_RES, c.body.clone()));
}

/// The cheat line: a stripe down each flank, firewall to tail - the seed's
/// accent, the fleet's identity slot.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    skin(
        kids,
        plan,
        (-1.55, 1.10),
        &[(0.035, 0.075), (0.425, 0.465)],
        &c.stripe,
        0.003,
    );
}

/// The two open cockpits in tandem, front then rear: each a well in her
/// decking lit with the fleet's window light, a small windscreen ahead of
/// it, raked back, and a leather lip across its back edge - the biplane's.
pub(super) fn cockpits(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    for (z0, z1) in COCKPITS {
        skin(kids, plan, (z0, z1), &[(0.17, 0.33)], &c.lamp, 0.004);
        // Its bottom face's middle into the crown: the fuselage, a
        // flattened sweep, offers `touch` no axis to meet, so a sample of
        // the screen must stand inside it.
        let z = z1 + WIND_AHEAD;
        kids.push(prim(
            cuboid(
                [plan.len(WIND_W), plan.len(WIND_H), dim(plan.len(MIN_DIM))],
                c.glass.clone(),
            ),
            plan.at(0.0, plan.crown(z) + WIND_OVER, z),
            quat_xyzw(quat_x(-WIND_TILT_DEG.to_radians())),
        ));
        skin(
            kids,
            plan,
            (z0 - 0.02, z0),
            &[(0.16, 0.34)],
            &c.leather,
            0.010,
        );
    }
}
