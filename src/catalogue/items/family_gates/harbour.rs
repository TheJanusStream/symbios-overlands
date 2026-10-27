//! Harbour Gateway - a family gate for the coast (#1796), stood by pirate
//! and resort worlds beside their own. A weathered stone quay arch: two
//! tide-darkened ashlar piers carry a lintel and a ring of voussoirs round
//! a tympanum with an iron anchor on it, a ship's lantern hangs off the
//! keystone on a bracket, a ship's bell hangs off one pier, and two
//! cast-iron mooring bollards stand at its feet.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! pirate and coastal resort pools through `kindred_gates`.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::pirate::{
    BRONZE_FITTING, IRON_BLACK, ROPE_HEMP, STONE_QUAY, ashlar, bronze, cobbles, hemp, iron, lantern,
};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, footing, id_quat, lathe, prim, quat_x, quat_z,
    solid, strut, tint, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};

pub struct HarbourGateway;

impl CatalogueEntry for HarbourGateway {
    fn slug(&self) -> &'static str {
        "harbour_gateway"
    }
    fn name(&self) -> &'static str {
        "Harbour Gateway"
    }
    fn description(&self) -> &'static str {
        "A weathered stone quay arch with a ship's lantern, a ship's bell and mooring bollards."
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

/// Half the opening between the piers' inner faces (m).
const HALF_OPENING: f32 = 1.45;
/// Top of the quay slab the piers stand on.
const SLAB_TOP: f32 = 0.35;
/// Underside of the lintel, the head of the opening.
const LINTEL_UNDER: f32 = 3.5;
/// The arch's springing line, on the impost tops.
const SPRING: f32 = 4.2;

fn build_tree() -> Generator {
    let pier_x = HALF_OPENING + 0.55;
    let pier_top = 3.95;
    let tide = tint(STONE_QUAY, [0.66, 0.72, 0.66]);

    // The buried footing is the root; everything hangs off it in the
    // ground frame.
    let mut prims = vec![footing(6.4, 2.6, [0.0, 0.0], 3.5)];
    // Quay flagstones.
    prims.push(prim(
        solid(cuboid_tapered(
            [6.4, SLAB_TOP, 2.6],
            0.0,
            cobbles(STONE_QUAY, 0x4B1),
        )),
        [0.0, SLAB_TOP * 0.5, 0.0],
        id_quat(),
    ));

    for (i, sx) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let seed = 0x4B10 + i as u32 * 7;
        // Pier, sunk 2 cm into the quay so its foot shares no face with
        // the plinth course round it.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.1, pier_top - SLAB_TOP + 0.02, 1.3],
                0.0,
                ashlar(STONE_QUAY, seed),
            )),
            [sx * pier_x, (pier_top + SLAB_TOP - 0.02) * 0.5, 0.0],
            id_quat(),
        ));
        // Tide-darkened plinth course, a centimetre shy of the opening.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.2, 0.55, 1.42],
                0.0,
                ashlar(tide, seed ^ 3),
            )),
            [sx * (pier_x + 0.06), SLAB_TOP + 0.275, 0.0],
            id_quat(),
        ));
        // Impost the arch springs from, oversailing the pier.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.3, SPRING - pier_top, 1.45],
                0.0,
                ashlar(STONE_QUAY, seed ^ 5),
            )),
            [sx * pier_x, (SPRING + pier_top) * 0.5, 0.0],
            id_quat(),
        ));
        // An iron mooring ring on its staple, low on the pier's face.
        prims.push(prim(
            torus(0.025, 0.11, iron(IRON_BLACK, seed ^ 9)),
            [sx * pier_x, 1.25, -0.672],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            cuboid_tapered([0.08, 0.06, 0.06], 0.0, iron(IRON_BLACK, seed ^ 11)),
            [sx * pier_x, 1.36, -0.67],
            id_quat(),
        ));
        // A cast-iron bollard at each front corner of the quay.
        let bx = sx * 2.6;
        prims.push(prim(
            solid(lathe(
                &[
                    (0.17, 0.0),
                    (0.155, 0.08),
                    (0.13, 0.38),
                    (0.21, 0.46),
                    (0.21, 0.53),
                    (0.15, 0.58),
                    (0.0, 0.6),
                ],
                14,
                false,
                iron(IRON_BLACK, seed ^ 13),
            )),
            [bx, SLAB_TOP - 0.01, -0.85],
            id_quat(),
        ));
    }
    // A hawser coiled round the left bollard's foot.
    for (k, y) in [0.42_f32, 0.49].into_iter().enumerate() {
        prims.push(prim(
            torus(0.035, 0.2 - k as f32 * 0.01, hemp(ROPE_HEMP)),
            [-2.6, y, -0.85],
            id_quat(),
        ));
    }

    // Lintel over the opening, its ends bedded in the piers.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_OPENING + 0.2, pier_top - LINTEL_UNDER, 1.0],
            0.0,
            ashlar(STONE_QUAY, 0x4B20),
        )),
        [0.0, (pier_top + LINTEL_UNDER) * 0.5, 0.0],
        id_quat(),
    ));
    // Tympanum under the arch, recessed behind the voussoirs: a wide lower
    // panel and a narrower one up into the crown, their edges inside the
    // ring of stones.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.95, 5.1 - pier_top, 0.5],
            0.0,
            ashlar(tint(STONE_QUAY, [0.9, 0.9, 0.9]), 0x4B21),
        )),
        [0.0, (5.1 + pier_top) * 0.5, 0.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [2.4, 0.7, 0.46],
            0.0,
            ashlar(tint(STONE_QUAY, [0.9, 0.9, 0.9]), 0x4B22),
        )),
        [0.0, 5.4, 0.05],
        id_quat(),
    ));
    prims.extend(anchor([0.0, 4.62, -0.2]));
    prims.extend(voussoirs());

    // The ship's lantern, hung off the keystone on an iron bracket.
    let key_front = -0.68;
    let arm_y = 6.05;
    let lantern_z = -1.45;
    prims.push(prim(
        cuboid_tapered([0.07, 0.07, 0.85], 0.0, iron(IRON_BLACK, 0x4B30)),
        [0.0, arm_y, key_front - 0.42],
        id_quat(),
    ));
    prims.push(strut(
        [0.0, 5.7, key_front + 0.01],
        [0.0, arm_y - 0.03, -1.3],
        0.025,
        6,
        iron(IRON_BLACK, 0x4B31),
    ));
    prims.push(strut(
        [0.0, arm_y - 0.03, lantern_z],
        [0.0, 5.55, lantern_z],
        0.015,
        6,
        iron(IRON_BLACK, 0x4B32),
    ));
    prims.push(lantern([0.0, 5.25, lantern_z], 0.6, 0x4B33));

    // The ship's bell, hung off the `+X` pier's face.
    let bell_x = pier_x;
    let bell_z = -1.05;
    prims.push(prim(
        cuboid_tapered([0.07, 0.07, 0.62], 0.0, iron(IRON_BLACK, 0x4B40)),
        [bell_x, 3.15, -0.94],
        id_quat(),
    ));
    prims.push(strut(
        [bell_x, 2.6, -0.64],
        [bell_x, 3.12, -1.1],
        0.03,
        6,
        iron(IRON_BLACK, 0x4B41),
    ));
    prims.push(prim(
        lathe(
            &[
                (0.0, 0.0),
                (0.21, 0.0),
                (0.2, 0.04),
                (0.15, 0.14),
                (0.13, 0.26),
                (0.12, 0.33),
                (0.08, 0.38),
                (0.0, 0.4),
            ],
            16,
            true,
            bronze(BRONZE_FITTING, 0x4B42),
        ),
        [bell_x, 2.71, bell_z],
        id_quat(),
    ));
    // Its lanyard, knotted at the end.
    prims.push(strut(
        [bell_x, 2.74, bell_z],
        [bell_x, 2.3, bell_z],
        0.018,
        6,
        hemp(ROPE_HEMP),
    ));
    prims.push(prim(
        torus(0.02, 0.025, hemp(ROPE_HEMP)),
        [bell_x, 2.33, bell_z],
        id_quat(),
    ));

    // The walk-in zone, its edges bedded 4 cm in the piers, the lintel and
    // the quay (#1006).
    let veil_h = LINTEL_UNDER - SLAB_TOP + 0.08;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.0 * HALF_OPENING + 0.08, veil_h, 0.8]),
        },
        [0.0, SLAB_TOP - 0.04 + veil_h * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// The ring of wedge stones round the tympanum, springing from the imposts:
/// nine voussoirs each narrowing toward the opening, alternately 2 cm
/// shallower so neighbours share no face, and a deeper, taller keystone.
fn voussoirs() -> Vec<Generator> {
    let r_in = HALF_OPENING;
    let count = 9;
    (0..count)
        .map(|i| {
            let theta = PI * (i as f32 + 0.5) / count as f32;
            let key = i == count / 2;
            let radial: f32 = if key { 0.72 } else { 0.55 };
            let outer = if key { 0.76 } else { 0.68 };
            let depth = if key {
                1.36
            } else if i % 2 == 0 {
                1.3
            } else {
                1.26
            };
            let r_mid = r_in + radial * 0.5;
            // The wedge's own +Y points in toward the arch's centre, so its
            // taper narrows it there.
            prim(
                solid(cuboid_tapered_xz(
                    [outer, radial, depth],
                    [1.0 - 0.49 / outer, 0.0],
                    ashlar(STONE_QUAY, 0x4B50 + i as u32),
                )),
                [r_mid * theta.cos(), SPRING + r_mid * theta.sin(), 0.0],
                quat_z(theta + FRAC_PI_2),
            )
        })
        .collect()
}

/// An iron anchor fixed to the tympanum's face at `at` (its shank's
/// centre, on the face): shank, stock, ring and two arms, each bedded a
/// centimetre in the stone.
fn anchor(at: [f32; 3]) -> Vec<Generator> {
    let [x, y, z] = at;
    let mat = |seed| iron(IRON_BLACK, seed);
    let mut parts = vec![
        prim(
            cuboid_tapered([0.07, 0.62, 0.05], 0.0, mat(0x4B60)),
            [x, y, z - 0.015],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.42, 0.06, 0.07], 0.0, mat(0x4B61)),
            [x, y + 0.2, z - 0.025],
            id_quat(),
        ),
        prim(
            torus(0.02, 0.07, mat(0x4B62)),
            [x, y + 0.38, z - 0.02],
            quat_x(FRAC_PI_2),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.32, 0.06, 0.064], 0.0, mat(0x4B63)),
            [x + sx * 0.13, y - 0.24, z - 0.022],
            quat_z(sx * 0.6),
        ));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&HarbourGateway);
    }
}
