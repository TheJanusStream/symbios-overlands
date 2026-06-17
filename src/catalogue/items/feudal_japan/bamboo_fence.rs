//! Bamboo fence — a Feudal-Japan prop. A short run of split-bamboo
//! palisade: upright canes lashed to two horizontal rails with dark cord,
//! the everyday boundary of a garden or lane.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BAMBOO_TAN, TIMBER_DARK, timber};

pub struct BambooFence;

impl CatalogueEntry for BambooFence {
    fn slug(&self) -> &'static str {
        "bamboo_fence"
    }
    fn name(&self) -> &'static str {
        "Bamboo Fence"
    }
    fn description(&self) -> &'static str {
        "Run of upright bamboo canes lashed to horizontal rails."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 4.0_f32;
    let cane_h = 1.7;

    // Lower rail — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([span, 0.1, 0.1], 0.0, timber(BAMBOO_TAN))),
        [0.0, 0.5, 0.0],
        id_quat(),
    )];
    // Upper rail.
    prims.push(prim(
        solid(cuboid_tapered([span, 0.1, 0.1], 0.0, timber(BAMBOO_TAN))),
        [0.0, cane_h - 0.2, 0.0],
        id_quat(),
    ));

    // Upright canes with their lashings.
    let canes = 9;
    for k in 0..canes {
        let x = -span * 0.5 + 0.2 + k as f32 * (span - 0.4) / (canes - 1) as f32;
        prims.push(prim(
            solid(cylinder_tapered(0.07, cane_h, 7, 0.04, timber(BAMBOO_TAN))),
            [x, cane_h * 0.5, 0.0],
            id_quat(),
        ));
        // Cord lashings at the two rails.
        for ry in [0.5, cane_h - 0.2] {
            prims.push(prim(
                cuboid_tapered([0.12, 0.08, 0.14], 0.0, timber(TIMBER_DARK)),
                [x, ry, 0.0],
                id_quat(),
            ));
        }
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BambooFence.build(""), "bamboo_fence");
    }
}
