//! Community center — the Suburban landmark. A long single-storey civic hall
//! with a brick base and rendered walls under a low shingle roof, fronted by
//! a white-columned portico and a lit sign, with a flag pole and foundation
//! shrubs on the lawn. Birdsong drifts over it and a sprinkler mists the
//! grass. The modest civic heart of the neighbourhood.

use crate::catalogue::items::civic_campus::column;
use crate::catalogue::items::roadside::{SIGN_AMBER, sign_board};
use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, GLASS_TINT, HEDGE_GREEN, RENDER_WHITE, ROOF_GREY, SIDING_BLUE, SIGN_GLOW,
    WOOD_WHITE, brick, enamel, fx, glass, render, shingle, wood,
};

pub struct CommunityCenter;

impl CatalogueEntry for CommunityCenter {
    fn slug(&self) -> &'static str {
        "community_center"
    }
    fn name(&self) -> &'static str {
        "Community Center"
    }
    fn description(&self) -> &'static str {
        "Low civic hall with a columned portico, lit sign, flag pole, and lawn."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 12.0,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 18.0_f32;
    let w = 12.0_f32;
    let base_h = 0.5;
    let brick_h = 1.4;
    let wall_h = 3.6;
    let wall_top = base_h + brick_h + wall_h;
    // Hero face on the -Z front (the render's lead tile): the portico, window
    // band, and lit sign read straight on instead of hiding round the back.
    let front = -w * 0.5;

    let mut prims = vec![
        // Concrete footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 1.0, base_h, w + 1.0],
                0.0,
                render([0.6, 0.6, 0.6]),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Brick base course.
        prim(
            solid(cuboid_tapered([l, brick_h, w], 0.0, brick(BRICK_TAN))),
            [0.0, base_h + brick_h * 0.5, 0.0],
            id_quat(),
        ),
        // Rendered upper walls.
        prim(
            solid(cuboid_tapered(
                [l - 0.4, wall_h, w - 0.4],
                0.0,
                render(RENDER_WHITE),
            )),
            [0.0, base_h + brick_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Window band along the front (-Z), proud of the wall.
    for c in 0..5 {
        let x = -l * 0.5 + 2.4 + c as f32 * (l - 4.8) / 4.0;
        prims.push(prim(
            cuboid_tapered([1.6, 1.8, 0.2], 0.0, glass(GLASS_TINT, 0.6)),
            [x, base_h + brick_h + 1.6, front - 0.1],
            id_quat(),
        ));
    }

    // Low shingle hip roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 2.0, 2.4, w + 2.0],
            0.45,
            shingle(ROOF_GREY),
        )),
        [0.0, wall_top + 1.2, 0.0],
        id_quat(),
    ));

    // Entrance portico: four classical columns and an entablature beam.
    for x in [-4.0_f32, -1.4, 1.4, 4.0] {
        prims.extend(column(
            x,
            front - 2.2,
            base_h,
            wall_h + brick_h,
            0.3,
            wood(WOOD_WHITE),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([9.4, 0.6, 3.0], 0.0, wood(WOOD_WHITE))),
        [0.0, base_h + wall_h + brick_h + 0.3, front - 2.0],
        id_quat(),
    ));
    // Lit sign over the entrance — segmented so it reads lit, not washed.
    prims.extend(sign_board(
        [0.0, base_h + brick_h + 2.6, front - 0.12],
        [6.0, 0.9],
        (4, 1),
        SIGN_AMBER,
        2.4,
        -1.0,
    ));

    // Flag pole with a finial and a small flag.
    let pole_x = -l * 0.5 - 1.5;
    prims.push(prim(
        solid(cylinder_tapered(0.1, 8.0, 8, 0.1, enamel([0.8, 0.8, 0.82]))),
        [pole_x, 4.0, front],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(0.18, 3, glow(SIGN_GLOW, 2.0))),
        [pole_x, 8.1, front],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.05, 0.8, 1.3], 0.0, enamel(SIDING_BLUE)),
        [pole_x, 7.2, front - 0.7],
        id_quat(),
    ));

    // Clipped foundation shrubs along the front lawn — leafy clumps, not slabs.
    prims.extend(crop_tufts(
        [-1.0, base_h, front - 0.9],
        [l * 0.7, 1.2],
        6,
        1,
        1.0,
        foliage(HEDGE_GREEN),
    ));

    let mut root = assemble(prims);
    // Signature life: birdsong over the lawn and a sprinkler misting it.
    root.audio = fx::birdsong();
    root.children
        .push(fx::sprinkler_mist([6.0, 0.4, front - 5.0], 0x5B19_DA11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CommunityCenter.build(""), "community_center");
    }

    #[test]
    fn has_lit_sign() {
        assert!(crate::catalogue::items::util::has_emissive(
            &CommunityCenter.build("")
        ));
    }
}
