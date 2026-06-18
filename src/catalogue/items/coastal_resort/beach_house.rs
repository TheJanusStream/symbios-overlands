//! Beach house — a Coastal-Resort secondary. A pastel stucco bungalow
//! raised on timber stilts above the tide line, with a railed front porch,
//! lit windows and a low gabled plank roof. The holiday let of the strip.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the deck floor.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DECK_PALE, DECK_WOOD, GLASS_AQUA, STEEL_GREY, STUCCO_SAND, glass, plank, steel, stucco,
};

pub struct BeachHouse;

impl CatalogueEntry for BeachHouse {
    fn slug(&self) -> &'static str {
        "beach_house"
    }
    fn name(&self) -> &'static str {
        "Beach House"
    }
    fn description(&self) -> &'static str {
        "Pastel stucco bungalow on stilts with a railed porch and a gabled plank roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let deck_y = 1.6_f32;
    let wall_h = 2.6_f32;
    let wall_y = deck_y + 0.15 + wall_h * 0.5;
    let wall_top = deck_y + 0.15 + wall_h;

    let mut prims = vec![
        // Plank deck floor — the root, raised on stilts.
        prim(
            solid(cuboid_tapered([7.0, 0.3, 6.0], 0.0, plank(DECK_PALE))),
            [0.0, deck_y, 0.0],
            id_quat(),
        ),
    ];

    // Timber stilts under the deck.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 0.0, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(0.18, deck_y, 8, 0.0, plank(DECK_WOOD))),
                [sx * 3.0, deck_y * 0.5, sz * 2.2],
                id_quat(),
            ));
        }
    }

    // Stucco walls.
    prims.push(prim(
        solid(cuboid_tapered([5.0, wall_h, 4.0], 0.0, stucco(STUCCO_SAND))),
        [0.0, wall_y, 0.0],
        id_quat(),
    ));

    // Low gabled plank roof.
    prims.push(prim(
        solid(cuboid_tapered([6.0, 1.6, 5.0], 0.55, plank(DECK_WOOD))),
        [0.0, wall_top + 0.8, 0.0],
        id_quat(),
    ));

    // Front door + flanking lit windows on the +Z face.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 2.0, 0.2], 0.0, plank(DECK_WOOD))),
        [0.0, deck_y + 0.15 + 1.0, 2.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([1.2, 1.2, 0.15], 0.0, glass(GLASS_AQUA, 1.2)),
            [sx * 1.6, wall_y + 0.2, 2.0],
            id_quat(),
        ));
    }

    // Railed front porch along the +Z edge of the deck.
    prims.push(prim(
        cuboid_tapered([7.0, 0.5, 0.08], 0.0, steel(STEEL_GREY)),
        [0.0, deck_y + 0.45, 3.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.7, 0.1], 0.0, steel(STEEL_GREY))),
            [sx * 3.3, deck_y + 0.35, 3.0],
            id_quat(),
        ));
    }

    // Two plank steps down off the porch.
    for (k, step) in [0.5_f32, 1.0].into_iter().enumerate() {
        let z = 3.3 + k as f32 * 0.6;
        prims.push(prim(
            solid(cuboid_tapered([2.0, 0.25, 0.6], 0.0, plank(DECK_WOOD))),
            [0.0, deck_y - step, z],
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
        assert_sanitize_stable(&BeachHouse.build(""), "beach_house");
    }
}
