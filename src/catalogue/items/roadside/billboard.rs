//! Billboard — a Roadside secondary. A big printed advertising panel on a
//! steel truss, a maintenance catwalk along its foot and a row of floodlights
//! washing the face. The roadside hoarding that looms over the strip.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the footing pad.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, ENAMEL_BLUE, ENAMEL_RED, PRICE_AMBER, SIGN_WHITE, STEEL_GREY, concrete, enamel,
    steel,
};

pub struct Billboard;

impl CatalogueEntry for Billboard {
    fn slug(&self) -> &'static str {
        "billboard"
    }
    fn name(&self) -> &'static str {
        "Billboard"
    }
    fn description(&self) -> &'static str {
        "Printed advertising panel on a steel truss with a catwalk and floodlights."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let foot_h = 0.4_f32;
    let board_bottom = 4.0_f32;
    let board_h = 3.6_f32;
    let board_y = board_bottom + board_h * 0.5;

    let mut prims = vec![
        // Concrete footing pad — the root.
        prim(
            solid(cuboid_tapered(
                [6.0, foot_h, 1.4],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Two vertical steel legs with back-leaning braces.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.35, board_bottom + 1.2, 0.35],
                0.0,
                steel(STEEL_GREY),
            )),
            [sx * 2.5, (board_bottom + 1.2) * 0.5, 0.0],
            id_quat(),
        ));
        // Diagonal back-brace in the Y-Z plane.
        prims.push(prim(
            solid(cuboid_tapered([0.2, 4.2, 0.2], 0.0, steel(STEEL_GREY))),
            [sx * 2.5, 2.0, -1.0],
            quat_x(0.5),
        ));
    }
    // Top cross-beam.
    prims.push(prim(
        solid(cuboid_tapered([5.6, 0.3, 0.3], 0.0, steel(STEEL_GREY))),
        [0.0, board_bottom - 0.2, 0.0],
        id_quat(),
    ));

    // Printed board face + two graphic blocks.
    prims.push(prim(
        solid(cuboid_tapered(
            [7.2, board_h, 0.25],
            0.0,
            enamel(SIGN_WHITE),
        )),
        [0.0, board_y, 0.15],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.0, 1.6, 0.1], 0.0, enamel(ENAMEL_RED)),
        [-1.6, board_y + 0.5, 0.3],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([2.6, 1.0, 0.1], 0.0, enamel(ENAMEL_BLUE)),
        [1.8, board_y - 0.6, 0.3],
        id_quat(),
    ));

    // Catwalk along the foot of the board.
    prims.push(prim(
        solid(cuboid_tapered([7.4, 0.12, 0.6], 0.0, steel(STEEL_GREY))),
        [0.0, board_bottom - 0.1, 0.5],
        id_quat(),
    ));

    // Floodlights washing the face — the billboard's emissive trim.
    for x in [-2.2_f32, 0.0, 2.2] {
        prims.push(prim(
            cuboid_tapered([0.3, 0.2, 0.3], 0.0, glow(PRICE_AMBER, 3.0)),
            [x, board_bottom + 0.1, 0.85],
            quat_x(-0.4),
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
        assert_sanitize_stable(&Billboard.build(""), "billboard");
    }
}
