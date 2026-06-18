//! Stone cross — a Gothic-Horror prop. A weathered ringed cross on a stepped
//! base, lichened with age. Scatter clutter marking the graves.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_MOSS, mossy};

pub struct StoneCross;

impl CatalogueEntry for StoneCross {
    fn slug(&self) -> &'static str {
        "stone_cross"
    }
    fn name(&self) -> &'static str {
        "Stone Cross"
    }
    fn description(&self) -> &'static str {
        "Weathered ringed cross on a stepped base, lichened with age."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
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
        // Lower step — the root.
        prim(
            solid(cuboid_tapered([1.4, 0.3, 1.4], 0.0, mossy(STONE_MOSS))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];
    // Upper step.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.3, 1.0], 0.0, mossy(STONE_MOSS))),
        [0.0, 0.45, 0.0],
        id_quat(),
    ));

    // Shaft.
    prims.push(prim(
        solid(cuboid_tapered([0.3, 2.6, 0.3], 0.05, mossy(STONE_MOSS))),
        [0.0, 1.9, 0.0],
        id_quat(),
    ));
    // Cross arm.
    prims.push(prim(
        solid(cuboid_tapered([1.3, 0.3, 0.28], 0.0, mossy(STONE_MOSS))),
        [0.0, 2.7, 0.0],
        id_quat(),
    ));
    // Celtic ring at the crossing.
    prims.push(prim(
        solid(torus(0.1, 0.45, mossy(STONE_MOSS))),
        [0.0, 2.7, 0.0],
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
        assert_sanitize_stable(&StoneCross.build(""), "stone_cross");
    }
}
