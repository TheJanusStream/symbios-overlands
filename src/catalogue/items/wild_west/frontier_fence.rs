//! Frontier fence — a Wild-West prop. A section of split-rail fence: rough
//! timber rails slotted through stout posts. Scatter clutter bounding the lots.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_RAW, clapboard};

pub struct FrontierFence;

impl CatalogueEntry for FrontierFence {
    fn slug(&self) -> &'static str {
        "frontier_fence"
    }
    fn name(&self) -> &'static str {
        "Frontier Fence"
    }
    fn description(&self) -> &'static str {
        "Section of split-rail fence: rough timber rails slotted through stout posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Weathered posts of slightly uneven height along the run.
    let posts = [(-2.4_f32, 1.5_f32), (-0.8, 1.24), (0.8, 1.48), (2.4, 1.16)];

    let mut prims = vec![
        // First post — the root.
        prim(
            solid(cuboid_tapered(
                [0.18, posts[0].1, 0.18],
                0.18,
                clapboard(WOOD_RAW),
            )),
            [posts[0].0, posts[0].1 * 0.5, 0.0],
            id_quat(),
        ),
    ];
    for &(x, h) in &posts[1..] {
        prims.push(prim(
            solid(cuboid_tapered([0.18, h, 0.18], 0.18, clapboard(WOOD_RAW))),
            [x, h * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Three rough-hewn split rails (hexagonal logs) threaded zigzag through
    // the posts — round logs read as split rails where flat boards did not.
    for (y, zoff) in [(0.45_f32, 0.13_f32), (0.92, -0.13), (1.32, 0.13)] {
        prims.push(prim(
            solid(cylinder_tapered(0.07, 5.1, 6, 0.0, clapboard(WOOD_RAW))),
            [0.0, y, zoff],
            quat_z(FRAC_PI_2),
        ));
    }
    // A leaning brace pole propping the end post along the run.
    prims.push(prim(
        solid(cylinder_tapered(0.06, 1.7, 6, 0.0, clapboard(WOOD_RAW))),
        [2.65, 0.7, 0.0],
        quat_z(0.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FrontierFence.build(""), "frontier_fence");
    }
}
