//! Wizard tower — the High-Fantasy landmark and the kit's lit hero. A tall
//! tapering stone tower with arcane-lit windows, a steep slate cone cap and a
//! glowing crystal orb on a gold spire, ringed by floating runestones. ~14 m
//! tall, so it anchors the arcane quarter and reads as the mage's seat from
//! across the home region. Its windows, orb and runes are the trim
//! escalation's ruin pass snuffs to a cold, dead spire.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the stone base.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid,
    sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ARCANE_GLASS, ARCANE_PURPLE, GOLD, RUNE_GOLD, STONE_GREY, TIMBER_DARK, fx, glass, gold, stone,
    timber,
};

pub struct WizardTower;

impl CatalogueEntry for WizardTower {
    fn slug(&self) -> &'static str {
        "wizard_tower"
    }
    fn name(&self) -> &'static str {
        "Wizard Tower"
    }
    fn description(&self) -> &'static str {
        "Tapering stone tower with arcane-lit windows, a slate cone and a glowing crystal orb."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
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
    let base_h = 1.0_f32;
    let shaft_h = 12.0_f32;
    let shaft_top = base_h + shaft_h;

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([5.0, base_h, 5.0], 0.0, stone(STONE_GREY))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(5.0, 5.0, [0.0, 0.0], 1.5));

    // Tapering stone shaft.
    prims.push(prim(
        solid(cylinder_tapered(2.4, shaft_h, 16, 0.4, stone(STONE_GREY))),
        [0.0, base_h + shaft_h * 0.5, 0.0],
        id_quat(),
    ));
    // Gold bands.
    for y in [base_h + 3.0, base_h + shaft_h - 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(2.1, 0.3, 16, 0.4, gold(GOLD))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Arcane-lit windows up the +Z face — emissive.
    for (k, y) in [base_h + 2.5, base_h + 5.5, base_h + 8.5]
        .into_iter()
        .enumerate()
    {
        let r = 2.2 - k as f32 * 0.25;
        prims.push(prim(
            cuboid_tapered([0.8, 1.6, 0.2], 0.0, glass(ARCANE_GLASS, 1.6)),
            [0.0, y, r],
            id_quat(),
        ));
    }

    // Timber door at the base.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 1.9, 0.2], 0.0, timber(TIMBER_DARK))),
        [0.0, base_h + 0.95, 2.4],
        id_quat(),
    ));

    // Steep slate cone cap.
    prims.push(prim(
        solid(cone(1.6, 4.0, 16, stone(STONE_GREY))),
        [0.0, shaft_top + 2.0, 0.0],
        id_quat(),
    ));
    // Gold spire + glowing crystal orb — emissive.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.2, 6, 0.4, gold(GOLD))),
        [0.0, shaft_top + 4.4, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.5, 3, glow(ARCANE_PURPLE, 4.0)),
        [0.0, shaft_top + 5.2, 0.0],
        id_quat(),
    ));

    // Floating runestones orbiting the upper shaft — emissive glyphs.
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU;
        let mut rune = prim(
            solid(cuboid_tapered([0.5, 0.8, 0.18], 0.0, stone(STONE_GREY))),
            [a.cos() * 3.2, shaft_top - 1.0, a.sin() * 3.2],
            id_quat(),
        );
        rune.children.push(prim(
            cuboid_tapered([0.3, 0.5, 0.22], 0.0, glow(RUNE_GOLD, 3.0)),
            [0.0, 0.0, 0.0],
            id_quat(),
        ));
        prims.push(rune);
    }

    let mut root = assemble(prims);
    // Signature life: an arcane hum, sparkles whirling around the orb.
    root.audio = fx::arcane_hum();
    root.children
        .push(fx::arcane_sparkle([0.0, shaft_top + 5.2, 0.0], 0x0A5C_0B12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WizardTower.build(""), "wizard_tower");
    }

    #[test]
    fn has_arcane_glow() {
        assert!(super::super::has_emissive(&WizardTower.build("")));
    }
}
