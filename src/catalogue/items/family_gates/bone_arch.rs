//! Bone Arch Gateway - a family gate for the uncanny (#1796), stood by
//! alien organic and gothic horror worlds beside their own. Two great pale
//! thigh bones stand on dark flags under a third laid across their heads, a
//! horned skull watching from the lintel; behind it a ribcage arches over
//! the way on a spine of vertebrae, and a lantern hangs from the spine in
//! the ribs.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! alien organic and gothic horror pools through `kindred_gates`.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::gothic_horror::{STONE_DARK, mossy};
use crate::catalogue::items::pirate::{BONE_PALE, WITCHFIRE, bone, lantern};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lathe, prim, quat_z, solid,
    sphere, spine, strut, superellipsoid, tint,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};

pub struct BoneArchGateway;

impl CatalogueEntry for BoneArchGateway {
    fn slug(&self) -> &'static str {
        "bone_arch_gateway"
    }
    fn name(&self) -> &'static str {
        "Bone Arch Gateway"
    }
    fn description(&self) -> &'static str {
        "An arch of great pale bones under a horned skull, a ribcage over the way and a lantern in its ribs."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn footprint(&self) -> Footprint {
        super::gate_footprint()
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Where the upright bones stand, out from the way's middle (m).
const BONE_X: f32 = 1.65;
/// Top of the flagstones.
const FLAG_TOP: f32 = 0.3;
/// The lintel bone's axis height.
const LINTEL_Y: f32 = 4.12;
/// The spine's height along the top of the ribcage.
const SPINE_Y: f32 = 4.42;

fn build_tree() -> Generator {
    let flag_z = 0.8;
    let mut prims = vec![footing(5.2, 4.0, [0.0, flag_z], 3.5)];
    prims.push(prim(
        solid(cuboid_tapered([5.2, FLAG_TOP, 4.0], 0.0, mossy(STONE_DARK))),
        [0.0, FLAG_TOP * 0.5, flag_z],
        id_quat(),
    ));

    // The two upright thigh bones, their feet bedded in the flags.
    let upright = 3.75;
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(thigh_bone(upright)),
            [sx * BONE_X, FLAG_TOP - 0.02, 0.0],
            id_quat(),
        ));
        // A small skull fallen at each bone's foot.
        prims.push(prim(
            superellipsoid([0.17, 0.15, 0.19], 0.8, 0.8, bone(BONE_PALE)),
            [sx * 2.2, FLAG_TOP + 0.13, -0.75],
            id_quat(),
        ));
    }
    // The third laid across their heads, from left to right.
    let span = 4.2;
    prims.push(prim(
        solid(thigh_bone(span)),
        [-span * 0.5, LINTEL_Y, 0.0],
        quat_z(-FRAC_PI_2),
    ));

    prims.extend(skull([0.0, 4.5, -0.32]));

    // The spine running back from the skull over the way, a dorsal spike on
    // each vertebra, and four pairs of ribs arching down to the flags.
    for k in 0..10 {
        let z = 0.05 + 0.3 * k as f32;
        prims.push(prim(
            solid(superellipsoid(
                [0.16, 0.12, 0.17],
                0.7,
                0.8,
                bone(BONE_PALE),
            )),
            [0.0, SPINE_Y, z],
            id_quat(),
        ));
        prims.push(prim(
            cylinder_tapered(0.05, 0.3, 8, 0.9, bone(BONE_PALE)),
            [0.0, SPINE_Y + 0.2, z],
            id_quat(),
        ));
    }
    let rib_mat = bone(tint(BONE_PALE, [0.92, 0.9, 0.86]));
    for z in [0.65_f32, 1.25, 1.85, 2.45] {
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                spine(
                    &[
                        ([sx * 0.12, SPINE_Y - 0.04, z], 0.08),
                        ([sx * 0.9, 4.25, z], 0.09),
                        ([sx * 1.6, 3.6, z], 0.09),
                        ([sx * 1.9, 2.3, z], 0.08),
                        ([sx * 1.75, 0.9, z], 0.07),
                        ([sx * 1.6, FLAG_TOP - 0.05, z], 0.07),
                    ],
                    8,
                    4,
                    rib_mat.clone(),
                ),
                [0.0, 0.0, 0.0],
                id_quat(),
            ));
        }
    }

    // The lantern hung from the spine in the ribs, on a chain.
    let lantern_z = 1.55;
    prims.push(strut(
        [0.0, SPINE_Y - 0.11, lantern_z],
        [0.0, 3.82, lantern_z],
        0.015,
        6,
        bone([0.18, 0.17, 0.16]),
    ));
    prims.push(lantern([0.0, 3.52, lantern_z], 0.6, 0x1796));

    // The walk-in zone between the bones, as thin a pane as a veil may be
    // (0.25 m) so its edges stay in their round shafts, bedded in the lintel bone and the flags
    // (#1006).
    let top = 4.0;
    let veil_h = top - FLAG_TOP + 0.04;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([3.08, veil_h, 0.25]),
        },
        [0.0, FLAG_TOP - 0.04 + veil_h * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// A thigh bone `len` metres long, turned about its own axis from a knobbed
/// head at its base to a knobbed head at its far end, both ends closed.
fn thigh_bone(len: f32) -> GeneratorKind {
    let profile: Vec<(f32, f32)> = [
        (0.0, 0.0),
        (0.3, 0.02),
        (0.31, 0.06),
        (0.22, 0.14),
        (0.17, 0.3),
        (0.16, 0.5),
        (0.17, 0.7),
        (0.22, 0.86),
        (0.31, 0.94),
        (0.3, 0.98),
        (0.0, 1.0),
    ]
    .into_iter()
    .map(|(r, h)| (r, h * len))
    .collect();
    lathe(&profile, 14, true, bone(BONE_PALE))
}

/// The horned skull watching from the lintel, centred at `at`: a cranium,
/// a snout, two dark sockets with a cold light in each, and a horn sweeping
/// up off each side.
fn skull(at: [f32; 3]) -> Vec<Generator> {
    let [x, y, z] = at;
    let mut parts = vec![
        prim(
            solid(superellipsoid(
                [0.36, 0.32, 0.42],
                0.8,
                0.8,
                bone(BONE_PALE),
            )),
            at,
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.3, 0.22, 0.36], 0.3, bone(BONE_PALE)),
            [x, y - 0.18, z - 0.4],
            id_quat(),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            sphere(0.09, 3, bone([0.05, 0.05, 0.05])),
            [x + sx * 0.15, y + 0.06, z - 0.36],
            id_quat(),
        ));
        parts.push(prim(
            sphere(0.04, 3, glow(WITCHFIRE, 3.0)),
            [x + sx * 0.15, y + 0.06, z - 0.42],
            id_quat(),
        ));
        parts.push(prim(
            spine(
                &[
                    ([x + sx * 0.28, y + 0.12, z + 0.02], 0.11),
                    ([x + sx * 0.75, y + 0.45, z - 0.03], 0.08),
                    ([x + sx * 1.0, y + 0.95, z + 0.12], 0.05),
                    ([x + sx * 0.9, y + 1.35, z + 0.32], 0.02),
                ],
                8,
                4,
                bone(tint(BONE_PALE, [0.85, 0.82, 0.76])),
            ),
            [0.0, 0.0, 0.0],
            id_quat(),
        ));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&BoneArchGateway);
    }
}
