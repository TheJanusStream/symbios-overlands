//! Adobe hut — the Mesoamerican *poor* landmark. A commoner's house: low
//! mud-brick walls under a steep palm-thatch roof, hearth smoke seeping from
//! the ridge. The humble counterpart to the [`step_pyramid`](super::step_pyramid):
//! same theme, opposite end of the prosperity axis (`Poor`), so a destitute
//! room grows this instead of the temple-mountain.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ADOBE_TAN, STONE_GREY, THATCH_STRAW, TIMBER_BROWN, cobble, fx, painted, thatch, timber,
};

pub struct AdobeHut;

impl CatalogueEntry for AdobeHut {
    fn slug(&self) -> &'static str {
        "adobe_hut"
    }
    fn name(&self) -> &'static str {
        "Adobe Hut"
    }
    fn description(&self) -> &'static str {
        "Mud-brick commoner's house under a steep thatch roof, seeping hearth smoke."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 8.0_f32;
    let w = 6.0_f32;
    let foot_h = 0.4;
    let wall_h = 2.4;
    let wall_top = foot_h + wall_h;
    let roof_h = 2.8;

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, foot_h, w + 0.6],
                0.0,
                cobble(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Adobe walls.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([l, wall_h, 0.4], 0.05, painted(ADOBE_TAN))),
            [0.0, foot_h + wall_h * 0.5, sz * (w * 0.5 - 0.2)],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.4, wall_h, w], 0.05, painted(ADOBE_TAN))),
            [sx * (l * 0.5 - 0.2), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Timber door in the near gable.
    prims.push(prim(
        solid(cuboid_tapered([0.45, 1.8, 1.3], 0.0, timber(TIMBER_BROWN))),
        [l * 0.5 - 0.1, foot_h + 0.9, 0.0],
        id_quat(),
    ));

    // Steep thatched hip roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.4, roof_h, w + 1.6],
            0.5,
            thatch(THATCH_STRAW),
        )),
        [0.0, wall_top + roof_h * 0.5, 0.0],
        id_quat(),
    ));

    let ridge_x = 2.0;
    let mut root = assemble(prims);
    // Signature life: hearth smoke seeping from the ridge.
    root.children.push(fx::hearth_smoke(
        [ridge_x, wall_top + roof_h + 0.1, 0.0],
        0x70F0_5E11,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&AdobeHut.build(""), "adobe_hut");
    }
}
