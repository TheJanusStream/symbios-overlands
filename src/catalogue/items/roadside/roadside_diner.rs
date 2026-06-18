//! Roadside diner — a Roadside secondary. A low chrome-banded brick diner
//! with a long run of lit windows and a vertical neon sign on the roof. The
//! all-night eatery of the strip.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, CHROME_BRIGHT, CONCRETE_GREY, ENAMEL_RED, GLASS_TINT, NEON_CYAN, NEON_RED,
    STEEL_GREY, brick, chrome, concrete, enamel, fx, glass, steel,
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
    // Chrome wainscot band at the base.
    prims.push(prim(
        solid(cuboid_tapered([10.3, 0.8, 5.3], 0.0, chrome(CHROME_BRIGHT))),
        [0.0, slab_h + 0.4, 0.0],
        id_quat(),
    ));
    // Chrome roof cap with a red eave stripe.
    prims.push(prim(
        solid(cuboid_tapered([10.5, 0.4, 5.5], 0.0, chrome(CHROME_BRIGHT))),
        [0.0, roof_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([10.6, 0.18, 5.6], 0.0, enamel(ENAMEL_RED)),
        [0.0, roof_y - 0.25, 0.0],
        id_quat(),
    ));

    // Long run of lit windows on the +Z front.
    prims.push(prim(
        cuboid_tapered([9.0, 1.6, 0.15], 0.0, glass(GLASS_TINT, 1.5)),
        [0.0, slab_h + 1.6, 2.55],
        id_quat(),
    ));
    // Door + chrome entrance canopy at one end.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 2.1, 0.2], 0.0, chrome(CHROME_BRIGHT))),
        [3.6, slab_h + 1.05, 2.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.15, 1.0], 0.0, chrome(CHROME_BRIGHT))),
        [3.6, slab_h + 2.2, 3.1],
        quat_x(0.2),
    ));

    // Vertical neon sign on the roof: a steel mast, an enamel board and a
    // glowing neon face with a cyan accent bar.
    let sx = -3.8_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.2, 1.2, 0.2], 0.0, steel(STEEL_GREY))),
        [sx, roof_y + 0.7, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 2.6, 1.7], 0.0, enamel(ENAMEL_RED))),
        [sx, roof_y + 2.4, 0.0],
        id_quat(),
    ));
    let mut neon = prim(
        cuboid_tapered([0.55, 2.3, 1.4], 0.0, glow(NEON_RED, 4.0)),
        [sx, roof_y + 2.4, 0.0],
        id_quat(),
    );
    neon.audio = fx::neon_buzz();
    prims.push(neon);
    prims.push(prim(
        cuboid_tapered([0.6, 0.3, 1.5], 0.0, glow(NEON_CYAN, 3.5)),
        [sx, roof_y + 1.3, 0.0],
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
        assert_sanitize_stable(&RoadsideDiner.build(""), "roadside_diner");
    }
}
