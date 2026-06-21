//! Rice shed — a Feudal-Japan *poor* secondary. A raised storehouse
//! (takakura): a small timber granary lifted on four posts with rat-guard
//! discs, a thatch roof, and a notched-log ladder. Keeps the harvest dry
//! and out of reach beside the [`minka`](super::minka).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    PLASTER_WHITE, STONE_GREY, THATCH_STRAW, TIMBER_BROWN, TIMBER_DARK, plaster, stone, thatch,
    timber,
};

pub struct RiceShed;

impl CatalogueEntry for RiceShed {
    fn slug(&self) -> &'static str {
        "rice_shed"
    }
    fn name(&self) -> &'static str {
        "Rice Shed"
    }
    fn description(&self) -> &'static str {
        "Raised timber granary on stilts with rat-guards under a thatch roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let post_h = 2.0;
    let box_y = post_h + 1.0;

    let mut prims = vec![
        // Stone pad — the root.
        prim(
            solid(cuboid_tapered([3.4, 0.3, 2.8], 0.0, stone(STONE_GREY))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Four posts with rat-guard discs.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let x = sx * 1.2;
        let z = sz * 0.9;
        prims.push(prim(
            solid(cylinder_tapered(0.14, post_h, 8, 0.0, timber(TIMBER_DARK))),
            [x, 0.3 + post_h * 0.5, z],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(0.4, 0.12, 12, 0.1, timber(TIMBER_BROWN))),
            [x, 0.3 + post_h, z],
            id_quat(),
        ));
    }

    // Raised granary body.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 2.0, 2.4],
            0.05,
            plaster(PLASTER_WHITE),
        )),
        [0.0, box_y, 0.0],
        id_quat(),
    ));
    // Timber corner battens.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 2.0, 0.2], 0.0, timber(TIMBER_DARK))),
            [sx * 1.45, box_y, sz * 1.15],
            id_quat(),
        ));
    }

    // Small raised granary door on the −Z front (hero face), reached by the
    // ladder.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.0, 0.15], 0.0, timber(TIMBER_DARK))),
        [0.0, box_y - 0.4, -1.28],
        id_quat(),
    ));

    // Steep pyramidal thatch roof.
    prims.push(prim(
        solid(cuboid_tapered([3.8, 1.9, 3.2], 0.7, thatch(THATCH_STRAW))),
        [0.0, box_y + 1.95, 0.0],
        id_quat(),
    ));

    // Notched-log ladder leaning against the front door.
    prims.push(prim(
        solid(cylinder_tapered(0.1, 2.8, 6, 0.0, timber(TIMBER_BROWN))),
        [0.0, box_y - 0.7, -1.7],
        quat_x(-0.4),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RiceShed.build(""), "rice_shed");
    }
}
