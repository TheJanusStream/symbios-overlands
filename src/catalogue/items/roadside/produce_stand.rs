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
    assemble, cuboid_tapered, cuboid_tapered_xz, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DRIFT_GREY, PLANK_WOOD, TARP_BLUE, enamel, plank};

/// Produce piled in the crates — sun-ripe reds, greens and oranges.
const APPLE_RED: [f32; 3] = [0.70, 0.20, 0.16];
const MELON_GREEN: [f32; 3] = [0.40, 0.55, 0.22];
const ORANGE_FRUIT: [f32; 3] = [0.86, 0.46, 0.10];

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
    // The display side faces the −Z camera front.
    let mut prims = vec![
        // Plank counter — the root.
        prim(
            solid(cuboid_tapered([3.6, 1.0, 1.5], 0.0, plank(DRIFT_GREY))),
            [0.0, 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Slanted display board across the counter front, tipped toward the buyer.
    prims.push(prim(
        solid(cuboid_tapered([3.5, 0.1, 0.7], 0.0, plank(PLANK_WOOD))),
        [0.0, 1.12, -0.72],
        quat_x(-0.5),
    ));

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

    // Peaked, sun-bleached tarp canopy (a low ridge along X) with a hanging
    // front valance flap on the −Z eave.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [3.9, 0.5, 2.3],
            [0.0, 0.85],
            enamel(TARP_BLUE),
        )),
        [0.0, 2.45, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.9, 0.45, 0.06], 0.0, enamel(TARP_BLUE)),
        [0.0, 2.25, -1.12],
        quat_x(0.18),
    ));

    // Crates of piled produce on the counter, facing the buyer.
    for (cx, fruit) in [
        (-1.1_f32, APPLE_RED),
        (-0.1, MELON_GREEN),
        (0.9, ORANGE_FRUIT),
        (1.5, APPLE_RED),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([0.66, 0.4, 0.6], 0.0, plank(PLANK_WOOD))),
            [cx, 1.2, -0.25],
            id_quat(),
        ));
        // A low heap of fruit (three faceted spheres).
        for (dx, dz, dy) in [
            (-0.13_f32, 0.0_f32, 0.0_f32),
            (0.13, 0.04, 0.0),
            (0.0, -0.1, 0.12),
        ] {
            prims.push(prim(
                solid(sphere(0.17, 4, enamel(fruit))),
                [cx + dx, 1.55 + dy, -0.25 + dz],
                id_quat(),
            ));
        }
    }

    // A spare crate stack on the ground at one end.
    for (k, gy) in [0.2_f32, 0.62].iter().enumerate() {
        prims.push(prim(
            solid(cuboid_tapered([0.6, 0.4, 0.55], 0.0, plank(PLANK_WOOD))),
            [-2.1 + k as f32 * 0.08, *gy, -0.4],
            id_quat(),
        ));
    }

    // Hand-painted price board hung on the front-left post, facing −Z.
    prims.push(prim(
        solid(cuboid_tapered([1.7, 0.7, 0.08], 0.0, plank(PLANK_WOOD))),
        [-1.0, 1.95, -0.72],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.4, 0.46, 0.04], 0.0, enamel([0.86, 0.82, 0.7])),
        [-1.0, 1.95, -0.78],
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
