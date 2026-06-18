//! Frontier fence — a Wild-West prop. A section of split-rail fence: rough
//! timber rails slotted through stout posts. Scatter clutter bounding the lots.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
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
    let mut prims = vec![
        // First post — the root.
        prim(
            solid(cuboid_tapered([0.2, 1.4, 0.2], 0.0, clapboard(WOOD_RAW))),
            [-1.8, 0.7, 0.0],
            id_quat(),
        ),
    ];
    // More posts along the run.
    for x in [0.0_f32, 1.8] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 1.4, 0.2], 0.0, clapboard(WOOD_RAW))),
            [x, 0.7, 0.0],
            id_quat(),
        ));
    }
    // Two rough rails threaded through.
    for y in [0.5_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([3.8, 0.14, 0.12], 0.0, clapboard(WOOD_RAW))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

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
