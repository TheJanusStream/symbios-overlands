//! Membrane wall — an Alien-Organic secondary. A barrier of translucent
//! membrane stretched between chitin ribs, threaded with glowing veins. The
//! living rampart of the colony; its veins are emissive trim the ruin pass can
//! darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the first rib.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, CHITIN_DARK, MEMBRANE_TEAL, chitin, membrane};

pub struct MembraneWall;

impl CatalogueEntry for MembraneWall {
    fn slug(&self) -> &'static str {
        "membrane_wall"
    }
    fn name(&self) -> &'static str {
        "Membrane Wall"
    }
    fn description(&self) -> &'static str {
        "Translucent membrane stretched between chitin ribs, threaded with glowing veins."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
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
        // First chitin rib — the root.
        prim(
            solid(cylinder_tapered(0.3, 4.0, 8, 0.3, chitin(CHITIN_DARK))),
            [-3.0, 2.0, 0.0],
            id_quat(),
        ),
    ];
    // More ribs along the wall.
    for x in [0.0_f32, 3.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.3, 4.0, 8, 0.3, chitin(CHITIN_DARK))),
            [x, 2.0, 0.0],
            id_quat(),
        ));
    }

    // Stretched membrane panels between the ribs.
    for x in [-1.5_f32, 1.5] {
        prims.push(prim(
            cuboid_tapered([2.8, 3.2, 0.1], 0.0, membrane(MEMBRANE_TEAL)),
            [x, 1.9, 0.0],
            id_quat(),
        ));
        // A glowing vein threading the panel — emissive.
        prims.push(prim(
            cuboid_tapered([0.12, 3.0, 0.14], 0.0, glow(BIOLUME_GREEN, 2.0)),
            [x, 1.9, 0.05],
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
        assert_sanitize_stable(&MembraneWall.build(""), "membrane_wall");
    }
}
