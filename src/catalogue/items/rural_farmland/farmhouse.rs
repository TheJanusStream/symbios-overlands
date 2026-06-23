//! Farmhouse — a Rural/Farmland secondary. A two-storey clapboard house on a
//! fieldstone foundation with a shingle roof, a covered front porch lit by a
//! warm porch light, and a stone chimney trailing hearth smoke.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLAPBOARD_CREAM, GLASS_TINT, LAMP_WARM, ROOF_GREY, STONE_GREY, TRIM_WHITE, clapboard, fx,
    glass, shingle, stone,
};

pub struct Farmhouse;

impl CatalogueEntry for Farmhouse {
    fn slug(&self) -> &'static str {
        "farmhouse"
    }
    fn name(&self) -> &'static str {
        "Farmhouse"
    }
    fn description(&self) -> &'static str {
        "Two-storey clapboard farmhouse with a covered porch and a smoking chimney."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 10.0_f32;
    let w = 8.0_f32;
    let foot_h = 0.5;
    let body_h = 6.0;
    let wall_top = foot_h + body_h;
    let front = w * 0.5;

    let mut prims = vec![
        // Fieldstone foundation — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.8, foot_h, w + 0.8],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Clapboard body.
        prim(
            solid(cuboid_tapered(
                [l, body_h, w],
                0.0,
                clapboard(CLAPBOARD_CREAM),
            )),
            [0.0, foot_h + body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Shingle hip roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.5, 3.0, w + 1.5],
            0.55,
            shingle(ROOF_GREY),
        )),
        [0.0, wall_top + 1.5, 0.0],
        id_quat(),
    ));

    // Hero face toward the render front (−Z); proud trim sits a touch further
    // toward −Z so nothing is coplanar with the wall.
    let f = -front;

    // Windows with white surrounds, on the −Z front.
    for (wx, wy) in [
        (-2.8_f32, 2.0_f32),
        (2.8, 2.0),
        (-2.8, 4.4),
        (2.8, 4.4),
        (0.0, 4.4),
    ] {
        prims.push(prim(
            cuboid_tapered([1.5, 1.7, 0.1], 0.0, clapboard(TRIM_WHITE)),
            [wx, foot_h + wy, f - 0.04],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([1.3, 1.5, 0.2], 0.0, glass(GLASS_TINT, 0.4)),
            [wx, foot_h + wy, f],
            id_quat(),
        ));
    }

    // Covered front porch: deck, turned posts, railing, shed roof, and steps.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.6, 0.2, 2.6],
            0.0,
            clapboard([0.5, 0.46, 0.4]),
        )),
        [0.0, foot_h + 0.1, f - 1.3],
        id_quat(),
    ));
    for px in [-4.0_f32, -1.3, 1.3, 4.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.13, 2.8, 8, 0.0, clapboard(TRIM_WHITE))),
            [px, foot_h + 1.4, f - 1.8],
            id_quat(),
        ));
    }
    // Railing between the outer post pairs (the central bay is the entry).
    for rx in [-2.65_f32, 2.65] {
        prims.push(prim(
            cuboid_tapered([2.3, 0.12, 0.1], 0.0, clapboard(TRIM_WHITE)),
            [rx, foot_h + 1.0, f - 1.8],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([l + 0.5, 0.3, 2.4], 0.0, shingle(ROOF_GREY))),
        [0.0, foot_h + 2.9, f - 1.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [1.2, 2.2, 0.15],
            0.0,
            clapboard([0.5, 0.34, 0.2]),
        )),
        [0.0, foot_h + 1.1, f - 0.05],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.3, 0.4, 0.2], 0.0, glow(LAMP_WARM, 3.0)),
        [1.0, foot_h + 2.2, f - 0.1],
        id_quat(),
    ));
    // Stone entry steps down off the deck.
    for (sy, sz, sw) in [(0.0_f32, 0.45_f32, 0.4_f32), (-0.15, 0.72, 0.55)] {
        prims.push(prim(
            solid(cuboid_tapered([2.0, 0.2, sw], 0.0, stone(STONE_GREY))),
            [0.0, foot_h + 0.1 + sy, f - 1.3 - sz],
            id_quat(),
        ));
    }

    // Stone chimney.
    let chimney_x = -l * 0.5 + 0.7;
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, body_h + 2.0, 0.9],
            0.0,
            stone(STONE_GREY),
        )),
        [chimney_x, foot_h + (body_h + 2.0) * 0.5, -1.5],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: hearth smoke from the chimney.
    root.children.push(fx::chimney_smoke(
        [chimney_x, wall_top + 2.2, -1.5],
        0x0FA1_5E11,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Farmhouse.build(""), "farmhouse");
    }
}
