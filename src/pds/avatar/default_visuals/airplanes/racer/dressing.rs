//! The ladder of secondary masses by ornateness - #1629 Q-F, agreed. An
//! adorned machine carries a race roundel on each flank behind her cockpit,
//! white on a dark rim, and a pitot ahead of her port wing; an ornate one a
//! winglet on each wingtip in the trim colour - her nav lights moved out
//! onto them ([`super::wing::lights`]) - and a lit strip low down each flank
//! from her wing to her tail, the neon of her homes. Wear has no ladder of
//! its own: it is the [`MaterialKit`](crate::seeded_defaults::MaterialKit)'s
//! grime on every surface.

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::{id_quat, prim, rod, sphere, sweep};
use super::super::{FusePlan, MIN_DIM, RacerColours, dim, skin};
use super::wing::{HALF_SPAN, on_wing, wing_le, wing_mid, wing_y};

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &FusePlan,
    c: &RacerColours,
    ornateness: OrnatenessTier,
) {
    if ornateness == OrnatenessTier::Plain {
        return;
    }
    roundels(kids, plan, c);
    pitot(kids, plan, c);
    if ornateness == OrnatenessTier::Ornate {
        winglets(kids, plan, c);
        lit_strips(kids, plan, c);
    }
}

/// A race roundel (prototype frame): its station behind the cockpit, how
/// far over the centreline it stands, and its two lenses - a dark rim and a
/// white disc a little proud of it - each a sphere drawn this flat across
/// her, half in her skin: their radii and their width over radius.
const ROUNDEL_Z: f32 = -0.36;
const ROUNDEL_RISE: f32 = 0.01;
const RIM_R: f32 = 0.110;
const RIM_FLAT: f32 = 0.15;
const DISC_R: f32 = 0.095;
const DISC_FLAT: f32 = 0.16;
const DISC_OUT: f32 = 0.004;

/// A race roundel on each flank, port (`+X`) first: the dark rim on her
/// flank, and the white disc on it.
fn roundels(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let y = plan.fuse_at(ROUNDEL_Z).0 + ROUNDEL_RISE;
    let flank = plan.flank(ROUNDEL_Z, y);
    for side in [1.0f32, -1.0] {
        for (r, flat, out, material) in [
            (RIM_R, RIM_FLAT, 0.0, &c.dark),
            (DISC_R, DISC_FLAT, DISC_OUT, &c.roundel),
        ] {
            let mut g = prim(
                sphere(plan.len(r), 4, material.clone()),
                plan.at(side * (flank + out), y, ROUNDEL_Z),
                id_quat(),
            );
            g.transform.scale = Fp3([flat, 1.0, 1.0]);
            kids.push(g);
        }
    }
}

/// The pitot (prototype frame): its span station as a share of the
/// half-span, how far under the wing's mid-plane it runs, and its ends - its
/// root behind the wing's leading edge, its tip ahead of it.
const PITOT_SPAN: f32 = 0.62;
const PITOT_DROP: f32 = 0.01;
const PITOT_ROOT_BACK: f32 = 0.04;
const PITOT_AHEAD: f32 = 0.16;

/// A pitot ahead of the port wing's leading edge.
fn pitot(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let x = HALF_SPAN * PITOT_SPAN;
    let (y, z) = (wing_y(x) - PITOT_DROP, wing_le(x));
    kids.push(rod(
        on_wing(plan, x, y, z - PITOT_ROOT_BACK),
        on_wing(plan, x, y, z + PITOT_AHEAD),
        dim(plan.len(MIN_DIM)),
        c.metal.clone(),
    ));
}

/// A winglet (prototype frame): its root's `x`, just in from the wingtip;
/// its stations `(out, rise over the tip's mid-plane, dz from the tip's
/// mid-chord, half-chord)` root to top - canted out and swept aft - drawn
/// [`WINGLET_FLAT`] as thick as its chord; and where its nav light stands on
/// it, `(out, rise, dz)`: on its outer face, half way up.
pub(super) const WINGLET_X: f32 = HALF_SPAN - 0.03;
const WINGLET: [(f32, f32, f32, f32); 4] = [
    (0.0, -0.01, 0.01, 0.10),
    (0.02, 0.07, -0.02, 0.085),
    (0.04, 0.14, -0.05, 0.06),
    (0.05, 0.18, -0.07, 0.025),
];
const WINGLET_FLAT: f32 = 0.14;
pub(super) const WINGLET_LIGHT: [f32; 3] = [0.045, 0.07, -0.02];

/// A winglet on each wingtip in the trim colour, port (`+X`) first: a
/// flattened sweep up from the tip, canted out and swept aft.
fn winglets(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    let (y, z) = (wing_y(HALF_SPAN), wing_mid(HALF_SPAN));
    for side in [1.0f32, -1.0] {
        // Drawn thinner than deep by its node's x scale: the x is divided
        // out.
        let points: Vec<([f32; 3], f32)> = WINGLET
            .iter()
            .map(|&(out, rise, dz, r)| {
                let [px, py, pz] = on_wing(plan, side * (WINGLET_X + out), y + rise, z + dz);
                ([px / WINGLET_FLAT, py, pz], plan.len(r))
            })
            .collect();
        let mut g = prim(sweep(&points, 16, 5, c.trim.clone()), [0.0; 3], id_quat());
        g.transform.scale = Fp3([WINGLET_FLAT, 1.0, 1.0]);
        kids.push(g);
    }
}

/// A lit strip low down each flank, from just behind her wing's root to
/// her tail - mirror-symmetric about the keel - in the fleet's lit window.
fn lit_strips(kids: &mut Vec<Generator>, plan: &FusePlan, c: &RacerColours) {
    skin(
        kids,
        plan,
        (-1.10, 0.05),
        &[Some((0.555, 0.585)), Some((0.915, 0.945))],
        &c.lamp,
        0.004,
    );
}
