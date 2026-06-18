//! Cathedral — the Gothic-Horror landmark and the kit's lit hero. A tall dark
//! stone nave with a great glowing rose window and lancets, buttress piers
//! with pinnacles, a steep slate roof and twin front spires. ~14 m wide, so it
//! looms over the necropolis and reads as the cathedral from across the home
//! region. Its stained glass is the trim escalation's ruin pass snuffs to a
//! black, gutted shell.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the stone base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, foundation_block, id_quat, prim, quat_x,
    solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEADWOOD, STAINED_GLOW, STAINED_TINT, STONE_DARK, fx, stained, stone, wood};

pub struct Cathedral;

impl CatalogueEntry for Cathedral {
    fn slug(&self) -> &'static str {
        "cathedral"
    }
    fn name(&self) -> &'static str {
        "Cathedral"
    }
    fn description(&self) -> &'static str {
        "Dark stone nave with a glowing rose window, buttress piers and twin spires."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 12.0,
            min_spawn_dist: 54.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 1.0_f32;
    let w = 11.0_f32;
    let d = 7.0_f32;
    let nave_h = 8.0_f32;
    let nave_top = base_h + nave_h;
    let half_d = d * 0.5;

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([14.0, base_h, 9.0], 0.0, stone(STONE_DARK))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(14.0, 9.0, [0.0, 0.0], 1.5));

    // Stone nave.
    prims.push(prim(
        solid(cuboid_tapered([w, nave_h, d], 0.0, stone(STONE_DARK))),
        [0.0, base_h + nave_h * 0.5, 0.0],
        id_quat(),
    ));

    // Lancet windows + a great rose window on the +Z front — emissive.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([1.0, 3.6, 0.2], 0.0, stained(STAINED_TINT, 2.2)),
            [sx * 3.0, base_h + 2.6, half_d + 0.02],
            id_quat(),
        ));
    }
    prims.push(prim(
        cylinder_tapered(1.5, 0.3, 16, 0.0, stained(STAINED_GLOW, 2.4)),
        [0.0, base_h + 5.6, half_d + 0.05],
        quat_x(FRAC_PI_2),
    ));

    // Side lancets — emissive.
    for sx in [-1.0_f32, 1.0] {
        for z in [-1.8_f32, 1.8] {
            prims.push(prim(
                cuboid_tapered([0.2, 3.0, 0.9], 0.0, stained(STAINED_TINT, 2.0)),
                [sx * (w * 0.5 + 0.02), base_h + 2.5, z],
                id_quat(),
            ));
        }
    }

    // Buttress piers with pinnacles down the long walls.
    for sx in [-1.0_f32, 1.0] {
        for z in [-2.2_f32, 0.0, 2.2] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.8, nave_h - 0.5, 1.0],
                    0.0,
                    stone(STONE_DARK),
                )),
                [sx * (w * 0.5 + 0.4), base_h + (nave_h - 0.5) * 0.5, z],
                id_quat(),
            ));
            prims.push(prim(
                solid(cone(0.5, 1.4, 6, stone(STONE_DARK))),
                [sx * (w * 0.5 + 0.4), nave_top + 0.2, z],
                id_quat(),
            ));
        }
    }

    // Steep slate gable roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.6, 3.5, d + 0.6],
            0.55,
            stone(STONE_DARK),
        )),
        [0.0, nave_top + 1.75, 0.0],
        id_quat(),
    ));

    // Pointed arch door + dark timber door on the front.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 3.4, 0.4], 0.2, stone(STONE_DARK))),
        [0.0, base_h + 1.7, half_d + 0.1],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.8, 2.8, 0.2], 0.1, wood(DEADWOOD))),
        [0.0, base_h + 1.4, half_d + 0.32],
        id_quat(),
    ));

    // Twin front spires.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.6, 3.0, 1.6], 0.0, stone(STONE_DARK))),
            [sx * (w * 0.5 - 0.8), nave_top + 1.5, half_d - 0.8],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(1.0, 4.0, 8, stone(STONE_DARK))),
            [sx * (w * 0.5 - 0.8), nave_top + 5.0, half_d - 0.8],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: a ghostly drone in the nave, mist creeping outside.
    root.audio = fx::ghostly_drone();
    root.children
        .push(fx::ground_mist([0.0, 0.3, half_d + 4.0], 0x60F0_CA12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Cathedral.build(""), "cathedral");
    }

    #[test]
    fn has_stained_glow() {
        assert!(super::super::has_emissive(&Cathedral.build("")));
    }
}
