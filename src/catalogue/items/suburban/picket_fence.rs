//! Picket fence — a Suburban prop. A short run of white pointed pickets on
//! two rails: the classic front-yard boundary.

use crate::catalogue::items::util::{assemble, cone, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_WHITE, wood};

pub struct PicketFence;

impl CatalogueEntry for PicketFence {
    fn slug(&self) -> &'static str {
        "picket_fence"
    }
    fn name(&self) -> &'static str {
        "Picket Fence"
    }
    fn description(&self) -> &'static str {
        "Run of white pointed pickets on two rails."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 4.0_f32;
    let picket_h = 1.1;

    // Lower rail — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([span, 0.12, 0.1], 0.0, wood(WOOD_WHITE))),
        [0.0, 0.45, 0.0],
        id_quat(),
    )];
    // Upper rail.
    prims.push(prim(
        solid(cuboid_tapered([span, 0.12, 0.1], 0.0, wood(WOOD_WHITE))),
        [0.0, 0.9, 0.0],
        id_quat(),
    ));

    // Sturdy end posts with pyramidal caps.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * span * 0.5;
        prims.push(prim(
            solid(cuboid_tapered([0.16, 1.3, 0.16], 0.0, wood(WOOD_WHITE))),
            [x, 0.65, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.13, 0.16, 4, wood(WOOD_WHITE))),
            [x, 1.38, 0.0],
            id_quat(),
        ));
    }

    // Pointed pickets, standing proud of the rails on the -Z front.
    let pickets = 11;
    for k in 0..pickets {
        let x = -span * 0.5 + 0.34 + k as f32 * (span - 0.68) / (pickets - 1) as f32;
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, picket_h, 0.05],
                0.0,
                wood(WOOD_WHITE),
            )),
            [x, picket_h * 0.5 + 0.1, -0.05],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.09, 0.18, 4, wood(WOOD_WHITE))),
            [x, picket_h + 0.1, -0.05],
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
        assert_sanitize_stable(&PicketFence.build(""), "picket_fence");
    }
}
