//! Blacksmith — a Medieval secondary and the kit's firelit hero. A
//! timber-framed open forge, daub-walled on three sides and open to the −Z
//! front where the work is done: a tall corbelled fieldstone chimney with a
//! tapered smoke hood over a glowing stone hearth, an iron anvil on an oak
//! stump, a water-quench barrel, a treadle grindstone, and a tool rack of
//! hanging tongs and hammers. Sooty smoke streams from the chimney, sparks
//! leap off the anvil, and a fire crackle plays at the hearth; its emissive
//! forge mouth is what the trim escalation's ruin pass snuffs to a cold dead
//! hearth.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_z, solid, torus,
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
        "Timber-framed open forge with a corbelled chimney, smoke hood, glowing hearth, and anvil."
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
    let lx = 5.6_f32; // along X
    let dz = 4.6_f32; // along Z; open toward −Z (camera)
    let foot_h = 0.35;
    let wall_h = 3.2;
    let wall_top = foot_h + wall_h;
    let back = dz * 0.5; // +Z back wall line

    let mut prims = vec![
        // Fieldstone footing — the root.
        prim(
            solid(cuboid_tapered(
                [lx + 1.0, foot_h, dz + 1.0],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Daub back wall (+Z).
        prim(
            solid(cuboid_tapered([lx, wall_h, 0.35], 0.0, daub(DAUB_CREAM))),
            [0.0, foot_h + wall_h * 0.5, back - 0.18],
            id_quat(),
        ),
    ];

    // Daub side walls, two-thirds deep (open front).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.35, wall_h, dz * 0.66],
                0.0,
                daub(DAUB_CREAM),
            )),
            [
                sx * (lx * 0.5 - 0.18),
                foot_h + wall_h * 0.5,
                back - dz * 0.33,
            ],
            id_quat(),
        ));
    }
    // Timber corner posts + a front lintel across the open mouth.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.28, wall_h, 0.28], 0.0, timber(WOOD_DARK))),
            [
                sx * (lx * 0.5 - 0.14),
                foot_h + wall_h * 0.5,
                sz * (dz * 0.5 - 0.14),
            ],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([lx, 0.4, 0.3], 0.0, timber(WOOD_OAK))),
        [0.0, wall_top - 0.2, -(dz * 0.5 - 0.14)],
        id_quat(),
    ));

    // Slate mono-pitch roof, high at the back, overhanging the open front.
    prims.push(prim(
        solid(cuboid_tapered(
            [lx + 1.2, 0.32, dz + 1.5],
            0.0,
            shingle(SLATE_GREY),
        )),
        [0.0, wall_top + 0.55, 0.1],
        quat_x(-0.3),
    ));

    // Tall corbelled fieldstone chimney at the back-left corner.
    let chim = [-lx * 0.5 + 0.9, 0.0, back - 0.6];
    let chim_h = wall_h + 2.8;
    prims.push(prim(
        solid(cuboid_tapered(
            [1.0, chim_h, 1.0],
            0.06,
            rough_stone(STONE_GREY),
        )),
        [chim[0], foot_h + chim_h * 0.5, chim[2]],
        id_quat(),
    ));
    // Corbelled flared cap.
    prims.push(prim(
        solid(cuboid_tapered([1.3, 0.4, 1.3], 0.0, stone(STONE_GREY))),
        [chim[0], foot_h + chim_h + 0.1, chim[2]],
        id_quat(),
    ));

    // Stone hearth block under the chimney, its glowing mouth facing −Z.
    let hx = chim[0];
    let hearth_z = back - 0.95;
    prims.push(prim(
        solid(cuboid_tapered([1.4, 1.2, 1.3], 0.0, stone(STONE_GREY))),
        [hx, foot_h + 0.6, hearth_z],
        id_quat(),
    ));
    // Tapered daub smoke hood funnelling hearth → chimney.
    prims.push(prim(
        solid(cuboid_tapered([1.5, 1.4, 1.4], 0.55, daub(DAUB_CREAM))),
        [hx, foot_h + 1.9, hearth_z + 0.05],
        id_quat(),
    ));
    // Glowing forge mouth set into the −Z face of the hearth — the emissive
    // heart, crackling. A flat face so the glow reads cleanly.
    let mouth = [hx, foot_h + 0.7, hearth_z - 0.66];
    let mut fire = prim(
        cuboid_tapered([0.7, 0.6, 0.12], 0.0, glow(FORGE_ORANGE, 4.5)),
        mouth,
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    prims.push(fire);

    // Anvil on an oak stump out front-centre, facing the open mouth.
    let anvil = [0.6, 0.0, -0.4];
    prims.push(prim(
        solid(cylinder_tapered(0.32, 0.7, 10, 0.06, timber(WOOD_OAK))),
        [anvil[0], foot_h + 0.35, anvil[2]],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.95, 0.32, 0.42], 0.0, iron(IRON_DARK))),
        [anvil[0], foot_h + 0.86, anvil[2]],
        id_quat(),
    ));
    // Horn of the anvil, a tapered nub off the +X end.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 0.34, 8, 0.6, iron(IRON_DARK))),
        [anvil[0] + 0.6, foot_h + 0.92, anvil[2]],
        quat_z(FRAC_PI_2),
    ));
    // A hammer left lying on the anvil face.
    prims.push(prim(
        solid(cylinder_tapered(0.05, 0.5, 6, 0.0, timber(WOOD_OAK))),
        [anvil[0] - 0.1, foot_h + 1.05, anvil[2] - 0.1],
        quat_z(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.18, 0.12, 0.12], 0.0, iron(IRON_DARK))),
        [anvil[0] - 0.36, foot_h + 1.05, anvil[2] - 0.1],
        id_quat(),
    ));

    // Water-quench barrel beside the anvil.
    let mut barrel = prim(
        solid(cylinder_tapered(0.4, 0.95, 14, -0.12, timber(WOOD_DARK))),
        [-1.5, foot_h + 0.475, -0.7],
        id_quat(),
    );
    for dy in [0.28_f32, -0.28] {
        barrel.children.push(prim(
            torus(0.035, 0.41, iron(IRON_DARK)),
            [0.0, dy, 0.0],
            id_quat(),
        ));
    }
    prims.push(barrel);

    // Treadle grindstone: a vertical sandstone wheel on a timber frame, right side.
    let grind = [1.9_f32, foot_h + 0.7, 0.4];
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.7, 0.1], 0.0, timber(WOOD_DARK))),
            [grind[0], foot_h + 0.35, grind[2] + sz * 0.28],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cylinder_tapered(0.36, 0.12, 14, 0.0, stone(STONE_GREY))),
        grind,
        quat_x(FRAC_PI_2),
    ));

    // Tool rack on the +X side wall: a bar with two hanging tongs.
    let rack_x = lx * 0.5 - 0.4;
    prims.push(prim(
        solid(cuboid_tapered([0.08, 0.08, 1.4], 0.0, timber(WOOD_DARK))),
        [rack_x, foot_h + 2.3, back - 1.4],
        id_quat(),
    ));
    for dz2 in [-0.4_f32, 0.3] {
        prims.push(prim(
            solid(cylinder_tapered(0.03, 0.8, 6, 0.0, iron(IRON_DARK))),
            [rack_x - 0.1, foot_h + 1.9, back - 1.4 + dz2],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: chimney smoke, forge flame at the mouth, anvil sparks.
    root.children.push(fx::forge_smoke(
        [chim[0], foot_h + chim_h + 0.5, chim[2]],
        0x510E_DA11,
    ));
    root.children.push(fx::forge_flame(
        [mouth[0], mouth[1] + 0.2, mouth[2] - 0.1],
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
        assert!(crate::catalogue::items::util::has_emissive(
            &Blacksmith.build("")
        ));
    }
}
