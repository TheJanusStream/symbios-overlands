//! Rubble barricade — a Post-apocalyptic *poor* secondary. A crude wall heaped
//! from broken concrete, scrap and a jammed-in tyre. The makeshift defence of
//! the drifter's camp.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, STEEL_GREY, TIRE_BLACK, concrete, rusted, tarp};

pub struct RubbleBarricade;

impl CatalogueEntry for RubbleBarricade {
    fn slug(&self) -> &'static str {
        "rubble_barricade"
    }
    fn name(&self) -> &'static str {
        "Rubble Barricade"
    }
    fn description(&self) -> &'static str {
        "Crude wall heaped from broken concrete, scrap and a jammed-in tyre."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Heaped concrete core — the root.
        prim(
            solid(cuboid_tapered(
                [4.0, 1.3, 1.2],
                0.3,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.65, 0.0],
            id_quat(),
        ),
    ];

    // Broken concrete chunks piled on top.
    for (cx, s) in [(-1.3_f32, 0.8_f32), (0.4, 0.9), (1.4, 0.7)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [s, s * 0.8, s],
                0.3,
                concrete(CONCRETE_GREY),
            )),
            [cx, 1.3 + s * 0.3, 0.1],
            id_quat(),
        ));
    }
    // A leaning scrap plate.
    prims.push(prim(
        solid(cuboid_tapered([1.2, 1.6, 0.12], 0.0, rusted(STEEL_GREY))),
        [-1.8, 1.0, 0.3],
        quat_x(0.2),
    ));
    // A tyre jammed into the heap.
    prims.push(prim(
        solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
        [1.6, 1.5, 0.2],
        quat_x(1.4),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RubbleBarricade.build(""), "rubble_barricade");
    }
}
