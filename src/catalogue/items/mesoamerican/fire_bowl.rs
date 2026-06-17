//! Fire bowl — a Mesoamerican prop, and the kit's lit hero. A stone brazier
//! on a stepped pedestal holding a burning offering: leaping flame, lofted
//! embers, and a fire crackle. Its emissive core is the trim escalation's
//! ruin pass snuffs to a cold dead bowl.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, OBSIDIAN_BLACK, STONE_GREY, STUCCO_CREAM, cobble, fx, limestone, obsidian,
};

pub struct FireBowl;

impl CatalogueEntry for FireBowl {
    fn slug(&self) -> &'static str {
        "fire_bowl"
    }
    fn name(&self) -> &'static str {
        "Fire Bowl"
    }
    fn description(&self) -> &'static str {
        "Stone brazier on a stepped pedestal burning an offering fire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
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
        // Stepped pedestal base — the root.
        prim(
            solid(cuboid_tapered(
                [1.2, 0.4, 1.2],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Column.
        prim(
            solid(cylinder_tapered(0.3, 1.1, 10, 0.1, cobble(STONE_GREY))),
            [0.0, 0.95, 0.0],
            id_quat(),
        ),
    ];

    // Obsidian-rimmed stone bowl.
    let bowl_y = 1.7;
    prims.push(prim(
        solid(cylinder_tapered(0.6, 0.5, 14, 0.3, cobble(STONE_GREY))),
        [0.0, bowl_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.09, 0.6, obsidian(OBSIDIAN_BLACK)),
        [0.0, bowl_y + 0.25, 0.0],
        id_quat(),
    ));

    // Glowing fire core — the emissive heart.
    prims.push(prim(
        sphere(0.42, 3, glow(FIRE_ORANGE, 5.0)),
        [0.0, bowl_y + 0.3, 0.0],
        id_quat(),
    ));

    let flame_y = bowl_y + 0.45;
    let mut root = assemble(prims);
    // Signature life: leaping flame, lofted embers, and a fire crackle.
    let mut flame = fx::sacred_flame([0.0, flame_y, 0.0], 0xF1A0_B0E1);
    flame.audio = fx::fire_crackle();
    root.children.push(flame);
    root.children
        .push(fx::fire_embers([0.0, flame_y + 0.3, 0.0], 0xE3BE_B0E1));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FireBowl.build(""), "fire_bowl");
    }

    #[test]
    fn has_fire() {
        assert!(super::super::has_emissive(&FireBowl.build("")));
    }
}
