//! Shrine — a Mesoamerican secondary. A small temple on a stepped limestone
//! platform: a red-stuccoed cella with a dark doorway under a palm-thatch
//! roof, copal incense smoking from the threshold. A neighbourhood place of
//! offering beneath the great pyramid.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    LIMESTONE_PALE, STUCCO_CREAM, STUCCO_RED, THATCH_STRAW, fx, limestone, painted, thatch,
};

pub struct Shrine;

impl CatalogueEntry for Shrine {
    fn slug(&self) -> &'static str {
        "shrine"
    }
    fn name(&self) -> &'static str {
        "Shrine"
    }
    fn description(&self) -> &'static str {
        "Red-stucco cella on a stepped platform under a thatch roof, smoking with copal."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Lower platform step — the root.
        prim(
            solid(cuboid_tapered(
                [6.0, 0.6, 6.0],
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, 0.3, 0.0],
            id_quat(),
        ),
        // Upper platform step.
        prim(
            solid(cuboid_tapered(
                [4.8, 0.6, 4.8],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
    ];

    let base = 1.2_f32;
    // Red-stucco cella.
    prims.push(prim(
        solid(cuboid_tapered([3.5, 2.6, 3.0], 0.0, painted(STUCCO_RED))),
        [0.0, base + 1.3, 0.0],
        id_quat(),
    ));
    // Dark doorway at the front.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.2, 1.8, 0.4],
            0.0,
            painted([0.1, 0.06, 0.05]),
        )),
        [0.0, base + 0.9, 1.4],
        id_quat(),
    ));
    // Palm-thatch hip roof.
    prims.push(prim(
        solid(cuboid_tapered([4.4, 1.5, 3.8], 0.45, thatch(THATCH_STRAW))),
        [0.0, base + 2.6 + 0.55, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: copal incense rising from the doorway.
    root.children
        .push(fx::copal_smoke([0.0, base + 0.4, 1.7], 0xC0A1_5E11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Shrine.build(""), "shrine");
    }
}
