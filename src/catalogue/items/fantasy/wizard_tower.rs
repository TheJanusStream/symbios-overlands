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

use crate::catalogue::items::gothic_horror::pointed_arch;
use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid,
    sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ARCANE_GLASS, ARCANE_PURPLE, CRYSTAL_CYAN, GOLD, RUNE_GOLD, STONE_GREY, TIMBER_DARK, crystal,
    fx, glass, gold, rune_marks, stone, timber,
};

/// Dark slate of the floating runestones — a cold backing the gold glyphs read
/// on, the [`runestone`](super::runestone) slate.
const SLATE: [f32; 3] = [0.33, 0.32, 0.37];

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
    let shaft_r0 = 2.4_f32;
    let taper = 0.4_f32;
    // Outer radius of the tapering shaft at world height `y`.
    let rad = |y: f32| shaft_r0 * (1.0 - taper * ((y - base_h) / shaft_h).clamp(0.0, 1.0));

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
        solid(cylinder_tapered(
            shaft_r0,
            shaft_h,
            16,
            taper,
            stone(STONE_GREY),
        )),
        [0.0, base_h + shaft_h * 0.5, 0.0],
        id_quat(),
    ));
    // Gold string-course bands hugging the shaft.
    for y in [base_h + 2.0, base_h + 6.0] {
        prims.push(prim(
            solid(torus(0.14, rad(y) + 0.04, gold(GOLD))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Arcane-lit windows climbing the −Z (front) face, each under a pointed
    // arch — emissive panes set into the wall.
    for y in [base_h + 2.6, base_h + 5.6, base_h + 8.6] {
        let zf = -(rad(y) - 0.08);
        prims.push(prim(
            cuboid_tapered([0.78, 1.5, 0.24], 0.0, glass(ARCANE_GLASS, 1.9)),
            [0.0, y, zf],
            id_quat(),
        ));
        prims.extend(pointed_arch(
            [0.0, y + 0.78, zf - 0.02],
            0.42,
            0.1,
            stone(STONE_GREY),
        ));
    }

    // Arched timber door at the base on the −Z front.
    let door_z = -(rad(base_h + 1.0) - 0.05);
    prims.push(prim(
        solid(cuboid_tapered([1.1, 2.0, 0.24], 0.0, timber(TIMBER_DARK))),
        [0.0, base_h + 1.0, door_z],
        id_quat(),
    ));
    prims.extend(pointed_arch(
        [0.0, base_h + 2.0, door_z - 0.02],
        0.58,
        0.12,
        stone(STONE_GREY),
    ));

    // Corbelled balcony gallery ringing the shaft below the cap — the read
    // that turns a lighthouse into a mage's tower.
    let bal_y = shaft_top - 0.4;
    let bal_r = rad(bal_y) + 0.55;
    prims.push(prim(
        solid(torus(0.22, bal_r, stone(STONE_GREY))),
        [0.0, bal_y, 0.0],
        id_quat(),
    ));
    // A railing of short merlon posts around the walkway.
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU;
        prims.push(prim(
            solid(cuboid_tapered([0.18, 0.42, 0.18], 0.0, stone(STONE_GREY))),
            [a.cos() * bal_r, bal_y + 0.3, a.sin() * bal_r],
            id_quat(),
        ));
    }

    // Steep witch-hat slate cone cap rising from the gallery.
    prims.push(prim(
        solid(cone(rad(shaft_top) + 0.2, 4.6, 16, stone(STONE_GREY))),
        [0.0, shaft_top + 2.3, 0.0],
        id_quat(),
    ));
    // Gold spire + glowing crystal orb finial — emissive.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.0, 6, 0.4, gold(GOLD))),
        [0.0, shaft_top + 5.0, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.42, 6, glow(ARCANE_PURPLE, 2.6)),
        [0.0, shaft_top + 5.7, 0.0],
        id_quat(),
    ));
    // A halo of little crystal points around the orb.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        prims.push(crystal(
            [a.cos() * 0.5, shaft_top + 5.5, a.sin() * 0.5],
            0.06,
            0.5,
            id_quat(),
            glow(CRYSTAL_CYAN, 1.7),
        ));
    }

    // Floating runestones orbiting the cap base — dark slate slabs with glowing
    // gold rune strokes facing out toward the −Z hero front.
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU - 0.5;
        let (rx, rz) = (a.cos() * 3.1, a.sin() * 3.1);
        prims.push(prim(
            solid(cuboid_tapered([0.7, 1.1, 0.2], 0.12, stone(SLATE))),
            [rx, shaft_top + 0.8, rz],
            id_quat(),
        ));
        prims.extend(rune_marks(
            [rx, shaft_top + 0.8, rz - 0.12],
            0.66,
            glow(RUNE_GOLD, 2.2),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: an arcane hum, sparkles whirling around the orb.
    root.audio = fx::arcane_hum();
    root.children
        .push(fx::arcane_sparkle([0.0, shaft_top + 5.7, 0.0], 0x0A5C_0B12));
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
        assert!(crate::catalogue::items::util::has_emissive(
            &WizardTower.build("")
        ));
    }
}
