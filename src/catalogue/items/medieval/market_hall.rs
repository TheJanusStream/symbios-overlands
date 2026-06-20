//! Market hall — a Medieval secondary. The classic open-ground market
//! house: a stone-pillared arcade left open at street level for traders'
//! stalls, a jettied timber-framed upper floor with daub infill where the
//! guild meets, and a tiled hip roof. The covered market that anchors a
//! burgh's square.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DAUB_CREAM, SLATE_GREY, STONE_GREY, STONE_PALE, WOOD_DARK, WOOD_OAK, daub, rough_stone,
    shingle, stone, timber,
};

pub struct MarketHall;

impl CatalogueEntry for MarketHall {
    fn slug(&self) -> &'static str {
        "market_hall"
    }
    fn name(&self) -> &'static str {
        "Market Hall"
    }
    fn description(&self) -> &'static str {
        "Open stone-pillared market arcade under a jettied timber-framed upper floor."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
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
    let l = 8.0_f32; // along X
    let w = 6.0_f32; // along Z
    let foot_h = 0.3;
    let ground_h = 2.8; // open arcade height
    let deck_y = foot_h + ground_h;
    let upper_h = 2.8;

    let mut prims = vec![
        // Cobbled footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, foot_h, w + 0.6],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Open arcade: a 3×2 grid of stone pillars holding up the floor.
    for ix in [-1.0_f32, 0.0, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.55, ground_h, 0.55],
                    0.0,
                    stone(STONE_PALE),
                )),
                [
                    ix * (l * 0.5 - 0.6),
                    foot_h + ground_h * 0.5,
                    sz * (w * 0.5 - 0.6),
                ],
                id_quat(),
            ));
        }
    }

    // Timber floor deck spanning the arcade.
    prims.push(prim(
        solid(cuboid_tapered([l, 0.4, w], 0.0, timber(WOOD_OAK))),
        [0.0, deck_y + 0.2, 0.0],
        id_quat(),
    ));

    // Jettied (oversailing) daub-infilled upper storey.
    let upper_y = deck_y + 0.4 + upper_h * 0.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.8, upper_h, w + 0.8],
            0.0,
            daub(DAUB_CREAM),
        )),
        [0.0, upper_y, 0.0],
        id_quat(),
    ));
    // Exposed timber frame on the upper storey: corner posts + mid rails.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            cuboid_tapered([0.3, upper_h, 0.3], 0.0, timber(WOOD_DARK)),
            [sx * (l * 0.5 + 0.25), upper_y, sz * (w * 0.5 + 0.25)],
            id_quat(),
        ));
    }
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([l + 0.9, 0.25, 0.12], 0.0, timber(WOOD_DARK)),
            [0.0, upper_y + upper_h * 0.5 - 0.2, sz * (w * 0.5 + 0.42)],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([l + 0.9, 0.25, 0.12], 0.0, timber(WOOD_DARK)),
            [0.0, upper_y - upper_h * 0.5 + 0.2, sz * (w * 0.5 + 0.42)],
            id_quat(),
        ));
    }

    // Tiled hip roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.6, 2.4, w + 1.6],
            0.45,
            shingle(SLATE_GREY),
        )),
        [0.0, upper_y + upper_h * 0.5 + 1.2, 0.0],
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
        assert_sanitize_stable(&MarketHall.build(""), "market_hall");
    }
}
