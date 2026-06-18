//! Iron fence — a Gothic-Horror prop. A section of black wrought-iron railing
//! with spear-tip finials between two posts. Scatter clutter bounding the
//! necropolis.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, iron};

pub struct IronFence;

impl CatalogueEntry for IronFence {
    fn slug(&self) -> &'static str {
        "iron_fence"
    }
    fn name(&self) -> &'static str {
        "Iron Fence"
    }
    fn description(&self) -> &'static str {
        "Section of black wrought-iron railing with spear-tip finials between two posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Top rail — the root.
        prim(
            solid(cuboid_tapered([3.6, 0.08, 0.08], 0.0, iron(IRON_BLACK))),
            [0.0, 1.0, 0.0],
            id_quat(),
        ),
    ];
    // Bottom rail.
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.08, 0.08], 0.0, iron(IRON_BLACK))),
        [0.0, 0.3, 0.0],
        id_quat(),
    ));

    // Two stouter end posts.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, 1.5, 0.14], 0.0, iron(IRON_BLACK))),
            [sx * 1.8, 0.75, 0.0],
            id_quat(),
        ));
    }

    // Vertical bars with spear-tip finials.
    for i in 0..7 {
        let x = -1.5 + i as f32 * 0.5;
        prims.push(prim(
            solid(cylinder_tapered(0.04, 1.2, 6, 0.0, iron(IRON_BLACK))),
            [x, 0.6, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.07, 0.25, 6, iron(IRON_BLACK))),
            [x, 1.3, 0.0],
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
        assert_sanitize_stable(&IronFence.build(""), "iron_fence");
    }
}
