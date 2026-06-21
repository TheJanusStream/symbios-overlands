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

    // Short front stair up the platform (front = −Z), two receding treads.
    for (i, z) in [-3.1_f32, -2.6].into_iter().enumerate() {
        prims.push(prim(
            solid(cuboid_tapered(
                [2.4, 0.35, 0.6],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, 0.2 + i as f32 * 0.35, z],
            id_quat(),
        ));
    }

    let base = 1.2_f32;
    // Battered talud base course under the cella.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.9, 0.5, 3.4],
            0.12,
            limestone(STUCCO_CREAM),
        )),
        [0.0, base + 0.25, 0.0],
        id_quat(),
    ));
    // Red-stucco cella wall.
    prims.push(prim(
        solid(cuboid_tapered([3.5, 2.4, 3.0], 0.0, painted(STUCCO_RED))),
        [0.0, base + 0.5 + 1.2, 0.0],
        id_quat(),
    ));
    // Projecting tablero cornice band at the wall head.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.9, 0.45, 3.4],
            0.0,
            limestone(STUCCO_CREAM),
        )),
        [0.0, base + 0.5 + 2.4 + 0.22, 0.0],
        id_quat(),
    ));
    // Corbel-arch doorway — a tapered dark recess narrowing to the Maya
    // stepped-vault profile, on the front (−Z) face.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.3, 1.9, 0.55],
            0.5,
            painted([0.08, 0.05, 0.04]),
        )),
        [0.0, base + 0.5 + 0.95, -1.5],
        id_quat(),
    ));
    // Steep palm-thatch hip roof rising to a thatch ridge cap.
    let roof_base = base + 0.5 + 2.4 + 0.45;
    prims.push(prim(
        solid(cuboid_tapered([4.7, 1.9, 4.1], 0.55, thatch(THATCH_STRAW))),
        [0.0, roof_base + 0.95, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([2.4, 0.45, 0.7], 0.2, thatch(THATCH_STRAW))),
        [0.0, roof_base + 1.95, 0.0],
        id_quat(),
    ));
    // Low offering altar before the doorway, where the copal smokes.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.2, 0.5, 0.9],
            0.1,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, base + 0.25, -2.1],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: copal incense rising from the offering altar.
    root.children
        .push(fx::copal_smoke([0.0, base + 0.6, -2.1], 0xC0A1_5E11));
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
