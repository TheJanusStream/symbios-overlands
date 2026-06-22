//! Cog tower — the Steampunk landmark and the kit's lit hero. A riveted iron
//! tower on a sooty brick base, its face dominated by a great exposed brass
//! gear and a glowing clock dial, furnace vents glowing at its foot and steam
//! venting from a pipe at the top. ~12 m tall, so it anchors the works and
//! reads as the engine of the quarter from across the home region. Its clock
//! and furnace glow are the trim escalation's ruin pass snuffs to a cold,
//! seized tower.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the brick base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, quat_x,
    solid, sphere, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRASS, BRICK_SOOT, COPPER_ORANGE, FURNACE_ORANGE, GAUGE_AMBER, IRON_DARK, brass, brick, cog,
    copper, fx, iron,
};

pub struct CogTower;

impl CatalogueEntry for CogTower {
    fn slug(&self) -> &'static str {
        "cog_tower"
    }
    fn name(&self) -> &'static str {
        "Cog Tower"
    }
    fn description(&self) -> &'static str {
        "Riveted iron tower with a great exposed brass gear, a glowing clock and steam vents."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 1.2_f32;
    let shaft_h = 10.0_f32;
    let shaft_top = base_h + shaft_h;
    // Hero face = −Z (the render front); detail rides slightly proud of it.
    let gear_z = -1.95_f32;

    let mut prims = vec![
        // Sooty brick base — the root.
        prim(
            solid(cuboid_tapered([5.0, base_h, 5.0], 0.0, brick(BRICK_SOOT))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(5.0, 5.0, [0.0, 0.0], 1.5));

    // Riveted iron shaft.
    prims.push(prim(
        solid(cuboid_tapered([3.5, shaft_h, 3.5], 0.04, iron(IRON_DARK))),
        [0.0, base_h + shaft_h * 0.5, 0.0],
        id_quat(),
    ));
    // Exposed corner straps — break the flat box into an industrial frame.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.34, shaft_h, 0.34], 0.04, iron(IRON_DARK))),
                [sx * 1.86, base_h + shaft_h * 0.5, sz * 1.86],
                id_quat(),
            ));
        }
    }
    // Brass bands.
    for y in [base_h + 2.0, base_h + shaft_h - 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([3.8, 0.4, 3.8], 0.0, brass(BRASS))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Furnace vents glowing at the base of the hero (−Z) face — emissive.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.9, 0.6, 0.12], 0.0, glow(FURNACE_ORANGE, 2.5)),
            [sx * 0.9, base_h * 0.5 + 0.1, -2.55],
            id_quat(),
        ));
    }

    // Exposed gear train on the hero (−Z) face — the signature silhouette.
    // cog() lies flat; quat_x(−π/2) stands it to face −Z, teeth ringing it.
    let gear_y = base_h + 4.7;
    prims.push(cog(
        [0.0, gear_y, gear_z],
        quat_x(-FRAC_PI_2),
        1.7,
        0.32,
        16,
        brass(BRASS),
        iron(IRON_DARK),
    ));
    prims.push(cog(
        [1.98, gear_y - 1.7, gear_z],
        quat_x(-FRAC_PI_2),
        0.85,
        0.3,
        12,
        iron(IRON_DARK),
        brass(BRASS),
    ));
    prims.push(cog(
        [-1.72, gear_y + 1.55, gear_z],
        quat_x(-FRAC_PI_2),
        0.6,
        0.28,
        10,
        brass(BRASS),
        iron(IRON_DARK),
    ));

    // Glowing clock dial high on the hero face — emissive, with hands.
    let clock_y = base_h + shaft_h - 2.2;
    prims.push(prim(
        solid(torus(0.2, 1.1, brass(BRASS))),
        [0.0, clock_y, -1.9],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cuboid_tapered([1.7, 1.7, 0.12], 0.0, glow(GAUGE_AMBER, 3.0)),
        [0.0, clock_y, -1.9],
        id_quat(),
    ));
    // Hour + minute hands, proud of the dial.
    prims.push(prim(
        cuboid_tapered([0.1, 0.85, 0.06], 0.0, iron(IRON_DARK)),
        [0.0, clock_y + 0.32, -2.02],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.6, 0.1, 0.06], 0.0, iron(IRON_DARK)),
        [0.22, clock_y, -2.02],
        id_quat(),
    ));

    // Iron cornice + machined copper cupola dome + brass finial.
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.6, 4.4], 0.1, iron(IRON_DARK))),
        [0.0, shaft_top + 0.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(with_cut(
            sphere(1.7, 6, copper(COPPER_ORANGE)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, shaft_top + 0.6, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.18, 1.4, 8, 0.4, brass(BRASS))),
        [0.0, shaft_top + 2.5, 0.0],
        id_quat(),
    ));

    // Hollow steam vent stack at a back corner of the cornice, with a flared
    // collar — tall enough to read as a vent against the ~12 m tower.
    prims.push(prim(
        solid(tube(0.34, 0.22, 3.0, 10, brass(BRASS))),
        [1.45, shaft_top + 1.9, 1.45],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.1, 0.42, brass(BRASS))),
        [1.45, shaft_top + 3.3, 1.45],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the engine chug, steam from the corner vent stack.
    root.audio = fx::engine_chug();
    root.children
        .push(fx::steam_vent([1.45, shaft_top + 3.7, 1.45], 0x57EA_C061));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CogTower.build(""), "cog_tower");
    }

    #[test]
    fn has_lit_clock_and_furnace() {
        assert!(crate::catalogue::items::util::has_emissive(
            &CogTower.build("")
        ));
    }
}
