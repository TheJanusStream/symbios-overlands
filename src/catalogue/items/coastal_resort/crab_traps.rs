//! Crab traps — a Coastal-Resort *poor* prop. A leaning stack of wire-frame
//! crab pots with bright net floats and a coil of rope: the working clutter
//! of the fishing hamlet's quay.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{AWNING_WHITE, BUOY_RED, DECK_WOOD, STEEL_GREY, enamel, plank, steel};

pub struct CrabTraps;

impl CatalogueEntry for CrabTraps {
    fn slug(&self) -> &'static str {
        "crab_traps"
    }
    fn name(&self) -> &'static str {
        "Crab Traps"
    }
    fn description(&self) -> &'static str {
        "A leaning stack of wire crab pots with net floats and a rope coil."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Bottom trap — the root.
        prim(
            solid(cuboid_tapered([0.7, 0.5, 0.7], 0.0, steel(STEEL_GREY))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
    ];

    // Two more traps stacked and offset.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.5, 0.7], 0.0, steel(STEEL_GREY))),
        [0.12, 0.75, 0.06],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.5, 0.7], 0.0, steel(STEEL_GREY))),
        [-0.62, 0.25, 0.32],
        id_quat(),
    ));

    // Bright net floats perched on the stack.
    for (pos, color) in [
        ([0.12_f32, 1.08, 0.06], BUOY_RED),
        ([-0.1, 1.08, -0.2], AWNING_WHITE),
        ([-0.62, 0.58, 0.32], BUOY_RED),
    ] {
        prims.push(prim(solid(sphere(0.16, 3, enamel(color))), pos, id_quat()));
    }

    // Coil of rope on the ground.
    prims.push(prim(
        torus(0.07, 0.4, plank(DECK_WOOD)),
        [0.8, 0.07, -0.5],
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
        assert_sanitize_stable(&CrabTraps.build(""), "crab_traps");
    }
}
