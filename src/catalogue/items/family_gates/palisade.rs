//! Palisade Gateway - a family gate for the timber north (#1796), stood by
//! Norse and medieval worlds beside their own. The gate of a stockade: two
//! log towers under shingled pyramid roofs, joined by a walkway over the way
//! behind a parapet of pointed stakes, a chip-carved lintel under it, a
//! torch on each tower's face and a run of sharpened stakes off either side.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! Nordic and medieval pools through `kindred_gates`.

use crate::catalogue::items::nordic::{
    FIRE_ORANGE, IRON_DARK, WOOD_DARK, WOOD_WARM, boards, iron, shingle, timber,
};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, prim, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};

pub struct PalisadeGateway;

impl CatalogueEntry for PalisadeGateway {
    fn slug(&self) -> &'static str {
        "palisade_gateway"
    }
    fn name(&self) -> &'static str {
        "Palisade Gateway"
    }
    fn description(&self) -> &'static str {
        "A stockade gate of two log towers joined by a walkway, its stakes running off either side."
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

/// Half the opening between the towers' inner faces (m).
const HALF_OPENING: f32 = 1.4;
/// Top of the plank causeway through the gate.
const SILL_TOP: f32 = 0.3;
/// Underside of the carved lintel, the head of the opening.
const LINTEL_UNDER: f32 = 3.65;
/// Top of the walkway deck.
const DECK_TOP: f32 = 4.3;
/// Top of the towers' walls, under their roofs.
const TOWER_TOP: f32 = 4.9;

fn build_tree() -> Generator {
    let tower_w = 1.3;
    let tower_d = 1.6;
    let tower_x = HALF_OPENING + tower_w * 0.5;

    let mut prims = vec![footing(5.8, 2.4, [0.0, 0.0], 3.5)];
    // Plank causeway through the gate.
    prims.push(prim(
        solid(cuboid_tapered([5.8, SILL_TOP, 2.4], 0.0, boards(WOOD_WARM))),
        [0.0, SILL_TOP * 0.5, 0.0],
        id_quat(),
    ));

    for sx in [-1.0_f32, 1.0] {
        let x = sx * tower_x;
        // Tower body, sunk 2 cm into the causeway.
        prims.push(prim(
            solid(cuboid_tapered(
                [tower_w, TOWER_TOP - SILL_TOP + 0.02, tower_d],
                0.0,
                timber(WOOD_WARM),
            )),
            [x, (TOWER_TOP + SILL_TOP - 0.02) * 0.5, 0.0],
            id_quat(),
        ));
        // A whole log up each corner, the inner ones standing a little into
        // the opening.
        for cx in [-1.0_f32, 1.0] {
            for cz in [-1.0_f32, 1.0] {
                prims.push(prim(
                    solid(cylinder_tapered(
                        0.17,
                        TOWER_TOP + 0.3,
                        10,
                        0.0,
                        timber(WOOD_DARK),
                    )),
                    [
                        x + cx * (tower_w * 0.5 + 0.1),
                        (TOWER_TOP - 0.3) * 0.5,
                        cz * 0.8,
                    ],
                    id_quat(),
                ));
            }
        }
        // Eave board and a shingled pyramid roof.
        prims.push(prim(
            solid(cuboid_tapered(
                [tower_w + 0.7, 0.14, tower_d + 0.6],
                0.0,
                timber(WOOD_DARK),
            )),
            [x, TOWER_TOP + 0.07, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [tower_w + 0.6, 1.3, tower_d + 0.5],
                0.96,
                shingle([0.26, 0.21, 0.16]),
            )),
            [x, TOWER_TOP + 0.13 + 0.65, 0.0],
            id_quat(),
        ));
        // A dark look-out slit high on the tower's face.
        prims.push(prim(
            cuboid_tapered([0.16, 0.55, 0.04], 0.0, timber([0.05, 0.04, 0.03])),
            [x, 3.9, -tower_d * 0.5 - 0.01],
            id_quat(),
        ));
        prims.extend(torch([x, 2.6, -tower_d * 0.5]));
        prims.extend(stake_run(sx, tower_x + tower_w * 0.5));
    }

    // The carved lintel over the way, its ends bedded in the towers.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_OPENING + 0.4, 4.1 - LINTEL_UNDER, 1.0],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, (4.1 + LINTEL_UNDER) * 0.5, 0.0],
        id_quat(),
    ));
    // Chip-carved diamonds down its face, between two rolled mouldings.
    let face = -0.5;
    for k in 0..7 {
        let x = (k as f32 - 3.0) * 0.38;
        prims.push(prim(
            cuboid_tapered([0.17, 0.17, 0.05], 0.0, timber(WOOD_WARM)),
            [x, 3.875, face - 0.015],
            quat_z(std::f32::consts::FRAC_PI_4),
        ));
    }
    for y in [3.71_f32, 4.04] {
        prims.push(prim(
            cylinder_tapered(0.035, 2.0 * HALF_OPENING - 0.04, 8, 0.0, timber(WOOD_WARM)),
            [0.0, y, face - 0.01],
            quat_z(std::f32::consts::FRAC_PI_2),
        ));
    }
    // The walkway deck over it, its ends in the towers' faces.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_OPENING + 0.1, DECK_TOP - 4.1, 1.9],
            0.0,
            boards(WOOD_WARM),
        )),
        [0.0, (DECK_TOP + 4.1) * 0.5, 0.0],
        id_quat(),
    ));
    // A parapet of pointed stakes along both edges of the walkway, on a
    // rail run between the towers.
    for sz in [-1.0_f32, 1.0] {
        let z = sz * 0.85;
        prims.push(prim(
            cuboid_tapered(
                [2.0 * HALF_OPENING + 0.04, 0.12, 0.12],
                0.0,
                timber(WOOD_DARK),
            ),
            [0.0, DECK_TOP + 0.5, z],
            id_quat(),
        ));
        for k in 0..9 {
            let x = (k as f32 - 4.0) * 0.3;
            prims.extend(stake([x, DECK_TOP - 0.02, z], 0.065, 0.78));
        }
    }

    // The walk-in zone, its edges bedded 4 cm in the towers, the lintel and
    // the causeway (#1006).
    let veil_h = LINTEL_UNDER - SILL_TOP + 0.08;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.0 * HALF_OPENING + 0.08, veil_h, 0.8]),
        },
        [0.0, SILL_TOP - 0.04 + veil_h * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// A sharpened stake standing from `foot`: a log of `radius` whose shaft
/// runs `shaft` metres up to a hewn point.
fn stake(foot: [f32; 3], radius: f32, shaft: f32) -> Vec<Generator> {
    let [x, y, z] = foot;
    let tip = radius * 3.0;
    vec![
        prim(
            solid(cylinder_tapered(radius, shaft, 8, 0.0, timber(WOOD_DARK))),
            [x, y + shaft * 0.5, z],
            id_quat(),
        ),
        prim(
            cylinder_tapered(radius, tip, 8, 0.95, timber(WOOD_WARM)),
            [x, y + shaft + tip * 0.5 - 0.01, z],
            id_quat(),
        ),
    ]
}

/// The palisade running off a tower's outer face at `from` (its x, on
/// side `sx`): three stakes stepping down away from the gate, driven into
/// the ground and bound by a rail lashed to the tower.
fn stake_run(sx: f32, from: f32) -> Vec<Generator> {
    let mut parts = Vec::new();
    for (k, shaft) in [2.9_f32, 2.6, 2.3].into_iter().enumerate() {
        let x = sx * (from + 0.15 + k as f32 * 0.28);
        let z = if k % 2 == 0 { 0.0 } else { 0.03 };
        parts.extend(stake([x, -0.3, z], 0.14, shaft + 0.3));
    }
    let rail_len = 0.95;
    parts.push(prim(
        solid(cylinder_tapered(0.08, rail_len, 8, 0.0, timber(WOOD_WARM))),
        [sx * (from - 0.02 + rail_len * 0.5), 1.5, -0.18],
        quat_z(std::f32::consts::FRAC_PI_2),
    ));
    parts
}

/// A torch on an iron bracket off a tower's face at `at` (on the face).
fn torch(at: [f32; 3]) -> Vec<Generator> {
    let [x, y, z] = at;
    let cup_z = z - 0.3;
    vec![
        prim(
            cuboid_tapered([0.06, 0.06, 0.32], 0.0, iron(IRON_DARK)),
            [x, y, z - 0.15],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.08, 0.14, 10, 0.0, iron(IRON_DARK)),
            [x, y + 0.09, cup_z],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.04, 0.5, 8, 0.0, timber(WOOD_DARK)),
            [x, y + 0.3, cup_z],
            id_quat(),
        ),
        // The flame - a small hot core, so it reads as fire rather than as
        // a lit ball.
        prim(
            sphere(0.09, 3, glow(FIRE_ORANGE, 4.0)),
            [x, y + 0.6, cup_z],
            id_quat(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&PalisadeGateway);
    }
}
