//! Motel — a Roadside secondary. A single-storey strip of rooms under a
//! corrugated walkway roof, doors and lit windows marching down the front,
//! with a tall neon MOTEL pylon and a red VACANCY sign at the corner.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, CONCRETE_GREY, CORRUGATED_GREY, ENAMEL_BLUE, GLASS_TINT, NEON_CYAN, NEON_RED,
    STEEL_GREY, brick, concrete, corrugated, enamel, fx, glass, steel,
};

pub struct Motel;

impl CatalogueEntry for Motel {
    fn slug(&self) -> &'static str {
        "motel"
    }
    fn name(&self) -> &'static str {
        "Motel"
    }
    fn description(&self) -> &'static str {
        "Single-storey room strip under a walkway roof with a neon MOTEL pylon."
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
            clearance: 9.0,
            min_spawn_dist: 40.0,
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
    let roof_top = slab_h + body_h;

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [16.0, slab_h, 7.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Brick room block, set back from the front.
    prims.push(prim(
        solid(cuboid_tapered([14.0, body_h, 5.0], 0.0, brick(BRICK_TAN))),
        [0.0, body_y, -0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [14.4, 0.4, 5.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, roof_top + 0.2, -0.5],
        id_quat(),
    ));

    // Doors + lit windows repeating down the +Z front.
    for k in 0..5 {
        let x = -6.0 + k as f32 * 3.0;
        prims.push(prim(
            solid(cuboid_tapered([1.0, 2.0, 0.15], 0.0, enamel(ENAMEL_BLUE))),
            [x - 0.6, slab_h + 1.0, 2.05],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([1.2, 1.1, 0.15], 0.0, glass(GLASS_TINT, 1.2)),
            [x + 0.7, slab_h + 1.4, 2.05],
            id_quat(),
        ));
    }

    // Corrugated walkway roof on steel posts.
    prims.push(prim(
        solid(cuboid_tapered(
            [15.0, 0.25, 2.6],
            0.0,
            corrugated(CORRUGATED_GREY),
        )),
        [0.0, roof_top + 0.05, 2.6],
        id_quat(),
    ));
    for x in [-6.0_f32, -3.0, 0.0, 3.0, 6.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.15, body_h, 0.15], 0.0, steel(STEEL_GREY))),
            [x, body_y, 3.6],
            id_quat(),
        ));
    }

    // Neon MOTEL pylon + red VACANCY sign at the corner.
    let px = -8.5_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.3, 6.0, 0.3], 0.0, steel(STEEL_GREY))),
        [px, slab_h + 3.0, 3.0],
        id_quat(),
    ));
    let mut motel = prim(
        cuboid_tapered([0.5, 3.0, 1.4], 0.0, glow(NEON_CYAN, 4.0)),
        [px, slab_h + 6.0, 3.0],
        id_quat(),
    );
    motel.audio = fx::neon_buzz();
    prims.push(motel);
    prims.push(prim(
        cuboid_tapered([0.55, 0.9, 1.5], 0.0, glow(NEON_RED, 3.0)),
        [px, slab_h + 4.0, 3.0],
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
        assert_sanitize_stable(&Motel.build(""), "motel");
    }
}
