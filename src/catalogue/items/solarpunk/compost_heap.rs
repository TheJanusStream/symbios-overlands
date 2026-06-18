//! Compost heap — a Solarpunk *poor* prop. A timber pallet bin heaped with
//! rotting compost and green scraps. The humble nutrient cycle of the
//! grassroots commune.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CROP_GREEN, TIMBER_WARM, foliage, timber};

/// Dark rotting brown of the compost.
const COMPOST: [f32; 3] = [0.30, 0.24, 0.16];

pub struct CompostHeap;

impl CatalogueEntry for CompostHeap {
    fn slug(&self) -> &'static str {
        "compost_heap"
    }
    fn name(&self) -> &'static str {
        "Compost Heap"
    }
    fn description(&self) -> &'static str {
        "Timber pallet bin heaped with rotting compost and green scraps."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Back pallet wall — the root.
        prim(
            solid(cuboid_tapered([1.4, 1.0, 0.1], 0.0, timber(TIMBER_WARM))),
            [0.0, 0.5, -0.65],
            id_quat(),
        ),
    ];

    // Two side pallet walls.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 1.0, 1.4], 0.0, timber(TIMBER_WARM))),
            [sx * 0.65, 0.5, 0.0],
            id_quat(),
        ));
    }

    // Heaped compost mound.
    prims.push(prim(
        solid(cuboid_tapered([1.2, 0.9, 1.2], 0.4, foliage(COMPOST))),
        [0.0, 0.5, 0.0],
        id_quat(),
    ));
    // Green scraps tossed on top.
    prims.push(prim(
        solid(cuboid_tapered([0.8, 0.3, 0.8], 0.3, foliage(CROP_GREEN))),
        [0.1, 0.95, 0.05],
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
        assert_sanitize_stable(&CompostHeap.build(""), "compost_heap");
    }
}
