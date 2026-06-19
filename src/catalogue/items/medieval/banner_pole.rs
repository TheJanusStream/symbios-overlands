//! Banner pole — a Medieval prop. A tall oak standard on a stone foot,
//! flying a heraldic gonfalon from a crossbar with dagged tails and an
//! iron spear finial: the lord's colours over the market square. A
//! freestanding standard, distinct from the wall-hung civic banner.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    HERALD_BLUE, HERALD_GOLD, IRON_DARK, STONE_GREY, WOOD_DARK, WOOD_OAK, cloth, iron, rough_stone,
    timber,
};

pub struct BannerPole;

impl CatalogueEntry for BannerPole {
    fn slug(&self) -> &'static str {
        "banner_pole"
    }
    fn name(&self) -> &'static str {
        "Banner Pole"
    }
    fn description(&self) -> &'static str {
        "Tall oak standard on a stone foot flying a heraldic gonfalon and iron finial."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 5.0;
    let base_h = 0.5;

    let mut prims = vec![
        // Stone foot — the root.
        prim(
            solid(cuboid_tapered(
                [0.7, base_h, 0.7],
                0.2,
                rough_stone(STONE_GREY),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Oak pole.
        prim(
            solid(cylinder_tapered(0.1, pole_h, 10, 0.1, timber(WOOD_OAK))),
            [0.0, base_h + pole_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Iron spear finial at the top.
    prims.push(prim(
        solid(cone(0.12, 0.5, 8, iron(IRON_DARK))),
        [0.0, base_h + pole_h + 0.2, 0.0],
        id_quat(),
    ));

    // Crossbar carrying the gonfalon, near the top.
    let bar_y = base_h + pole_h - 0.6;
    prims.push(prim(
        solid(cuboid_tapered([0.06, 0.06, 1.3], 0.0, timber(WOOD_DARK))),
        [0.0, bar_y, 0.0],
        id_quat(),
    ));

    // Heraldic gonfalon hanging from the crossbar.
    prims.push(prim(
        cuboid_tapered([0.07, 2.0, 1.1], 0.0, cloth(HERALD_BLUE, HERALD_GOLD)),
        [0.0, bar_y - 1.1, 0.0],
        id_quat(),
    ));
    // Two dagged (forked) tails at the foot of the banner.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.07, 0.6, 0.45], 0.6, cloth(HERALD_GOLD, HERALD_BLUE)),
            [0.0, bar_y - 2.3, sz * 0.27],
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
        assert_sanitize_stable(&BannerPole.build(""), "banner_pole");
    }
}
