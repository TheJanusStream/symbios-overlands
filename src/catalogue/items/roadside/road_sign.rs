//! Road sign — a Roadside prop. A green highway guide panel on twin steel
//! posts, white-bordered with a blank legend block. Scatter clutter for the
//! shoulder.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ROAD_GREEN, SIGN_WHITE, STEEL_GREY, enamel, steel};

pub struct RoadSign;

impl CatalogueEntry for RoadSign {
    fn slug(&self) -> &'static str {
        "road_sign"
    }
    fn name(&self) -> &'static str {
        "Road Sign"
    }
    fn description(&self) -> &'static str {
        "Green highway guide panel on twin steel posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
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
        // Left post — the root.
        prim(
            solid(cuboid_tapered([0.12, 2.8, 0.12], 0.0, steel(STEEL_GREY))),
            [-0.7, 1.4, 0.0],
            id_quat(),
        ),
    ];
    // Right post.
    prims.push(prim(
        solid(cuboid_tapered([0.12, 2.8, 0.12], 0.0, steel(STEEL_GREY))),
        [0.7, 1.4, 0.0],
        id_quat(),
    ));

    // Green panel with a white border and a blank legend block.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 1.1, 0.1], 0.0, enamel(ROAD_GREEN))),
        [0.0, 2.4, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([2.5, 1.2, 0.06], 0.0, enamel(SIGN_WHITE)),
        [0.0, 2.4, -0.04],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.6, 0.4, 0.12], 0.0, enamel(SIGN_WHITE)),
        [0.0, 2.4, 0.06],
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
        assert_sanitize_stable(&RoadSign.build(""), "road_sign");
    }
}
