//! Blacksmith — a Medieval secondary and the kit's firelit hero. A
//! timber-framed open forge with daub-infilled back and side walls, a
//! tall fieldstone chimney, a glowing stone hearth, and an iron anvil on
//! an oak stump out under the open front. Sooty smoke streams from the
//! chimney, sparks leap off the anvil, and a fire crackle plays at the
//! hearth; its emissive forge mouth is the trim escalation's ruin pass
//! snuffs to a cold dead hearth.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DAUB_CREAM, FORGE_ORANGE, IRON_DARK, SLATE_GREY, STONE_GREY, WOOD_DARK, WOOD_OAK, daub, fx,
    iron, rough_stone, shingle, stone, timber,
};

pub struct Blacksmith;

impl CatalogueEntry for Blacksmith {
    fn slug(&self) -> &'static str {
        "blacksmith"
    }
    fn name(&self) -> &'static str {
        "Blacksmith"
    }
    fn description(&self) -> &'static str {
        "Timber-framed open forge with a fieldstone chimney, glowing hearth, and anvil."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 5.5_f32; // along X, open toward +X
    let w = 4.5_f32; // along Z
    let foot_h = 0.35;
    let wall_h = 3.2;
    let wall_top = foot_h + wall_h;
    let back = -l * 0.5;

    let mut prims = vec![
        // Fieldstone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 1.0, foot_h, w + 1.0],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Daub back wall.
        prim(
            solid(cuboid_tapered([0.35, wall_h, w], 0.0, daub(DAUB_CREAM))),
            [back + 0.18, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Daub side walls, short (open front).
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [l * 0.7, wall_h, 0.35],
                0.0,
                daub(DAUB_CREAM),
            )),
            [
                back + l * 0.35,
                foot_h + wall_h * 0.5,
                sz * (w * 0.5 - 0.18),
            ],
            id_quat(),
        ));
    }
    // Timber corner posts + a front lintel across the open mouth.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.28, wall_h, 0.28], 0.0, timber(WOOD_DARK))),
            [
                sx * (l * 0.5 - 0.14),
                foot_h + wall_h * 0.5,
                sz * (w * 0.5 - 0.14),
            ],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([0.3, 0.4, w], 0.0, timber(WOOD_OAK))),
        [l * 0.5 - 0.14, wall_top - 0.2, 0.0],
        id_quat(),
    ));

    // Slate lean roof, overhanging the open front.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.2, 1.6, w + 1.0],
            0.3,
            shingle(SLATE_GREY),
        )),
        [0.2, wall_top + 0.8, 0.0],
        id_quat(),
    ));

    // Tall fieldstone chimney at the back corner.
    let chim = [back + 0.5, 0.0, w * 0.5 - 0.6];
    let chim_h = wall_h + 2.6;
    prims.push(prim(
        solid(cuboid_tapered(
            [1.0, chim_h, 1.0],
            0.08,
            rough_stone(STONE_GREY),
        )),
        [chim[0], foot_h + chim_h * 0.5, chim[2]],
        id_quat(),
    ));

    // Stone hearth block at the back, beneath the chimney.
    let hearth_x = back + 0.7;
    prims.push(prim(
        solid(cuboid_tapered([1.3, 1.3, 1.4], 0.0, stone(STONE_GREY))),
        [hearth_x, foot_h + 0.65, w * 0.5 - 0.7],
        id_quat(),
    ));
    // Glowing forge mouth set into the hearth — the emissive heart, crackling.
    let mouth = [hearth_x + 0.6, foot_h + 0.7, w * 0.5 - 0.7];
    let mut fire = prim(
        cuboid_tapered([0.5, 0.55, 0.5], 0.0, glow(FORGE_ORANGE, 4.0)),
        mouth,
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    prims.push(fire);

    // Anvil on an oak stump out under the open front.
    let anvil = [l * 0.5 - 1.4, 0.0, -w * 0.25];
    prims.push(prim(
        solid(cylinder_tapered(0.32, 0.7, 10, 0.06, timber(WOOD_OAK))),
        [anvil[0], foot_h + 0.35, anvil[2]],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.9, 0.32, 0.4], 0.0, iron(IRON_DARK))),
        [anvil[0], foot_h + 0.86, anvil[2]],
        id_quat(),
    ));
    // Water-quench trough beside the anvil.
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.5, 1.0], 0.0, timber(WOOD_DARK))),
        [anvil[0] - 0.1, foot_h + 0.25, anvil[2] + 1.1],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: chimney smoke, forge flame at the mouth, anvil sparks.
    root.children.push(fx::forge_smoke(
        [chim[0], foot_h + chim_h + 0.4, chim[2]],
        0x510E_DA11,
    ));
    root.children.push(fx::forge_flame(
        [mouth[0] + 0.2, mouth[1] + 0.2, mouth[2]],
        0xF1A3_0E12,
    ));
    root.children.push(fx::forge_sparks(
        [anvil[0], foot_h + 1.1, anvil[2]],
        0x0E3B_E012,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Blacksmith.build(""), "blacksmith");
    }

    #[test]
    fn keeps_forge_fire() {
        assert!(super::super::has_emissive(&Blacksmith.build("")));
    }
}
