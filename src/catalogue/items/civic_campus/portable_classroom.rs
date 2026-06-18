//! Portable classroom — the Civic/Campus *poor* landmark. A demountable
//! modular cabin up on cinder blocks, its painted skirting peeling, a metal
//! door at the top of a short ramp and a row of dim windows. The
//! hardscrabble counterpart to the [`town_hall`](super::town_hall): same
//! quarter, opposite end of the prosperity axis (`Poor`), so a destitute
//! civic room grows the underfunded lot instead of the stone campus.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the cabin floor.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, GLASS_TINT, PLANK_WOOD, concrete, glass, painted, plank};

/// Faded beige of the cabin's painted cladding.
const CABIN_BEIGE: [f32; 3] = [0.74, 0.70, 0.58];

pub struct PortableClassroom;

impl CatalogueEntry for PortableClassroom {
    fn slug(&self) -> &'static str {
        "portable_classroom"
    }
    fn name(&self) -> &'static str {
        "Portable Classroom"
    }
    fn description(&self) -> &'static str {
        "Demountable cabin on cinder blocks with a metal door, ramp and dim windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let floor_y = 0.7_f32;
    let wall_h = 2.6_f32;
    let wall_y = floor_y + 0.15 + wall_h * 0.5;
    let wall_top = floor_y + 0.15 + wall_h;

    let mut prims = vec![
        // Painted cabin floor box — the root, raised on blocks.
        prim(
            solid(cuboid_tapered([7.0, 0.3, 3.5], 0.0, painted(CABIN_BEIGE))),
            [0.0, floor_y, 0.0],
            id_quat(),
        ),
    ];

    // Cinder blocks under the cabin.
    for sx in [-1.0_f32, 0.0, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.5, floor_y, 0.5],
                    0.0,
                    concrete(CONCRETE_GREY),
                )),
                [sx * 3.0, floor_y * 0.5, sz * 1.4],
                id_quat(),
            ));
        }
    }

    // Painted walls.
    prims.push(prim(
        solid(cuboid_tapered(
            [6.6, wall_h, 3.2],
            0.0,
            painted(CABIN_BEIGE),
        )),
        [0.0, wall_y, 0.0],
        id_quat(),
    ));
    // Flat roof cap.
    prims.push(prim(
        solid(cuboid_tapered(
            [7.0, 0.25, 3.6],
            0.0,
            painted([0.5, 0.5, 0.5]),
        )),
        [0.0, wall_top + 0.12, 0.0],
        id_quat(),
    ));

    // Metal door + dim windows on the +Z face.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, 2.0, 0.15],
            0.0,
            painted([0.45, 0.46, 0.48]),
        )),
        [-2.2, floor_y + 0.15 + 1.0, 1.65],
        id_quat(),
    ));
    for x in [0.2_f32, 2.0] {
        prims.push(prim(
            cuboid_tapered([1.2, 1.0, 0.12], 0.0, glass(GLASS_TINT, 0.4)),
            [x, wall_y + 0.2, 1.65],
            id_quat(),
        ));
    }

    // Short access ramp up to the door.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.15, 1.8], 0.0, plank(PLANK_WOOD))),
        [-2.2, floor_y * 0.5, 2.4],
        quat_x(0.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PortableClassroom.build(""), "portable_classroom");
    }
}
