//! Library — a Civic/Campus secondary. A stone reading hall behind a small
//! marble colonnade, tall lit windows down its front and a balustraded flat
//! roof. The scholarly heart of the quad.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{COPPER_VERDIGRIS, GLASS_TINT, MARBLE_WHITE, STONE_PALE, copper, glass, marble, stone};

pub struct Library;

impl CatalogueEntry for Library {
    fn slug(&self) -> &'static str {
        "library"
    }
    fn name(&self) -> &'static str {
        "Library"
    }
    fn description(&self) -> &'static str {
        "Stone reading hall with a marble colonnade and tall lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
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
    let base_h = 0.6_f32;
    let body_h = 5.0_f32;
    let body_top = base_h + body_h;

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered(
                [12.0, base_h, 8.0],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Stone body.
    prims.push(prim(
        solid(cuboid_tapered([10.0, body_h, 6.5], 0.0, stone(STONE_PALE))),
        [0.0, base_h + body_h * 0.5, -0.3],
        id_quat(),
    ));
    // Tall lit windows across the front.
    prims.push(prim(
        cuboid_tapered([8.0, 3.0, 0.2], 0.0, glass(GLASS_TINT, 1.2)),
        [0.0, base_h + 2.5, 2.95],
        id_quat(),
    ));
    // Bronze doors.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.8, 2.6, 0.3],
            0.0,
            copper(COPPER_VERDIGRIS),
        )),
        [0.0, base_h + 1.3, 3.0],
        id_quat(),
    ));

    // Four marble columns across the front.
    for x in [-3.0_f32, -1.0, 1.0, 3.0] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.4,
                body_h - 0.6,
                12,
                0.04,
                marble(MARBLE_WHITE),
            )),
            [x, base_h + (body_h - 0.6) * 0.5, 3.4],
            id_quat(),
        ));
    }
    // Entablature beam over the colonnade.
    prims.push(prim(
        solid(cuboid_tapered([9.0, 0.7, 1.0], 0.0, marble(MARBLE_WHITE))),
        [0.0, body_top - 0.4, 3.2],
        id_quat(),
    ));

    // Roof parapet with balustrade posts.
    prims.push(prim(
        solid(cuboid_tapered([10.4, 0.5, 6.9], 0.0, marble(MARBLE_WHITE))),
        [0.0, body_top + 0.25, -0.3],
        id_quat(),
    ));
    for x in [-4.0_f32, -2.0, 0.0, 2.0, 4.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.4, 0.6, 0.4], 0.0, marble(MARBLE_WHITE))),
            [x, body_top + 0.8, 2.9],
            id_quat(),
        ));
    }

    // Two front steps.
    for k in 0..2 {
        let kf = k as f32;
        prims.push(prim(
            solid(cuboid_tapered([9.0, 0.3, 0.9], 0.0, marble(MARBLE_WHITE))),
            [0.0, base_h - 0.15 - kf * 0.3, 4.2 + kf * 0.8],
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
        assert_sanitize_stable(&Library.build(""), "library");
    }
}
