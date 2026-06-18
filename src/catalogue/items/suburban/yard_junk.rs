//! Yard junk — a Suburban *poor* prop. A heap of cast-offs: a stack of bald
//! tires, a dead chest freezer on its side, and a tangle of scrap, left out
//! on the lot.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_BROWN, enamel, wood};

/// Bald-tyre black.
const TIRE: [f32; 3] = [0.08, 0.08, 0.09];
/// Yellowed old-appliance enamel.
const APPLIANCE: [f32; 3] = [0.74, 0.72, 0.64];

pub struct YardJunk;

impl CatalogueEntry for YardJunk {
    fn slug(&self) -> &'static str {
        "yard_junk"
    }
    fn name(&self) -> &'static str {
        "Yard Junk"
    }
    fn description(&self) -> &'static str {
        "Stack of bald tires, a dead chest freezer, and a tangle of scrap."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_POOR
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
    let tire = || solid(cylinder_tapered(0.42, 0.25, 12, 0.0, enamel(TIRE)));

    // Bottom tire of the stack — the root.
    let mut prims = vec![prim(tire(), [0.0, 0.13, 0.0], id_quat())];
    prims.push(prim(tire(), [0.08, 0.38, 0.05], id_quat()));
    prims.push(prim(tire(), [-0.05, 0.63, -0.04], id_quat()));

    // Dead chest freezer tipped on its side.
    prims.push(prim(
        solid(cuboid_tapered([1.5, 0.85, 0.8], 0.0, enamel(APPLIANCE))),
        [1.3, 0.43, 0.4],
        quat_y(0.4),
    ));
    // Open lid hanging off it.
    prims.push(prim(
        solid(cuboid_tapered([1.5, 0.1, 0.7], 0.0, enamel(APPLIANCE))),
        [1.3, 0.85, 0.0],
        quat_x(0.5),
    ));

    // A tangle of scrap lumber.
    for (x, z, yaw) in [(-1.1_f32, 0.3_f32, 0.3_f32), (-0.9, 0.6, 1.2)] {
        prims.push(prim(
            solid(cylinder_tapered(0.06, 1.6, 5, 0.0, wood(WOOD_BROWN))),
            [x, 0.1, z],
            quat_x(FRAC_PI_2 + yaw * 0.1),
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
        assert_sanitize_stable(&YardJunk.build(""), "yard_junk");
    }
}
