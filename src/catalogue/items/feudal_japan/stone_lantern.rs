//! Stone lantern — a Feudal-Japan prop, and the kit's lit hero. A stacked
//! granite ishidōrō: a footed base, a column, a platform, a glowing light
//! box (hibukuro), a pyramidal cap, and an onion finial. Its emissive light
//! box is the trim escalation's ruin pass snuffs to cold dead stone.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{LANTERN_GLOW, STONE_GREY, stone};

pub struct StoneLantern;

impl CatalogueEntry for StoneLantern {
    fn slug(&self) -> &'static str {
        "stone_lantern"
    }
    fn name(&self) -> &'static str {
        "Stone Lantern"
    }
    fn description(&self) -> &'static str {
        "Stacked granite lantern with a glowing light box under a pyramidal cap."
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
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Footed base — the root.
        prim(
            solid(cuboid_tapered([0.9, 0.4, 0.9], 0.15, stone(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Column.
        prim(
            solid(cylinder_tapered(0.18, 1.6, 8, 0.05, stone(STONE_GREY))),
            [0.0, 1.2, 0.0],
            id_quat(),
        ),
        // Platform under the light box.
        prim(
            solid(cuboid_tapered([0.85, 0.2, 0.85], 0.1, stone(STONE_GREY))),
            [0.0, 2.1, 0.0],
            id_quat(),
        ),
    ];

    // Light box: a stone frame around a glowing core, with four corner posts.
    let box_y = 2.65;
    prims.push(prim(
        cuboid_tapered([0.42, 0.5, 0.42], 0.0, glow(LANTERN_GLOW, 3.0)),
        [0.0, box_y, 0.0],
        id_quat(),
    ));
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.6, 0.1], 0.0, stone(STONE_GREY))),
            [sx * 0.28, box_y, sz * 0.28],
            id_quat(),
        ));
    }

    // Pyramidal cap (kasa) and onion finial (hōju).
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.45, 1.0], 0.7, stone(STONE_GREY))),
        [0.0, box_y + 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(0.16, 3, stone(STONE_GREY))),
        [0.0, box_y + 0.85, 0.0],
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
        assert_sanitize_stable(&StoneLantern.build(""), "stone_lantern");
    }

    #[test]
    fn has_light() {
        assert!(super::super::has_emissive(&StoneLantern.build("")));
    }
}
