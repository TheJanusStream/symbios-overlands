//! Bus shelter — a Civic/Campus *poor* secondary. A worn three-sided steel-
//! and-glass transit shelter with a bench and a faded route panel. The edge
//! of the underfunded quarter.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, GLASS_TINT, PLANK_WOOD, STEEL_GREY, concrete, glass, painted, plank, steel,
};

pub struct BusShelter;

impl CatalogueEntry for BusShelter {
    fn slug(&self) -> &'static str {
        "bus_shelter"
    }
    fn name(&self) -> &'static str {
        "Bus Shelter"
    }
    fn description(&self) -> &'static str {
        "Worn three-sided steel-and-glass transit shelter with a bench and route panel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.2_f32;
    let post_h = 2.4_f32;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [3.6, pad_h, 1.6],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Four corner steel posts.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.1, post_h, 0.1], 0.0, steel(STEEL_GREY))),
                [sx * 1.7, pad_h + post_h * 0.5, sz * 0.7],
                id_quat(),
            ));
        }
    }

    // Grimy glass back (on the +Z back) and side panels — the shelter opens
    // toward the -Z render front so the camera looks in at the bench.
    prims.push(prim(
        cuboid_tapered([3.4, 1.8, 0.1], 0.0, glass(GLASS_TINT, 0.0)),
        [0.0, pad_h + 1.1, 0.7],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.1, 1.8, 1.4], 0.0, glass(GLASS_TINT, 0.0)),
            [sx * 1.7, pad_h + 1.1, 0.0],
            id_quat(),
        ));
    }

    // Flat steel roof with a faded fascia band along the open front.
    prims.push(prim(
        solid(cuboid_tapered([3.8, 0.2, 1.8], 0.0, steel(STEEL_GREY))),
        [0.0, pad_h + post_h, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [3.8, 0.34, 0.1],
            0.0,
            painted([0.32, 0.4, 0.5]),
        )),
        [0.0, pad_h + post_h - 0.18, -0.95],
        id_quat(),
    ));

    // Bench against the back glass: a slatted seat on two end frames with a
    // low back rail.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.12, 0.42], 0.0, plank(PLANK_WOOD))),
        [0.0, pad_h + 0.5, 0.42],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.3, 0.08], 0.0, plank(PLANK_WOOD))),
        [0.0, pad_h + 0.78, 0.6],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.5, 0.42], 0.0, steel(STEEL_GREY))),
            [sx * 1.4, pad_h + 0.25, 0.42],
            id_quat(),
        ));
    }

    // Faded timetable panel mounted on the back glass, reading to the front.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, 1.0, 0.06],
            0.0,
            painted([0.6, 0.64, 0.68]),
        )),
        [1.0, pad_h + 1.3, 0.62],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.78, 0.22, 0.04],
            0.0,
            painted([0.30, 0.42, 0.52]),
        )),
        [1.0, pad_h + 1.66, 0.59],
        id_quat(),
    ));

    // A dented litter bin in the front corner.
    prims.push(prim(
        solid(cylinder_tapered(
            0.22,
            0.7,
            12,
            0.08,
            steel([0.4, 0.42, 0.44]),
        )),
        [-1.45, pad_h + 0.35, -0.45],
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
        assert_sanitize_stable(&BusShelter.build(""), "bus_shelter");
    }
}
