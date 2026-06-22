//! Beehive — a Solarpunk prop. A white Langstroth hive of stacked boxes on a
//! timber stand with a landing board. Scatter clutter pollinating the
//! gardens.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STEEL_GREY, STEEL_WHITE, TIMBER_WARM, foliage, steel, timber};

/// Dark mouth of the hive entrance.
const ENTRANCE_DARK: [f32; 3] = [0.12, 0.1, 0.08];

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

    // Three stacked white hive boxes (supers), each ringed by a proud
    // handhold band so it reads as a real Langstroth box, not a plain block.
    for k in 0..3 {
        let y = 0.6 + k as f32 * 0.42;
        prims.push(prim(
            solid(cuboid_tapered([0.66, 0.4, 0.66], 0.0, steel(STEEL_WHITE))),
            [0.0, y, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.7, 0.07, 0.7], 0.0, steel(STEEL_GREY))),
            [0.0, y + 0.1, 0.0],
            id_quat(),
        ));
    }
    // Flat telescoping lid seated on the top box, with a low timber cap.
    prims.push(prim(
        solid(cuboid_tapered([0.78, 0.12, 0.78], 0.0, steel(STEEL_WHITE))),
        [0.0, 1.7, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.12, 0.6], 0.4, timber(TIMBER_WARM))),
        [0.0, 1.82, 0.0],
        id_quat(),
    ));
    // Dark entrance slot + landing board on the −Z hero front.
    prims.push(prim(
        cuboid_tapered([0.42, 0.06, 0.05], 0.0, foliage(ENTRANCE_DARK)),
        [0.0, 0.46, -0.34],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.05, 0.26], 0.0, timber(TIMBER_WARM))),
        [0.0, 0.43, -0.46],
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
