//! Suburban house — a Suburban secondary. A two-storey family home in lap
//! siding with a shingle roof, an attached garage, a small covered porch lit
//! by a warm porch light, and a brick chimney. The building the
//! neighbourhood is made of.

use crate::catalogue::items::civic_campus::column;
use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, CAR_SILVER, GLASS_TINT, HEDGE_GREEN, PORCH_WARM, ROOF_GREY, SIDING_CREAM,
    WOOD_WHITE, brick, enamel, glass, parked_car, render, shingle, siding, wood,
};

pub struct SuburbanHouse;

impl CatalogueEntry for SuburbanHouse {
    fn slug(&self) -> &'static str {
        "suburban_house"
    }
    fn name(&self) -> &'static str {
        "Suburban House"
    }
    fn description(&self) -> &'static str {
        "Two-storey sided family house with an attached garage and a lit porch."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.4;
    let body_w = 10.0_f32;
    let body_d = 8.0_f32;
    let body_h = 6.0_f32;
    // Hero face (porch, door, windows, lit porch light) on the -Z front.
    let front = -body_d * 0.5;

    let mut prims = vec![
        // Concrete footing + driveway — the root.
        prim(
            solid(cuboid_tapered(
                [body_w + 5.5, base_h, body_d + 1.0],
                0.0,
                render([0.55, 0.55, 0.56]),
            )),
            [1.5, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Main siding body.
        prim(
            solid(cuboid_tapered(
                [body_w, body_h, body_d],
                0.0,
                siding(SIDING_CREAM),
            )),
            [0.0, base_h + body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Attached garage.
    let g_w = 5.0_f32;
    let g_h = 3.2_f32;
    let g_x = body_w * 0.5 + g_w * 0.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [g_w, g_h, body_d * 0.7],
            0.0,
            siding(SIDING_CREAM),
        )),
        [g_x, base_h + g_h * 0.5, 0.6],
        id_quat(),
    ));
    // Garage door on the -Z front.
    prims.push(prim(
        cuboid_tapered([g_w - 1.0, g_h - 0.6, 0.2], 0.0, enamel([0.82, 0.82, 0.80])),
        [g_x, base_h + (g_h - 0.6) * 0.5, 0.6 - body_d * 0.35 - 0.1],
        id_quat(),
    ));

    // Shingle hip roofs.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 1.4, 2.6, body_d + 1.4],
            0.5,
            shingle(ROOF_GREY),
        )),
        [0.0, base_h + body_h + 1.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [g_w + 0.8, 1.4, body_d * 0.7 + 0.8],
            0.45,
            shingle(ROOF_GREY),
        )),
        [g_x, base_h + g_h + 0.7, 0.6],
        id_quat(),
    ));

    // Windows on the main body front (-Z), proud of the siding.
    for (wx, wy) in [
        (-2.6_f32, 2.0_f32),
        (2.6, 2.0),
        (-2.6, 4.6),
        (2.6, 4.6),
        (0.0, 4.6),
    ] {
        prims.push(prim(
            cuboid_tapered([1.4, 1.4, 0.2], 0.0, glass(GLASS_TINT, 0.5)),
            [wx, base_h + wy, front - 0.1],
            id_quat(),
        ));
    }

    // Covered porch: two classical posts, a roof, the door, and a warm light.
    for x in [-1.6_f32, 1.6] {
        prims.extend(column(x, front - 1.6, base_h, 2.8, 0.13, wood(WOOD_WHITE)));
    }
    prims.push(prim(
        solid(cuboid_tapered([4.2, 0.3, 2.0], 0.0, shingle(ROOF_GREY))),
        [0.0, base_h + 2.9, front - 1.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.2, 2.2, 0.15], 0.0, wood(WOOD_WHITE))),
        [0.0, base_h + 1.1, front - 0.05],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.3, 0.4, 0.2], 0.0, glow(PORCH_WARM, 3.0)),
        [1.0, base_h + 2.2, front - 0.12],
        id_quat(),
    ));

    // Brick chimney (back corner).
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, body_h + 1.5, 0.9],
            0.0,
            brick(BRICK_TAN),
        )),
        [-body_w * 0.5 + 0.6, base_h + (body_h + 1.5) * 0.5, 1.5],
        id_quat(),
    ));

    // A car on the driveway — round wheels, glazed cabin.
    prims.extend(parked_car([g_x, base_h, front - 3.0], CAR_SILVER));

    // Clipped foundation hedge along the front lawn, beside the porch.
    prims.extend(crop_tufts(
        [-3.5, base_h, front - 0.7],
        [4.5, 1.0],
        4,
        1,
        0.8,
        foliage(HEDGE_GREEN),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SuburbanHouse.build(""), "suburban_house");
    }
}
