//! Dormitory — a Civic/Campus secondary. A tall brick residence hall with
//! three banded floors of lit windows, a concrete parapet and a small
//! entrance canopy. The student housing of the campus.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the plinth.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, CONCRETE_GREY, GLASS_TINT, brick, concrete, glass};

pub struct Dormitory;

impl CatalogueEntry for Dormitory {
    fn slug(&self) -> &'static str {
        "dormitory"
    }
    fn name(&self) -> &'static str {
        "Dormitory"
    }
    fn description(&self) -> &'static str {
        "Tall brick residence hall with banded floors of lit windows."
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
            clearance: 8.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plinth_h = 0.4_f32;
    let body_h = 7.0_f32;
    let body_top = plinth_h + body_h;

    let mut prims = vec![
        // Concrete plinth — the root.
        prim(
            solid(cuboid_tapered(
                [11.0, plinth_h, 7.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, plinth_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Brick body.
    prims.push(prim(
        solid(cuboid_tapered([9.5, body_h, 6.0], 0.0, brick(BRICK_RED))),
        [0.0, plinth_h + body_h * 0.5, 0.0],
        id_quat(),
    ));

    // Three banded floors of lit windows on the +Z front.
    for fy in [1.4_f32, 3.6, 5.8] {
        prims.push(prim(
            cuboid_tapered([8.6, 1.0, 0.15], 0.0, glass(GLASS_TINT, 1.2)),
            [0.0, plinth_h + fy, 3.05],
            id_quat(),
        ));
    }

    // Concrete parapet.
    prims.push(prim(
        solid(cuboid_tapered(
            [9.9, 0.5, 6.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, body_top + 0.25, 0.0],
        id_quat(),
    ));

    // Entrance door + small concrete canopy.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.6, 2.2, 0.2],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, plinth_h + 1.1, 3.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [2.6, 0.25, 1.2],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, plinth_h + 2.4, 3.6],
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
        assert_sanitize_stable(&Dormitory.build(""), "dormitory");
    }
}
