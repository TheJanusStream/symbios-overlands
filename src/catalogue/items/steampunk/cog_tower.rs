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

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, quat_x,
    solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, BRICK_SOOT, FURNACE_ORANGE, GAUGE_AMBER, IRON_DARK, brass, brick, fx, iron};

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
    // Brass bands.
    for y in [base_h + 2.0, base_h + shaft_h - 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([3.7, 0.4, 3.7], 0.0, brass(BRASS))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Furnace vents glowing at the base — emissive.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.9, 0.6, 0.12], 0.0, glow(FURNACE_ORANGE, 2.5)),
            [sx * 0.9, base_h * 0.5 + 0.1, 2.55],
            id_quat(),
        ));
    }

    // Great exposed brass gear on the +Z face, with teeth around its rim.
    let gear_y = base_h + 4.5;
    let gear_r = 1.7_f32;
    prims.push(prim(
        solid(cylinder_tapered(gear_r, 0.3, 24, 0.0, brass(BRASS))),
        [0.0, gear_y, 2.0],
        quat_x(FRAC_PI_2),
    ));
    // Hub.
    prims.push(prim(
        solid(cylinder_tapered(0.4, 0.45, 12, 0.0, iron(IRON_DARK))),
        [0.0, gear_y, 2.0],
        quat_x(FRAC_PI_2),
    ));
    // Teeth around the rim (in the disc's world XY plane).
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU;
        prims.push(prim(
            solid(cuboid_tapered([0.3, 0.3, 0.35], 0.0, brass(BRASS))),
            [
                a.cos() * (gear_r + 0.1),
                gear_y + a.sin() * (gear_r + 0.1),
                2.0,
            ],
            id_quat(),
        ));
    }
    // A smaller interlocking iron cog beside it.
    prims.push(prim(
        solid(cylinder_tapered(0.8, 0.25, 18, 0.0, iron(IRON_DARK))),
        [1.9, gear_y - 1.3, 2.0],
        quat_x(FRAC_PI_2),
    ));

    // Glowing clock dial high on the face — emissive.
    let clock_y = base_h + shaft_h - 2.4;
    prims.push(prim(
        solid(torus(0.18, 1.0, brass(BRASS))),
        [0.0, clock_y, 2.0],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cuboid_tapered([1.6, 1.6, 0.12], 0.0, glow(GAUGE_AMBER, 3.0)),
        [0.0, clock_y, 1.98],
        id_quat(),
    ));

    // Iron cap + brass finial.
    prims.push(prim(
        solid(cuboid_tapered([4.0, 0.6, 4.0], 0.2, iron(IRON_DARK))),
        [0.0, shaft_top + 0.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.2, 1.2, 8, 0.3, brass(BRASS))),
        [0.0, shaft_top + 1.2, 0.0],
        id_quat(),
    ));
    // Steam pipe at the corner of the cap.
    prims.push(prim(
        solid(cylinder_tapered(0.22, 1.4, 8, 0.0, brass(BRASS))),
        [1.3, shaft_top + 1.0, 1.3],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the engine chug, steam from the corner pipe.
    root.audio = fx::engine_chug();
    root.children
        .push(fx::steam_vent([1.3, shaft_top + 1.8, 1.3], 0x57EA_C061));
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
