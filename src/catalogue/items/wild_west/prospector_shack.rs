//! Prospector's shack — the Wild-West *poor* landmark. A tiny weathered
//! timber shack with a crooked stovepipe, a lean-to of tin and a pick left in
//! the dirt. The bust counterpart to the [`saloon`](super::saloon): same
//! frontier, opposite end of the prosperity axis (`Poor`), so a destitute room
//! grows the dried-up claim instead of the boomtown.
//!
//! The stovepipe and pick lean with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, iron, tin};

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

    let mut prims = vec![
        // Weathered timber walls — the root.
        prim(
            solid(cuboid_tapered([3.4, wall_h, 3.0], 0.0, clapboard(WOOD_RAW))),
            [0.0, wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Sloped tin roof.
    prims.push(prim(
        solid(cuboid_tapered([3.8, 0.2, 3.4], 0.0, tin(TIN_GREY))),
        [0.0, wall_top + 0.2, 0.0],
        quat_x(0.12),
    ));
    // Plank door + a small window opening on the front.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.8, 1.7, 0.15],
            0.0,
            clapboard([0.34, 0.26, 0.16]),
        )),
        [-0.7, 0.85, 1.5],
        id_quat(),
    ));

    // Crooked iron stovepipe.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.6, 8, 0.0, iron(IRON_DARK))),
        [1.2, wall_top + 0.7, -0.8],
        quat_x(0.18),
    ));

    // A tin lean-to off one side.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.15, 2.0], 0.0, tin(TIN_GREY))),
        [2.4, wall_top - 0.4, 0.0],
        quat_x(0.3),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.12, 1.6, 0.12], 0.0, clapboard(WOOD_RAW))),
        [3.1, 0.8, 0.8],
        id_quat(),
    ));

    // A pick left leaning in the dirt.
    prims.push(prim(
        solid(cylinder_tapered(0.05, 1.2, 6, 0.0, clapboard(WOOD_RAW))),
        [-2.0, 0.6, 1.0],
        quat_x(0.5),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.1, 0.1], 0.0, iron(IRON_DARK))),
        [-2.0, 1.1, 1.3],
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
