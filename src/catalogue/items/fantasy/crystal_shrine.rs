//! Crystal shrine — a High-Fantasy secondary. An open stone shrine of four
//! pillars sheltering a great glowing crystal cluster on a gold-ringed plinth,
//! singing softly. Its crystal is emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the plinth.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CRYSTAL_CYAN, GOLD, STONE_GREY, fx, gold, stone};

pub struct CrystalShrine;

impl CatalogueEntry for CrystalShrine {
    fn slug(&self) -> &'static str {
        "crystal_shrine"
    }
    fn name(&self) -> &'static str {
        "Crystal Shrine"
    }
    fn description(&self) -> &'static str {
        "Open stone shrine sheltering a glowing crystal cluster on a gold-ringed plinth."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plinth_h = 0.8_f32;

    let mut prims = vec![
        // Stone plinth — the root.
        prim(
            solid(cuboid_tapered([4.0, plinth_h, 4.0], 0.0, stone(STONE_GREY))),
            [0.0, plinth_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Four corner pillars + a stone canopy.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(0.3, 3.4, 10, 0.1, stone(STONE_GREY))),
                [sx * 1.6, plinth_h + 1.7, sz * 1.6],
                id_quat(),
            ));
        }
    }
    prims.push(prim(
        solid(cuboid_tapered([4.2, 0.5, 4.2], 0.3, stone(STONE_GREY))),
        [0.0, plinth_h + 3.6, 0.0],
        id_quat(),
    ));

    // Gold ring around the crystal base.
    prims.push(prim(
        solid(torus(0.12, 0.9, gold(GOLD))),
        [0.0, plinth_h + 0.1, 0.0],
        id_quat(),
    ));

    // Glowing crystal cluster — a tall central shard flanked by smaller ones.
    prims.push(prim(
        cone(0.5, 2.6, 6, glow(CRYSTAL_CYAN, 3.5)),
        [0.0, plinth_h + 1.3, 0.0],
        id_quat(),
    ));
    for (cx, cz, h) in [
        (0.6_f32, 0.2_f32, 1.4_f32),
        (-0.5, 0.4, 1.2),
        (0.1, -0.6, 1.0),
    ] {
        prims.push(prim(
            cone(0.28, h, 6, glow(CRYSTAL_CYAN, 3.0)),
            [cx, plinth_h + h * 0.5, cz],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: the crystal's shimmer and rising mana motes.
    root.audio = fx::crystal_shimmer();
    root.children
        .push(fx::mana_motes([0.0, plinth_h + 1.5, 0.0], 0x0A1A_C512));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CrystalShrine.build(""), "crystal_shrine");
    }
}
