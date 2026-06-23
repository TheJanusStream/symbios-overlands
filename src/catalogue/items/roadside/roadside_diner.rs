//! Roadside diner — a Roadside secondary. A low chrome-banded brick diner
//! with a long run of lit windows and a vertical neon sign on the roof. The
//! all-night eatery of the strip.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::modern_city::curtain_wall;
use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, CHROME_BRIGHT, CONCRETE_GREY, ENAMEL_RED, GLASS_TINT, NEON_CYAN, NEON_RED,
    STEEL_GREY, brick, chrome, concrete, enamel, fx, glass, sign_board, steel,
};

pub struct RoadsideDiner;

impl CatalogueEntry for RoadsideDiner {
    fn slug(&self) -> &'static str {
        "roadside_diner"
    }
    fn name(&self) -> &'static str {
        "Roadside Diner"
    }
    fn description(&self) -> &'static str {
        "Chrome-banded brick diner with lit windows and a vertical neon sign."
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
            clearance: 7.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_h = 3.0_f32;
    let body_y = slab_h + body_h * 0.5;
    let roof_y = slab_h + body_h + 0.2;

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [12.0, slab_h, 6.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Brick body.
    prims.push(prim(
        solid(cuboid_tapered([10.0, body_h, 5.0], 0.0, brick(BRICK_TAN))),
        [0.0, body_y, 0.0],
        id_quat(),
    ));
    // Chrome wainscot band at the base, proud of the brick.
    prims.push(prim(
        solid(cuboid_tapered([10.4, 0.8, 5.4], 0.0, chrome(CHROME_BRIGHT))),
        [0.0, slab_h + 0.4, 0.0],
        id_quat(),
    ));
    // Chrome streamline eave band + roof cap, proud and staggered so the trim
    // never sits flush with the brick (coplanar z-fight).
    prims.push(prim(
        solid(cuboid_tapered([10.6, 0.5, 5.6], 0.0, chrome(CHROME_BRIGHT))),
        [0.0, slab_h + body_h - 0.25, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([10.7, 0.22, 5.7], 0.0, enamel(ENAMEL_RED)),
        [0.0, slab_h + body_h - 0.55, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([10.2, 0.4, 5.2], 0.0, chrome(CHROME_BRIGHT))),
        [0.0, roof_y, 0.0],
        id_quat(),
    ));

    // Long run of mullioned lit windows on the −Z (camera) front.
    let front = -2.5_f32;
    for g in curtain_wall(
        [0.0, slab_h + 1.7, front - 0.2],
        [8.4, 1.7],
        (6, 1),
        -0.22,
        glass(GLASS_TINT, 1.6),
        chrome(CHROME_BRIGHT),
    ) {
        prims.push(g);
    }
    // Glazed door + chrome entrance canopy at one end, projecting toward −Z.
    prims.push(prim(
        cuboid_tapered([1.0, 2.0, 0.12], 0.0, glass(GLASS_TINT, 1.7)),
        [3.6, slab_h + 1.0, front - 0.32],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.8, 0.15, 1.0], 0.0, chrome(CHROME_BRIGHT))),
        [3.6, slab_h + 2.3, front - 0.7],
        quat_x(-0.2),
    ));

    // Vertical neon sign on the roof: a steel mast, an enamel board and a
    // segmented glowing neon strip (stacked letters) facing −Z, with a cyan
    // accent bar — segmented so the lit face reads as a sign, not a blown slab.
    let sx = -3.6_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.25, 1.0, 0.25], 0.0, steel(STEEL_GREY))),
        [sx, roof_y + 0.6, 0.0],
        id_quat(),
    ));
    // Enamel blade board, broad face toward the −Z road.
    prims.push(prim(
        solid(cuboid_tapered([1.7, 3.0, 0.3], 0.0, enamel(ENAMEL_RED))),
        [sx, roof_y + 2.4, 0.0],
        id_quat(),
    ));
    // Stacked-letter neon strip, proud of the blade front, facing −Z.
    let mut neon = sign_board(
        [sx, roof_y + 2.6, -0.35],
        [1.3, 2.2],
        (1, 4),
        NEON_RED,
        2.4,
        -1.0,
    );
    neon[1].audio = fx::neon_buzz();
    prims.extend(neon);
    // Cyan accent bar at the foot of the blade.
    for g in sign_board(
        [sx, roof_y + 1.2, -0.35],
        [1.4, 0.4],
        (2, 1),
        NEON_CYAN,
        2.2,
        -1.0,
    ) {
        prims.push(g);
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RoadsideDiner.build(""), "roadside_diner");
    }
}
