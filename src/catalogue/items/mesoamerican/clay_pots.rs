//! Clay pots — a Mesoamerican *poor* prop. A cluster of unglazed terracotta
//! ollas — a big water jar and a few smaller pots — with a spill of dried
//! maize cobs. The everyday clutter of a commoner's yard.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
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
    let olla = |r: f32, h: f32| solid(cylinder_tapered(r, h, 12, 0.25, painted(CLAY_TERRACOTTA)));

    let mut prims = vec![
        // Big water jar — the root.
        prim(olla(0.5, 1.1), [0.0, 0.55, 0.0], id_quat()),
    ];
    // Neck collar of the big jar.
    prims.push(prim(
        solid(cylinder_tapered(
            0.28,
            0.25,
            10,
            0.0,
            painted(CLAY_TERRACOTTA),
        )),
        [0.0, 1.2, 0.0],
        id_quat(),
    ));

    // Smaller pots around it.
    prims.push(prim(olla(0.32, 0.6), [0.75, 0.3, 0.2], id_quat()));
    prims.push(prim(olla(0.26, 0.5), [-0.6, 0.25, 0.45], id_quat()));
    prims.push(prim(olla(0.3, 0.55), [0.1, 0.28, -0.75], id_quat()));

    // A spill of dried maize cobs.
    for (x, z) in [(-0.3_f32, -0.4_f32), (-0.15, -0.55)] {
        prims.push(prim(
            cuboid_tapered([0.28, 0.1, 0.1], 0.2, painted(MAIZE_GOLD)),
            [x, 0.06, z],
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
        assert_sanitize_stable(&ClayPots.build(""), "clay_pots");
    }
}
