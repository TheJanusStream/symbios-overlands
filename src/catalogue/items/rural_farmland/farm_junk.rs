//! Farm junk — a Rural/Farmland *poor* prop. A heap of cast-off equipment:
//! rusted oil drums, a busted plough, scrap sheet metal and an old tyre, left
//! to rot at the field edge.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::enamel;

/// Rusted iron.
const RUST: [f32; 3] = [0.42, 0.26, 0.15];
/// Faded blue drum.
const DRUM_BLUE: [f32; 3] = [0.28, 0.34, 0.42];
/// Old tyre.
const TIRE: [f32; 3] = [0.10, 0.10, 0.11];

pub struct FarmJunk;

impl CatalogueEntry for FarmJunk {
    fn slug(&self) -> &'static str {
        "farm_junk"
    }
    fn name(&self) -> &'static str {
        "Farm Junk"
    }
    fn description(&self) -> &'static str {
        "Heap of rusted drums, a busted plough, scrap metal, and an old tyre."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Standing rusted drum — the root.
    let mut prims = vec![prim(
        solid(cylinder_tapered(0.35, 1.0, 12, 0.0, enamel(RUST))),
        [0.0, 0.5, 0.0],
        id_quat(),
    )];
    // A faded drum tipped on its side.
    prims.push(prim(
        solid(cylinder_tapered(0.34, 1.0, 12, 0.0, enamel(DRUM_BLUE))),
        [0.9, 0.34, 0.3],
        quat_x(FRAC_PI_2),
    ));

    // Scrap sheet metal leaning in a heap.
    for (x, z, yaw) in [(-0.8_f32, 0.4_f32, 0.5_f32), (-0.6, -0.5, 1.1)] {
        prims.push(prim(
            solid(cuboid_tapered([1.2, 0.05, 0.8], 0.0, enamel(RUST))),
            [x, 0.3, z],
            quat_y(yaw),
        ));
    }

    // A busted plough body.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 0.4, 0.5], 0.2, enamel(RUST))),
        [0.4, 0.3, -0.8],
        quat_y(0.3),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.1, 0.6, 0.4],
            0.4,
            enamel([0.5, 0.32, 0.18]),
        )),
        [0.8, 0.3, -0.9],
        id_quat(),
    ));

    // Old tyre flat on the ground.
    prims.push(prim(
        solid(cylinder_tapered(0.42, 0.22, 12, 0.0, enamel(TIRE))),
        [-1.1, 0.11, -0.6],
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
        assert_sanitize_stable(&FarmJunk.build(""), "farm_junk");
    }
}
