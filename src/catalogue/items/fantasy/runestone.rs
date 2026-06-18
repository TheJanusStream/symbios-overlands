//! Runestone — a High-Fantasy prop. A carved stone slab hovering above a
//! glowing rune mark, its glyphs alight. Scatter clutter of the arcane
//! quarter; its glow is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_PURPLE, RUNE_GOLD, STONE_GREY, stone};

pub struct Runestone;

impl CatalogueEntry for Runestone {
    fn slug(&self) -> &'static str {
        "runestone"
    }
    fn name(&self) -> &'static str {
        "Runestone"
    }
    fn description(&self) -> &'static str {
        "Carved stone slab hovering above a glowing rune mark, its glyphs alight."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Glowing rune mark on the ground — the root.
        prim(
            cylinder_tapered(0.8, 0.06, 16, 0.0, glow(ARCANE_PURPLE, 2.0)),
            [0.0, 0.04, 0.0],
            id_quat(),
        ),
    ];

    // Floating stone slab above the mark.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.5, 0.3], 0.0, stone(STONE_GREY))),
        [0.0, 1.5, 0.0],
        id_quat(),
    ));
    // Glowing glyphs carved into the face — emissive.
    prims.push(prim(
        cuboid_tapered([0.4, 0.9, 0.34], 0.0, glow(RUNE_GOLD, 3.0)),
        [0.0, 1.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Runestone.build(""), "runestone");
    }
}
