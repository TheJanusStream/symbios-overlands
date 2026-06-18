//! Fortified ruin — the Post-apocalyptic landmark and the kit's lit hero. A
//! gutted concrete building patched with welded scrap and sandbags, a lookout
//! platform with a salvaged worklight and a burning barrel at the gate. ~10 m
//! across, so it anchors the holdout and reads as the stronghold from across
//! the home region. Its barrel fire and worklight are the trim escalation's
//! ruin pass snuffs to a cold, dark husk.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the slab.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, quat_x,
    solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_RUST, FIRE_ORANGE, RUST_BROWN, STEEL_GREY, TARP_FADED, WORKLIGHT,
    concrete, fx, rusted, sheet, tarp,
};

pub struct FortifiedRuin;

impl CatalogueEntry for FortifiedRuin {
    fn slug(&self) -> &'static str {
        "fortified_ruin"
    }
    fn name(&self) -> &'static str {
        "Fortified Ruin"
    }
    fn description(&self) -> &'static str {
        "Gutted concrete building patched with scrap and sandbags, a worklight and a barrel fire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 11.0,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [10.0, base_h, 8.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(10.0, 8.0, [0.0, 0.0], 1.5));

    // Surviving concrete walls at broken heights (back + two sides).
    prims.push(prim(
        solid(cuboid_tapered(
            [10.0, 5.0, 0.6],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, base_h + 2.5, -3.7],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, 4.2, 7.0],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [-4.7, base_h + 2.1, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, 2.6, 7.0],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [4.7, base_h + 1.3, 0.0],
        id_quat(),
    ));
    // Broken front wall stubs flanking the gate.
    for (sx, h) in [(-1.0_f32, 3.0_f32), (1.0, 1.8)] {
        prims.push(prim(
            solid(cuboid_tapered([2.6, h, 0.6], 0.0, concrete(CONCRETE_GREY))),
            [sx * 3.3, base_h + h * 0.5, 3.7],
            id_quat(),
        ));
    }

    // Welded scrap-sheet reinforcement on the back wall.
    prims.push(prim(
        solid(cuboid_tapered([4.0, 3.0, 0.2], 0.0, sheet(CORRUGATED_RUST))),
        [1.5, base_h + 1.8, -3.4],
        id_quat(),
    ));
    // Sandbag stack at the gate.
    for k in 0..3 {
        prims.push(prim(
            solid(cuboid_tapered([2.0, 0.4, 0.7], 0.1, tarp(TARP_FADED))),
            [-3.3, base_h + 0.2 + k as f32 * 0.4, 3.4],
            id_quat(),
        ));
    }

    // Lookout platform on the tall corner, with a worklight — emissive.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 0.3, 2.6], 0.0, rusted(STEEL_GREY))),
        [-3.4, base_h + 4.3, -2.4],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.08, 1.4, 6, 0.0, rusted(STEEL_GREY))),
        [-3.4, base_h + 5.1, -2.4],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.3, 0.4], 0.0, glow(WORKLIGHT, 3.0)),
        [-3.4, base_h + 5.7, -2.2],
        quat_x(0.4),
    ));

    // Burning barrel at the gate — emissive, with flame + crackle.
    let barrel = [3.3_f32, 3.6_f32];
    prims.push(prim(
        solid(cylinder_tapered(0.4, 1.0, 12, 0.0, rusted(RUST_BROWN))),
        [barrel[0], base_h + 0.5, barrel[1]],
        id_quat(),
    ));
    let mut fire = prim(
        solid(cylinder_tapered(0.34, 0.4, 10, 0.0, glow(FIRE_ORANGE, 4.0))),
        [barrel[0], base_h + 1.1, barrel[1]],
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    prims.push(fire);

    let mut root = assemble(prims);
    // Signature life: desolate wind, drifting ash, the barrel flame.
    root.audio = fx::desolate_wind();
    root.children
        .push(fx::ash_drift([0.0, 0.6, 5.0], 0x0A57_C012));
    root.children.push(fx::fire_flame(
        [barrel[0], base_h + 1.4, barrel[1]],
        0x0A57_F112,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FortifiedRuin.build(""), "fortified_ruin");
    }

    #[test]
    fn has_fire_and_worklight() {
        assert!(super::super::has_emissive(&FortifiedRuin.build("")));
    }
}
