//! Clay pots — a Mesoamerican *poor* prop. A cluster of unglazed terracotta
//! ollas — a big water jar and a few smaller pots — with a spill of dried
//! maize cobs. The everyday clutter of a commoner's yard.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAY_TERRACOTTA, painted};

/// Dried-maize gold.
const MAIZE_GOLD: [f32; 3] = [0.78, 0.62, 0.22];

pub struct ClayPots;

impl CatalogueEntry for ClayPots {
    fn slug(&self) -> &'static str {
        "clay_pots"
    }
    fn name(&self) -> &'static str {
        "Clay Pots"
    }
    fn description(&self) -> &'static str {
        "Cluster of terracotta ollas with a spill of dried maize."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Big round-bellied water olla — the root: a fat terracotta belly (the
    // amphora-belly sphere) with a narrow neck and a flared rim.
    let mut prims = vec![prim(
        solid(sphere(0.52, 6, painted(CLAY_TERRACOTTA))),
        [0.0, 0.55, 0.0],
        id_quat(),
    )];
    prims.push(prim(
        solid(cylinder_tapered(
            0.2,
            0.32,
            12,
            0.1,
            painted(CLAY_TERRACOTTA),
        )),
        [0.0, 0.98, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.05, 0.22, painted(CLAY_TERRACOTTA))),
        [0.0, 1.14, 0.0],
        id_quat(),
    ));

    // A few smaller round ollas standing around it, each a belly + neck.
    for (cx, cz, br) in [(0.82_f32, 0.18_f32, 0.34_f32), (-0.66, 0.42, 0.3)] {
        prims.push(prim(
            solid(sphere(br, 6, painted(CLAY_TERRACOTTA))),
            [cx, br * 0.95, cz],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(
                br * 0.42,
                br * 0.6,
                10,
                0.1,
                painted(CLAY_TERRACOTTA),
            )),
            [cx, br * 1.7, cz],
            id_quat(),
        ));
    }

    // A small olla tipped on its side, spilling a heap of dried maize cobs.
    let tip = [0.25_f32, 0.3_f32, -0.8_f32];
    prims.push(prim(
        solid(sphere(0.32, 6, painted(CLAY_TERRACOTTA))),
        tip,
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(
            0.15,
            0.3,
            10,
            0.1,
            painted(CLAY_TERRACOTTA),
        )),
        [tip[0], tip[1], tip[2] - 0.4],
        quat_x(FRAC_PI_2),
    ));
    // The maize spill tumbling from the mouth.
    for (x, z, r) in [
        (0.1_f32, -1.15_f32, 0.0_f32),
        (0.35, -1.25, 0.5),
        (0.0, -1.35, -0.4),
        (0.28, -1.45, 0.3),
        (0.12, -1.55, 0.1),
    ] {
        prims.push(prim(
            cuboid_tapered([0.26, 0.1, 0.1], 0.25, painted(MAIZE_GOLD)),
            [x, 0.07, z],
            quat_y(r),
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
        assert_sanitize_stable(&ClayPots.build(""), "clay_pots");
    }
}
