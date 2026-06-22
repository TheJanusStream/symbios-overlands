//! Prospector's shack — the Wild-West *poor* landmark. A tiny weathered
//! timber shack with a crooked stovepipe, a lean-to of tin and a pick left in
//! the dirt. The bust counterpart to the [`saloon`](super::saloon): same
//! frontier, opposite end of the prosperity axis (`Poor`), so a destitute room
//! grows the dried-up claim instead of the boomtown.
//!
//! The stovepipe and pick lean with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DUST_TAN, IRON_DARK, TIN_GREY, WOOD_RAW, canvas, clapboard, iron, tin};

pub struct ProspectorShack;

impl CatalogueEntry for ProspectorShack {
    fn slug(&self) -> &'static str {
        "prospector_shack"
    }
    fn name(&self) -> &'static str {
        "Prospector's Shack"
    }
    fn description(&self) -> &'static str {
        "Tiny weathered timber shack with a crooked stovepipe, a tin lean-to and a pick."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let wall_h = 2.2_f32;
    let wall_top = wall_h;
    // Render FRONT = −Z — door, window and clutter face −Z.
    let front_z = -1.5_f32;

    let mut prims = vec![
        // Weathered timber walls — the root.
        prim(
            solid(cuboid_tapered([3.4, wall_h, 3.0], 0.0, clapboard(WOOD_RAW))),
            [0.0, wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Board-and-batten: a few proud vertical battens breaking the front wall.
    for bx in [-1.5_f32, -0.1, 1.5] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.1, wall_h, 0.1],
                0.0,
                clapboard([0.36, 0.27, 0.16]),
            )),
            [bx, wall_h * 0.5, front_z - 0.03],
            id_quat(),
        ));
    }

    // Sloped, patched tin roof.
    prims.push(prim(
        solid(cuboid_tapered([3.9, 0.2, 3.4], 0.0, tin(TIN_GREY))),
        [0.0, wall_top + 0.2, 0.0],
        quat_x(0.14),
    ));
    for (px, pz) in [(-0.9_f32, 0.6_f32), (0.8, -0.5)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.8, 0.06, 0.8],
                0.0,
                tin([0.42, 0.34, 0.28]),
            )),
            [px, wall_top + 0.36, pz],
            quat_x(0.14),
        ));
    }

    // Plank door + a small lit window on the front.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.8, 1.7, 0.14],
            0.0,
            clapboard([0.3, 0.22, 0.13]),
        )),
        [-0.7, 0.85, front_z - 0.03],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.66, 0.66, 0.06],
            0.0,
            clapboard([0.36, 0.27, 0.16]),
        )),
        [0.75, 1.3, front_z - 0.02],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.48, 0.48, 0.1], 0.0, glow([1.0, 0.72, 0.4], 1.6)),
        [0.75, 1.3, front_z - 0.06],
        id_quat(),
    ));

    // Crooked iron stovepipe rising from a box on the roof, with a rain cap.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.45, 0.35, 0.45],
            0.0,
            clapboard([0.34, 0.25, 0.15]),
        )),
        [1.0, wall_top + 0.5, -0.45],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.5, 8, 0.0, iron(IRON_DARK))),
        [1.05, wall_top + 1.35, -0.45],
        quat_x(0.12),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.2, 0.08, 8, 0.0, iron(IRON_DARK))),
        [1.12, wall_top + 2.1, -0.43],
        id_quat(),
    ));

    // A tin lean-to off the side on two posts.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.15, 2.0], 0.0, tin(TIN_GREY))),
        [2.4, wall_top - 0.4, 0.0],
        quat_x(0.3),
    ));
    for pz in [-0.8_f32, 0.8] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 1.7, 0.1], 0.0, clapboard(WOOD_RAW))),
            [3.1, 0.85, pz],
            id_quat(),
        ));
    }

    // A pick left leaning in the dirt.
    prims.push(prim(
        solid(cylinder_tapered(0.05, 1.2, 6, 0.0, clapboard(WOOD_RAW))),
        [-2.0, 0.6, front_z + 0.4],
        quat_x(0.5),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.1, 0.1], 0.0, iron(IRON_DARK))),
        [-2.0, 1.1, front_z + 0.7],
        id_quat(),
    ));
    // A gold pan resting on the ground — a shallow tin basin.
    prims.push(prim(
        solid(with_cut(
            sphere(0.46, 6, tin([0.62, 0.6, 0.56])),
            [0.0, 1.0],
            [0.0, 0.55],
            0.0,
        )),
        [-2.2, 0.14, front_z + 1.0],
        id_quat(),
    ));
    // A barrel by the wall.
    prims.push(prim(
        solid(cylinder_tapered(0.3, 0.75, 10, 0.06, clapboard(WOOD_RAW))),
        [-2.1, 0.38, 0.6],
        id_quat(),
    ));
    // A heap of mine tailings.
    prims.push(prim(
        solid(cone(0.7, 0.5, 7, canvas(DUST_TAN))),
        [2.0, 0.25, front_z + 0.1],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ProspectorShack.build(""), "prospector_shack");
    }
}
