//! Dinghy — a Coastal-Resort prop. A small clinker rowboat hauled up on the
//! sand: a tapered painted hull with two thwart benches and a pair of oars
//! laid along the gunwales.
//!
//! The hull is a tapered cylinder tipped on its side (a single [`quat_x`]
//! of π/2 lays the Y axis along Z), so it reads as a faceted clinker boat
//! narrowing to a bow.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DECK_WOOD, HULL_BLUE, plank};

pub struct Dinghy;

impl CatalogueEntry for Dinghy {
    fn slug(&self) -> &'static str {
        "dinghy"
    }
    fn name(&self) -> &'static str {
        "Dinghy"
    }
    fn description(&self) -> &'static str {
        "Small painted rowboat with thwart benches and oars, hauled up on the sand."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.2,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Tapered hull laid along Z — the root.
        prim(
            solid(cylinder_tapered(0.7, 3.4, 8, 0.25, plank(HULL_BLUE))),
            [0.0, 0.5, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Two thwart benches across the hull.
    for sz in [-0.8_f32, 0.7] {
        prims.push(prim(
            solid(cuboid_tapered([1.0, 0.08, 0.3], 0.0, plank(DECK_WOOD))),
            [0.0, 0.85, sz],
            id_quat(),
        ));
    }

    // A pair of oars laid fore-and-aft along the gunwales.
    for sx in [-0.42_f32, 0.42] {
        prims.push(prim(
            solid(cylinder_tapered(0.04, 2.4, 6, 0.0, plank(DECK_WOOD))),
            [sx, 0.92, 0.0],
            quat_x(FRAC_PI_2),
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
        assert_sanitize_stable(&Dinghy.build(""), "dinghy");
    }
}
