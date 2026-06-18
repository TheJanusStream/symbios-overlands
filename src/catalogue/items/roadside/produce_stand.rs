//! Produce stand — the Roadside *poor* landmark. A rickety plank counter
//! piled with crates of fruit under a sagging tarp on crooked posts, a
//! hand-painted board out front. The hardscrabble counterpart to the
//! [`gas_station`](super::gas_station): same shoulder, opposite end of the
//! prosperity axis (`Poor`), so a destitute roadside room grows the broke-
//! down hamlet instead of the franchise strip.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the counter.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DRIFT_GREY, PLANK_WOOD, TARP_BLUE, enamel, plank};

/// Red and green produce piled in the crates.
const APPLE_RED: [f32; 3] = [0.70, 0.20, 0.16];
const MELON_GREEN: [f32; 3] = [0.40, 0.55, 0.22];

pub struct ProduceStand;

impl CatalogueEntry for ProduceStand {
    fn slug(&self) -> &'static str {
        "produce_stand"
    }
    fn name(&self) -> &'static str {
        "Produce Stand"
    }
    fn description(&self) -> &'static str {
        "Rickety plank stand of fruit crates under a sagging tarp with a painted board."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Plank counter — the root.
        prim(
            solid(cuboid_tapered([3.6, 1.0, 1.5], 0.0, plank(DRIFT_GREY))),
            [0.0, 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Four crooked posts holding the tarp.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.12, 2.4, 0.12], 0.0, plank(PLANK_WOOD))),
                [sx * 1.6, 1.2, sz * 0.6],
                id_quat(),
            ));
        }
    }

    // Sagging blue tarp canopy, slightly sloped.
    prims.push(prim(
        cuboid_tapered([3.9, 0.1, 2.2], 0.1, enamel(TARP_BLUE)),
        [0.0, 2.5, 0.0],
        quat_x(0.12),
    ));

    // Fruit crates on the counter with piled produce.
    for (cx, fruit) in [(-1.0_f32, APPLE_RED), (0.2, MELON_GREEN), (1.2, APPLE_RED)] {
        prims.push(prim(
            solid(cuboid_tapered([0.7, 0.4, 0.6], 0.0, plank(PLANK_WOOD))),
            [cx, 1.2, 0.2],
            id_quat(),
        ));
        prims.push(prim(
            solid(sphere(0.28, 3, enamel(fruit))),
            [cx, 1.55, 0.2],
            id_quat(),
        ));
    }

    // Hand-painted board on the front-left post.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.6, 0.1], 0.0, plank(PLANK_WOOD))),
        [-1.0, 1.9, 0.7],
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
        assert_sanitize_stable(&ProduceStand.build(""), "produce_stand");
    }
}
