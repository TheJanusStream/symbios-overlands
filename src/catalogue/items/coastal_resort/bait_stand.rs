//! Bait stand — a Coastal-Resort *poor* secondary. A driftwood counter
//! under a lean-to plank roof on two posts, a hand-lettered board out front
//! and a pair of chum buckets: the bait shop of the fishing hamlet.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_WHITE, BUOY_RED, DECK_WOOD, DRIFT_GREY, LAMP_WARM, STEEL_GREY, enamel, plank, steel,
};

pub struct BaitStand;

impl CatalogueEntry for BaitStand {
    fn slug(&self) -> &'static str {
        "bait_stand"
    }
    fn name(&self) -> &'static str {
        "Bait Stand"
    }
    fn description(&self) -> &'static str {
        "Driftwood counter under a lean-to plank roof with a sign and chum buckets."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_POOR
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
    let mut prims = vec![
        // Driftwood counter — the root.
        prim(
            solid(cuboid_tapered([3.0, 1.0, 1.2], 0.0, plank(DRIFT_GREY))),
            [0.0, 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Two back posts (the serving side and signage face the -Z render front).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 2.2, 0.12], 0.0, plank(DECK_WOOD))),
            [sx * 1.3, 1.1, 0.5],
            id_quat(),
        ));
    }

    // Slanted lean-to roof, pitched down toward the back.
    prims.push(prim(
        solid(cuboid_tapered([3.4, 0.2, 2.0], 0.0, plank(DRIFT_GREY))),
        [0.0, 2.1, -0.1],
        quat_x(-0.3),
    ));

    // Hand-lettered board out front with a pale painted placard.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.5, 0.1], 0.0, plank(DECK_WOOD))),
        [0.0, 1.35, -0.65],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.3, 0.32, 0.06], 0.0, enamel(AWNING_WHITE)),
        [0.0, 1.35, -0.72],
        id_quat(),
    ));

    // Two chum buckets on the counter.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.2, 0.4, 10, 0.08, enamel(BUOY_RED))),
            [sx * 0.8, 1.2, -0.2],
            id_quat(),
        ));
    }

    // A tackle crate on the counter.
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.34, 0.4], 0.0, plank(DRIFT_GREY))),
        [0.05, 1.17, 0.3],
        id_quat(),
    ));

    // A pair of rods leaning against the +X post.
    for k in 0..2 {
        prims.push(prim(
            solid(cylinder_tapered(0.03, 2.6, 6, 0.0, steel(STEEL_GREY))),
            [1.25 + k as f32 * 0.12, 1.7, 0.45],
            quat_z(0.22 + k as f32 * 0.05),
        ));
    }

    // A dim bulb under the lean-to.
    prims.push(prim(
        cuboid_tapered([0.16, 0.2, 0.16], 0.0, glow(LAMP_WARM, 1.6)),
        [0.0, 1.95, -0.2],
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
        assert_sanitize_stable(&BaitStand.build(""), "bait_stand");
    }
}
