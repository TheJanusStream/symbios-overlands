//! Rail fence — a Rural/Farmland prop. A weathered post-and-rail fence: a few
//! squared posts carrying two split rails, the boundary of a paddock.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_GREY, weathered};

pub struct RailFence;

impl CatalogueEntry for RailFence {
    fn slug(&self) -> &'static str {
        "rail_fence"
    }
    fn name(&self) -> &'static str {
        "Rail Fence"
    }
    fn description(&self) -> &'static str {
        "Weathered post-and-rail paddock fence."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 4.5_f32;
    let post_h = 1.3;

    // Lower rail — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([span, 0.14, 0.1], 0.0, weathered(WOOD_GREY))),
        [0.0, 0.5, 0.0],
        id_quat(),
    )];
    // Upper rail.
    prims.push(prim(
        solid(cuboid_tapered([span, 0.14, 0.1], 0.0, weathered(WOOD_GREY))),
        [0.0, 1.05, 0.0],
        id_quat(),
    ));

    // Posts.
    for x in [
        -span * 0.5 + 0.2,
        -span * 0.18,
        span * 0.18,
        span * 0.5 - 0.2,
    ] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.16, post_h, 0.16],
                0.0,
                weathered(WOOD_GREY),
            )),
            [x, post_h * 0.5, 0.0],
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
        assert_sanitize_stable(&RailFence.build(""), "rail_fence");
    }
}
