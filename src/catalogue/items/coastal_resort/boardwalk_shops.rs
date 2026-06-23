//! Boardwalk shops — a Coastal-Resort secondary. A short plank boardwalk
//! lined with three little stucco kiosks under striped awnings, their lit
//! glass fronts facing the strollers: the ice-cream, postcard and beach-
//! tat stalls of the promenade.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the boardwalk deck.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_TEAL, AWNING_WHITE, DECK_PALE, DECK_WOOD, GLASS_AQUA, SIGN_AMBER, SIGN_GOLD,
    STEEL_GREY, STUCCO_WHITE, canvas, glass, plank, steel, stucco,
};

pub struct BoardwalkShops;

impl CatalogueEntry for BoardwalkShops {
    fn slug(&self) -> &'static str {
        "boardwalk_shops"
    }
    fn name(&self) -> &'static str {
        "Boardwalk Shops"
    }
    fn description(&self) -> &'static str {
        "A plank boardwalk of three awninged kiosks with lit glass fronts."
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
            clearance: 7.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let deck_top = 0.525_f32;
    let wall_h = 2.8_f32;
    let wall_y = deck_top + wall_h * 0.5;

    let mut prims = vec![
        // Plank boardwalk deck — the root.
        prim(
            solid(cuboid_tapered([12.0, 0.25, 5.0], 0.0, plank(DECK_PALE))),
            [0.0, 0.4, 0.0],
            id_quat(),
        ),
    ];

    // The strollers' side faces the -Z render front: shopfronts, counters,
    // awnings and signage all face -Z; the back railing rings the +Z edge.
    let stripes = [AWNING_RED, AWNING_TEAL, AWNING_RED];
    for (i, x) in [-4.0_f32, 0.0, 4.0].into_iter().enumerate() {
        // Stucco kiosk box, set back so its front opens onto the deck.
        prims.push(prim(
            solid(cuboid_tapered(
                [3.2, wall_h, 3.2],
                0.0,
                stucco(STUCCO_WHITE),
            )),
            [x, wall_y, 0.4],
            id_quat(),
        ));
        // Service counter / bulkhead under the open shopfront.
        prims.push(prim(
            solid(cuboid_tapered([2.8, 1.0, 0.34], 0.0, plank(DECK_WOOD))),
            [x, deck_top + 0.5, -1.15],
            id_quat(),
        ));
        // Lit glass shopfront on the -Z face, above the counter.
        prims.push(prim(
            cuboid_tapered([2.6, 1.4, 0.15], 0.0, glass(GLASS_AQUA, 1.3)),
            [x, deck_top + 1.7, -1.2],
            id_quat(),
        ));
        // Striped awning over the front, leading edge dropping toward shore.
        prims.push(prim(
            cuboid_tapered([3.4, 0.18, 1.5], 0.0, canvas(stripes[i], AWNING_WHITE)),
            [x, deck_top + 2.4, -1.6],
            quat_x(-0.3),
        ));
        // Plank sign board above the awning with a deep-amber lit name strip.
        prims.push(prim(
            solid(cuboid_tapered([2.6, 0.5, 0.12], 0.0, plank(DECK_WOOD))),
            [x, deck_top + 2.9, -0.9],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([2.0, 0.3, 0.06], 0.0, glow(SIGN_AMBER, 2.0)),
            [x, deck_top + 2.9, -0.97],
            id_quat(),
        ));
    }

    // A warm lit lamp slung over the middle stall.
    prims.push(prim(
        cuboid_tapered([0.4, 0.4, 0.2], 0.0, glow(SIGN_GOLD, 2.4)),
        [0.0, deck_top + 3.3, -1.15],
        id_quat(),
    ));

    // Steel railing along the back (+Z) edge of the boardwalk.
    prims.push(prim(
        cuboid_tapered([12.0, 0.1, 0.08], 0.0, steel(STEEL_GREY)),
        [0.0, deck_top + 0.7, 2.4],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.8, 0.1], 0.0, steel(STEEL_GREY))),
            [sx * 5.6, deck_top + 0.4, 2.4],
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
        assert_sanitize_stable(&BoardwalkShops.build(""), "boardwalk_shops");
    }
}
