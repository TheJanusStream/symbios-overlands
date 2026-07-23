//! Enchanted library — a High-Fantasy secondary. A domed stone hall with tall
//! arcane-lit windows, gold trim and a few grimoires drifting glowing above
//! the door. The repository of spells; its windows and floating books are
//! emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::gothic_horror::pointed_arch;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, plane, prim, quat_x, solid, sphere,
    window_card, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_GLASS, ARCANE_PURPLE, CRYSTAL_CYAN, GOLD, STONE_GREY, crystal, gold, stone};

pub struct EnchantedLibrary;

impl CatalogueEntry for EnchantedLibrary {
    fn slug(&self) -> &'static str {
        "enchanted_library"
    }
    fn name(&self) -> &'static str {
        "Enchanted Library"
    }
    fn description(&self) -> &'static str {
        "Domed stone hall with arcane-lit windows and grimoires drifting above the door."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
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
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;
    let body_h = 5.0_f32;
    let body_top = base_h + body_h;
    let front = -3.45_f32; // body front (−Z) wall face

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([12.0, base_h, 8.0], 0.0, stone(STONE_GREY))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Stone body.
    prims.push(prim(
        solid(cuboid_tapered([10.0, body_h, 6.5], 0.0, stone(STONE_GREY))),
        [0.0, base_h + body_h * 0.5, -0.2],
        id_quat(),
    ));

    // Tall arcane-lit arched windows flanking the entrance on the −Z front.
    // The body is solid stone, so behind each opening sits an arcane light
    // chamber set just proud of the wall; the glazing is a cut window card on a
    // plane (opacity below the alpha-mask cutoff), so the panes reveal that
    // glow rather than tiling a `Window` texture over a slab (#950). Jamb ribs,
    // a sill and a pointed head frame it in stone.
    for x in [-3.5_f32, 3.5] {
        prims.push(prim(
            solid(cuboid_tapered(
                [1.5, 3.1, 0.12],
                0.0,
                glow(ARCANE_GLASS, 2.0),
            )),
            [x, base_h + 2.0, front - 0.04],
            id_quat(),
        ));
        prims.push(prim(
            plane([1.5, 3.0], window_card(ARCANE_GLASS, 3, 5, 0.35, 0.05)),
            [x, base_h + 2.0, front - 0.11],
            quat_x(-FRAC_PI_2),
        ));
        // Jamb ribs, sill and arched head.
        for s in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.16, 3.0, 0.34], 0.0, stone(STONE_GREY))),
                [x + s * 0.83, base_h + 2.0, front - 0.06],
                id_quat(),
            ));
        }
        prims.push(prim(
            solid(cuboid_tapered([1.8, 0.18, 0.24], 0.0, stone(STONE_GREY))),
            [x, base_h + 0.4, front - 0.06],
            id_quat(),
        ));
        prims.extend(pointed_arch(
            [x, base_h + 3.5, front - 0.06],
            0.75,
            0.14,
            stone(STONE_GREY),
        ));
    }

    // Columned entrance porch projecting from the front.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.32, 3.4, 12, 0.08, stone(STONE_GREY))),
            [s * 1.3, base_h + 1.7, front - 0.7],
            id_quat(),
        ));
    }
    // Porch entablature lintel.
    prims.push(prim(
        solid(cuboid_tapered([3.4, 0.5, 1.2], 0.0, gold(GOLD))),
        [0.0, base_h + 3.65, front - 0.4],
        id_quat(),
    ));
    // Bronze double door, recessed in the wall behind the porch.
    prims.push(prim(
        solid(cuboid_tapered([1.8, 2.8, 0.3], 0.0, gold(GOLD))),
        [0.0, base_h + 1.4, front + 0.05],
        id_quat(),
    ));
    prims.extend(pointed_arch(
        [0.0, base_h + 2.8, front + 0.04],
        0.9,
        0.16,
        stone(STONE_GREY),
    ));

    // Gold cornice.
    prims.push(prim(
        solid(cuboid_tapered([10.4, 0.45, 6.9], 0.0, gold(GOLD))),
        [0.0, body_top + 0.1, -0.2],
        id_quat(),
    ));

    // Domed stone drum + hemisphere dome + gold-and-crystal finial.
    prims.push(prim(
        solid(cylinder_tapered(3.3, 1.2, 24, 0.05, stone(STONE_GREY))),
        [0.0, body_top + 0.9, -0.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(with_cut(
            sphere(3.2, 6, stone(STONE_GREY)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, body_top + 1.5, -0.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.18, 1.0, 8, 0.5, gold(GOLD))),
        [0.0, body_top + 4.9, -0.2],
        id_quat(),
    ));
    prims.push(crystal(
        [0.0, body_top + 5.2, -0.2],
        0.16,
        0.9,
        id_quat(),
        glow(CRYSTAL_CYAN, 1.8),
    ));

    // Grimoires drifting glowing above the porch — emissive, on the −Z front.
    for (dx, dy) in [(-1.3_f32, 4.5), (0.3, 4.9), (1.2, 4.4)] {
        prims.push(prim(
            cuboid_tapered([0.5, 0.16, 0.38], 0.0, glow(ARCANE_PURPLE, 1.8)),
            [dx, dy, front - 1.0],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&EnchantedLibrary.build(""), "enchanted_library");
    }

    #[test]
    fn has_arcane_glow() {
        assert!(crate::catalogue::items::util::has_emissive(
            &EnchantedLibrary.build("")
        ));
    }

    /// #950: every `Window` card sits on a `Plane` at `uv_scale` 1.0 (spans
    /// once, not tiled), and the built tree survives a serde round-trip.
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
        let mut g = EnchantedLibrary.build("");
        walk(&mut g);
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "enchanted library must survive a serde round-trip"
        );
    }
}
