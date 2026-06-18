//! Fishing shack — the Coastal-Resort *poor* landmark. A weathered
//! driftwood-plank hut on short stilts at the tide line, its sagging roof
//! patched, a drying net slung on one wall and a salt barrel by the door.
//! The hardscrabble counterpart to the [`grand_hotel`](super::grand_hotel):
//! same coast, opposite end of the prosperity axis (`Poor`), so a destitute
//! coastal room grows the fishing hamlet instead of the resort strip.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the deck floor.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DECK_WOOD, DRIFT_GREY, canvas, plank};

pub struct FishingShack;

impl CatalogueEntry for FishingShack {
    fn slug(&self) -> &'static str {
        "fishing_shack"
    }
    fn name(&self) -> &'static str {
        "Fishing Shack"
    }
    fn description(&self) -> &'static str {
        "Weathered driftwood hut on stilts with a sagging roof and a drying net."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let deck_y = 1.0_f32;
    let wall_h = 2.2_f32;
    let wall_y = deck_y + 0.15 + wall_h * 0.5;
    let wall_top = deck_y + 0.15 + wall_h;
    let net = canvas([0.46, 0.5, 0.44], [0.36, 0.4, 0.34]);

    let mut prims = vec![
        // Plank deck floor — the root, raised on stilts.
        prim(
            solid(cuboid_tapered([5.0, 0.3, 4.0], 0.0, plank(DRIFT_GREY))),
            [0.0, deck_y, 0.0],
            id_quat(),
        ),
    ];

    // Short driftwood stilts.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(0.16, deck_y, 8, 0.0, plank(DECK_WOOD))),
                [sx * 2.0, deck_y * 0.5, sz * 1.4],
                id_quat(),
            ));
        }
    }

    // Driftwood walls.
    prims.push(prim(
        solid(cuboid_tapered([4.0, wall_h, 3.0], 0.0, plank(DRIFT_GREY))),
        [0.0, wall_y, 0.0],
        id_quat(),
    ));

    // Sagging gabled plank roof.
    prims.push(prim(
        solid(cuboid_tapered([4.6, 1.2, 3.6], 0.45, plank(DRIFT_GREY))),
        [0.0, wall_top + 0.6, 0.0],
        id_quat(),
    ));

    // Plank door on the +Z face.
    prims.push(prim(
        solid(cuboid_tapered([0.8, 1.8, 0.2], 0.0, plank(DECK_WOOD))),
        [0.0, deck_y + 0.15 + 0.9, 1.5],
        id_quat(),
    ));

    // Drying net slung on the +X wall.
    prims.push(prim(
        cuboid_tapered([0.05, 1.4, 2.0], 0.0, net),
        [2.05, wall_y, 0.0],
        id_quat(),
    ));

    // Salt barrel by the door.
    prims.push(prim(
        solid(cylinder_tapered(0.4, 0.9, 10, 0.08, plank(DECK_WOOD))),
        [2.4, 0.45, 1.2],
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
        assert_sanitize_stable(&FishingShack.build(""), "fishing_shack");
    }
}
