//! Runestone — a High-Fantasy prop. A carved stone slab hovering above a
//! glowing rune mark, its glyphs alight. Scatter clutter of the arcane
//! quarter; its glow is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_PURPLE, RUNE_GOLD, STONE_MOSS, mossy, rune_marks, stone};

/// Dark slate of the floating slab — a cold backing the gold glyphs read on.
const SLATE: [f32; 3] = [0.33, 0.32, 0.37];

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
        // Mossy stone groundplate — the root.
        prim(
            solid(cylinder_tapered(0.85, 0.1, 16, 0.15, mossy(STONE_MOSS))),
            [0.0, 0.05, 0.0],
            id_quat(),
        ),
    ];
    // Glowing sigil ring inscribed on the plate (a ring, not a flat puddle).
    prims.push(prim(
        torus(0.05, 0.62, glow(ARCANE_PURPLE, 1.8)),
        [0.0, 0.12, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.04, 0.34, glow(ARCANE_PURPLE, 1.8)),
        [0.0, 0.12, 0.0],
        id_quat(),
    ));

    // Floating dark-slate slab above the mark, its top tapered to a crown.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 1.6, 0.26], 0.18, stone(SLATE))),
        [0.0, 1.55, 0.0],
        id_quat(),
    ));
    // Glowing rune strokes carved proud of the slab's −Z (front) face.
    prims.extend(rune_marks([0.0, 1.55, -0.16], 0.95, glow(RUNE_GOLD, 1.9)));

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
