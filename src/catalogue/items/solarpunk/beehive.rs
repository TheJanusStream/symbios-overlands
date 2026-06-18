//! Beehive — a Solarpunk prop. A white Langstroth hive of stacked boxes on a
//! timber stand with a landing board. Scatter clutter pollinating the
//! gardens.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STEEL_WHITE, TIMBER_WARM, steel, timber};

pub struct Beehive;

impl CatalogueEntry for Beehive {
    fn slug(&self) -> &'static str {
        "beehive"
    }
    fn name(&self) -> &'static str {
        "Beehive"
    }
    fn description(&self) -> &'static str {
        "White Langstroth hive of stacked boxes on a timber stand."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Timber stand — the root.
        prim(
            solid(cuboid_tapered([0.8, 0.4, 0.8], 0.0, timber(TIMBER_WARM))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Three stacked white hive boxes.
    for k in 0..3 {
        prims.push(prim(
            solid(cuboid_tapered([0.66, 0.4, 0.66], 0.0, steel(STEEL_WHITE))),
            [0.0, 0.6 + k as f32 * 0.42, 0.0],
            id_quat(),
        ));
    }
    // Gabled lid.
    prims.push(prim(
        solid(cuboid_tapered([0.74, 0.3, 0.74], 0.5, steel(STEEL_WHITE))),
        [0.0, 2.05, 0.0],
        id_quat(),
    ));
    // Landing board at the entrance.
    prims.push(prim(
        solid(cuboid_tapered([0.66, 0.06, 0.25], 0.0, timber(TIMBER_WARM))),
        [0.0, 0.45, 0.45],
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
        assert_sanitize_stable(&Beehive.build(""), "beehive");
    }
}
