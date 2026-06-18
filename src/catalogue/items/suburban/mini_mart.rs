//! Mini-mart — a Suburban secondary. A small convenience store: a rendered
//! box on a brick base with a glazed storefront under a lit fascia sign, a
//! flat parapet roof with an AC unit, and a tall lit pole sign at the kerb.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, CAR_SILVER, GLASS_TINT, RENDER_WHITE, SIGN_GLOW, brick, enamel, glass, render,
};

pub struct MiniMart;

impl CatalogueEntry for MiniMart {
    fn slug(&self) -> &'static str {
        "mini_mart"
    }
    fn name(&self) -> &'static str {
        "Mini-Mart"
    }
    fn description(&self) -> &'static str {
        "Small convenience store with a glazed storefront and a lit pole sign."
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
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 10.0_f32;
    let d = 8.0_f32;
    let base_h = 0.4;
    let brick_h = 1.0;
    let body_h = 4.0;
    let front = d * 0.5;

    let mut prims = vec![
        // Asphalt-grey lot slab — the root.
        prim(
            solid(cuboid_tapered(
                [w + 6.0, base_h, d + 4.0],
                0.0,
                render([0.32, 0.32, 0.34]),
            )),
            [0.0, base_h * 0.5, 3.0],
            id_quat(),
        ),
        // Brick base course.
        prim(
            solid(cuboid_tapered([w, brick_h, d], 0.0, brick(BRICK_TAN))),
            [0.0, base_h + brick_h * 0.5, 0.0],
            id_quat(),
        ),
        // Rendered upper walls.
        prim(
            solid(cuboid_tapered(
                [w, body_h - brick_h, d],
                0.0,
                render(RENDER_WHITE),
            )),
            [0.0, base_h + brick_h + (body_h - brick_h) * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Glazed storefront on the front.
    prims.push(prim(
        cuboid_tapered([w - 2.0, 2.4, 0.2], 0.0, glass(GLASS_TINT, 0.8)),
        [0.0, base_h + brick_h + 1.2, front],
        id_quat(),
    ));
    // Lit fascia sign over the storefront.
    prims.push(prim(
        cuboid_tapered([w - 1.0, 0.9, 0.2], 0.0, glow(SIGN_GLOW, 2.2)),
        [0.0, base_h + body_h - 0.5, front + 0.1],
        id_quat(),
    ));

    // Parapet and rooftop AC unit.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.3, 0.5, d + 0.3],
            0.0,
            render([0.7, 0.7, 0.68]),
        )),
        [0.0, base_h + body_h + 0.25, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0, 1.0, 1.8],
            0.0,
            enamel([0.7, 0.7, 0.72]),
        )),
        [-2.5, base_h + body_h + 0.5 + 0.5, -1.0],
        id_quat(),
    ));

    // Tall lit pole sign at the kerb.
    let pole_x = w * 0.5 + 3.0;
    prims.push(prim(
        solid(cuboid_tapered(
            [0.3, 5.0, 0.3],
            0.0,
            enamel([0.6, 0.6, 0.62]),
        )),
        [pole_x, 2.5, front + 1.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.8, 1.2, 0.25], 0.0, glow(SIGN_GLOW, 2.5)),
        [pole_x, 4.8, front + 1.0],
        id_quat(),
    ));

    // A parked car out front.
    prims.push(prim(
        solid(cuboid_tapered([1.9, 1.3, 4.0], 0.08, enamel(CAR_SILVER))),
        [2.5, base_h + 0.8, front + 4.0],
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
        assert_sanitize_stable(&MiniMart.build(""), "mini_mart");
    }
}
