//! The bubble: one sweep of dark glass, the skins laid on it - the painted
//! chin, the lit side panes and the door hoop - and the painted bulkhead
//! that closes it at the back.
//!
//! The skins are the utility's idiom (`utility::pod`): each a second sweep
//! over the bubble's OWN stations, a few millimetres proud, with its section
//! scale, trimmed fore and aft by arc length ([`PodPlan::along`]) and round
//! the section by angle - each angular band symmetric about the crown
//! (`0.25`) or the keel (`0.75`), so it is the same whichever way round a
//! sweep's frame turns.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::super::common::{id_quat, prim, spine, sweep, with_cut};
use super::super::{PodPlan, TrainerColours, dim};

/// The facets round the bubble's section, and round a skin's - a skin's are
/// finer, so its cut edges stay crisp against the glass.
const BUBBLE_RES: u32 = 30;
const SKIN_RES: u32 = 32;

/// The bubble's rear, the cabin's back wall (prototype frame).
pub(super) const BULKHEAD_Z: f32 = 0.935;

/// How much of the section the painted chin covers, centred on the keel.
const TUB: f32 = 0.22;

/// A sweep over the bubble's own stations, `proud` (prototype metres) over
/// it, with its section scale.
fn over_bubble(
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

/// The bubble: one sweep of glass, nose to bulkhead.
pub(super) fn bubble(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    kids.push(over_bubble(plan, 0.0, BUBBLE_RES, c.glass.clone()));
}

/// How far ahead of its first station the bubble is DRAWN (m, prototype
/// frame), read off the mesher's own vertices of the very sweep [`bubble`]
/// draws, without building its mesh (`primitive_outline`) - the blunt
/// nose's bulge, which her box's nose takes on
/// ([`PodPlan::with_nose_reach`]). The girth raises each station by half its
/// own growth, so the centreline climbs off the small first station and the
/// rings it tilts lean forward: 3.7 cm on her slimmest bubble, 7.5 on her
/// fullest. The node's section scale stretches `y` alone, so the reach is
/// the mesh's own.
pub(super) fn nose_reach(plan: &PodPlan) -> f32 {
    let g = over_bubble(plan, 0.0, BUBBLE_RES, SovereignMaterialSettings::default());
    let front = crate::world_builder::primitive_outline(&g.kind)
        .into_iter()
        .map(|p| p.z)
        .fold(f32::MIN, f32::max);
    let nose = plan.at(0.0, 0.0, plan.nose())[2];
    (front - nose) / plan.len(1.0)
}

/// A skin laid on the bubble between prototype stations `z0 < z1`, one
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
        let mut g = over_bubble(plan, proud, SKIN_RES, material.clone());
        g.kind = with_cut(g.kind, [band.0, band.1], [t0, t1], 0.0);
        kids.push(g);
    }
}

/// Every skin, in the prototype's order.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    // The chin: the bubble's lower band painted nose to bulkhead - the
    // console's tub.
    skin(
        kids,
        plan,
        (0.94, 1.63),
        &[(0.75 - TUB * 0.5, 0.75 + TUB * 0.5)],
        &c.body,
        0.004,
    );
    // The lit side panes behind the door frame, the windscreen dark (#1621
    // Q-B): the chin strip did not read at 12 m, and a lit bubble read as an
    // orange egg.
    skin(
        kids,
        plan,
        (0.99, 1.30),
        &[(0.03, 0.17), (0.33, 0.47)],
        &c.lamp,
        0.007,
    );
    // The door hoop, all the way round where the doors hang: her identity.
    skin(kids, plan, (1.30, 1.335), &[(0.0, 1.0)], &c.stripe, 0.009);
}

/// The bulkhead: a painted disc closing the bubble at the back, drawn its
/// section times taller than wide as the bubble is - a short sweep along her
/// length under the bubble's own node scale, where the prototype turned a
/// cylinder under a non-uniform scale (a turn under one is no similarity,
/// and `touch` cannot read it).
pub(super) fn bulkhead(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TrainerColours) {
    let (y, r, _) = plan.pod_at(BULKHEAD_Z);
    let section = plan.section();
    let disc = |z: f32| {
        let [x, y, z] = plan.at(0.0, y, z);
        ([x, y / section, z], dim(plan.len(r * 0.98)))
    };
    let mut g = prim(
        sweep(
            &[disc(BULKHEAD_Z - 0.002), disc(BULKHEAD_Z - 0.022)],
            24,
            2,
            c.body.clone(),
        ),
        [0.0; 3],
        id_quat(),
    );
    g.transform.scale = Fp3([1.0, section, 1.0]);
    kids.push(g);
}
