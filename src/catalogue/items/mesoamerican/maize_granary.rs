//! Maize granary — a Mesoamerican *poor* secondary. A cuezcomatl: a fat
//! round adobe storage jar on a stone foot, capped with a conical thatch
//! lid, where a household keeps its dried maize beside the
//! [`adobe_hut`](super::adobe_hut).

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ADOBE_TAN, STONE_GREY, THATCH_STRAW, TIMBER_BROWN, cobble, painted, thatch, timber};

/// Dried-maize gold.
const MAIZE_GOLD: [f32; 3] = [0.78, 0.62, 0.22];

pub struct MaizeGranary;

impl CatalogueEntry for MaizeGranary {
    fn slug(&self) -> &'static str {
        "maize_granary"
    }
    fn name(&self) -> &'static str {
        "Maize Granary"
    }
    fn description(&self) -> &'static str {
        "Round adobe storage jar on a stone foot under a conical thatch lid."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let belly_r = 1.35_f32;
    let belly_y = 1.6_f32;
    // Radius of the belly sphere at a given height — used to seat the coil
    // ridges and collar flush against its bulge.
    let belly_at = |y: f32| (belly_r * belly_r - (y - belly_y).powi(2)).max(0.0).sqrt();

    let mut prims = vec![
        // Stone foot — the root.
        prim(
            solid(cylinder_tapered(1.3, 0.4, 12, 0.0, cobble(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Fat round adobe belly (a plain sphere — the amphora belly), the
        // bulging body of the cuezcomatl.
        prim(
            solid(sphere(belly_r, 6, painted(ADOBE_TAN))),
            [0.0, belly_y, 0.0],
            id_quat(),
        ),
    ];

    // Mud-coil ridges banding the belly — the coiled-clay courses it is
    // built up from.
    for cy in [0.9_f32, 1.5, 2.1] {
        prims.push(prim(
            torus(0.05, belly_at(cy), painted([0.5, 0.36, 0.24])),
            [0.0, cy, 0.0],
            id_quat(),
        ));
    }

    // Low access hatch at the front base, where maize is drawn out.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.5, 0.4],
            0.0,
            painted([0.14, 0.1, 0.07]),
        )),
        [0.0, 0.7, -(belly_at(0.7) - 0.06)],
        id_quat(),
    ));
    // A few maize cobs spilled at the foot of the hatch.
    for (x, z) in [(-0.2_f32, -1.45_f32), (0.05, -1.55), (0.25, -1.4)] {
        prims.push(prim(
            solid(cuboid_tapered([0.26, 0.1, 0.1], 0.2, painted(MAIZE_GOLD))),
            [x, 0.07, z],
            id_quat(),
        ));
    }

    // Adobe rim collar atop the belly.
    prims.push(prim(
        solid(cylinder_tapered(
            belly_at(2.5) + 0.05,
            0.3,
            14,
            0.0,
            painted(ADOBE_TAN),
        )),
        [0.0, 2.55, 0.0],
        id_quat(),
    ));
    // Conical thatch lid, lashed down with a timber binding ring.
    prims.push(prim(
        solid(cone(1.3, 1.3, 14, thatch(THATCH_STRAW))),
        [0.0, 2.9, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.06, 1.1, timber(TIMBER_BROWN))),
        [0.0, 3.05, 0.0],
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
        assert_sanitize_stable(&MaizeGranary.build(""), "maize_granary");
    }
}
