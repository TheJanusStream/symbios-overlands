//! Enchanted library — a High-Fantasy secondary. A domed stone hall with tall
//! arcane-lit windows, gold trim and a few grimoires drifting glowing above
//! the door. The repository of spells; its windows and floating books are
//! emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_GLASS, ARCANE_PURPLE, GOLD, STONE_GREY, glass, gold, stone};

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
    // Tall lit windows across the front — emissive.
    prims.push(prim(
        cuboid_tapered([8.5, 3.0, 0.2], 0.0, glass(ARCANE_GLASS, 1.4)),
        [0.0, base_h + 2.6, 3.05],
        id_quat(),
    ));
    // Gold cornice.
    prims.push(prim(
        solid(cuboid_tapered([10.4, 0.4, 6.9], 0.0, gold(GOLD))),
        [0.0, body_top, -0.2],
        id_quat(),
    ));

    // Stone dome + gold finial.
    prims.push(prim(
        solid(sphere(3.4, 3, stone(STONE_GREY))),
        [0.0, body_top, -0.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.4, 1.0, 0.4], 0.6, gold(GOLD))),
        [0.0, body_top + 3.6, -0.2],
        id_quat(),
    ));

    // Bronze door.
    prims.push(prim(
        solid(cuboid_tapered([1.8, 2.6, 0.3], 0.0, gold(GOLD))),
        [0.0, base_h + 1.3, 3.1],
        id_quat(),
    ));
    // Grimoires drifting glowing above the door — emissive.
    for (dx, dy) in [(-1.2_f32, 4.2), (0.4, 4.6), (1.1, 4.0)] {
        prims.push(prim(
            cuboid_tapered([0.5, 0.18, 0.4], 0.0, glow(ARCANE_PURPLE, 2.0)),
            [dx, dy, 3.2],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&EnchantedLibrary.build(""), "enchanted_library");
    }
}
