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

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::gothic_horror::pointed_arch;
use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, plane, prim,
    quat_x, solid, sphere, torus, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ARCANE_GLASS, ARCANE_PURPLE, CRYSTAL_CYAN, GOLD, RUNE_GOLD, STONE_GREY, TIMBER_DARK, crystal,
    fx, gold, rune_marks, stone, timber,
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
    // Gold string-course bands hugging the shaft, set high in the gaps between
    // the stacked windows — clear of each window's pointed-arch top below and
    // the sill above — so they never cut across a pane or its arch.
    for y in [base_h + 4.5, base_h + 7.5] {
        prims.push(prim(
            solid(torus(0.14, rad(y) + 0.04, gold(GOLD))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Arcane-lit windows climbing the −Z (front) face, each under a pointed
    // arch. The shaft is solid stone, so behind every opening sits a small
    // emissive arcane chamber set just proud of the curved wall — the cut
    // window panes (opacity below the alpha-mask cutoff) reveal that glow, not
    // the stone, instead of the old `Window`-textured slab that tiled since
    // #936 and read as a tinted block (#948). A stone sill finishes each.
    for y in [base_h + 2.6, base_h + 5.6, base_h + 8.6] {
        let zf = -rad(y); // shaft surface at this height (front = −Z)
        prims.push(prim(
            solid(cuboid_tapered(
                [0.86, 1.6, 0.1],
                0.0,
                glow(ARCANE_GLASS, 2.2),
            )),
            [0.0, y, zf - 0.02],
            id_quat(),
        ));
        prims.push(prim(
            plane([0.78, 1.5], window_card(ARCANE_GLASS, 2, 4, 0.35, 0.06)),
            [0.0, y, zf - 0.09],
            quat_x(-FRAC_PI_2),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.94, 0.12, 0.18], 0.0, stone(STONE_GREY))),
            [0.0, y - 0.82, zf - 0.05],
            id_quat(),
        ));
        prims.extend(pointed_arch(
            [0.0, y + 0.78, zf - 0.05],
            0.42,
            0.1,
            stone(STONE_GREY),
        ));
    }

    // Timber door at the base on the −Z front.
    let door_z = -(rad(base_h + 1.0) - 0.05);
    prims.push(prim(
        solid(cuboid_tapered([1.1, 2.0, 0.24], 0.0, timber(TIMBER_DARK))),
        [0.0, base_h + 1.0, door_z],
        id_quat(),
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
    // Short gold neck + glowing crystal orb finial — emissive. The orb is
    // seated down onto the cone tip (its underside dips below the apex) and the
    // neck is short, so the orb nestles on the point instead of perching high
    // on a thin needle.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 0.7, 6, 0.4, gold(GOLD))),
        [0.0, shaft_top + 4.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.42, 6, glow(ARCANE_PURPLE, 2.6)),
        [0.0, shaft_top + 4.9, 0.0],
        id_quat(),
    ));
    // A halo of little crystal points around the orb.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        prims.push(crystal(
            [a.cos() * 0.5, shaft_top + 4.7, a.sin() * 0.5],
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
        .push(fx::arcane_sparkle([0.0, shaft_top + 5.2, 0.0], 0x0A5C_0B12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

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

    /// #948: every `Window` card sits on a `Plane` at `uv_scale` 1.0 (spans
    /// once, not tiled), and — being a landmark that gets embedded in room
    /// records — the built tree survives a serde round-trip.
    #[test]
    fn glazing_is_planes_and_round_trips() {
        use crate::pds::material_finish::node_materials_mut;
        fn walk(g: &mut Generator) {
            let tag = g.kind.kind_tag();
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in node_materials_mut(&mut g.kind) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane, found {tag}");
                    assert_eq!(m.uv_scale.0, 1.0, "Window cards must stay at uv_scale 1.0");
                }
            }
            for c in &mut g.children {
                walk(c);
            }
        }
        let mut g = WizardTower.build("");
        walk(&mut g);
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "wizard tower must survive a serde round-trip"
        );
    }
}
