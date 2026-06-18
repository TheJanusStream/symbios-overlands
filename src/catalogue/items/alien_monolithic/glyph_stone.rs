//! Glyph stone — an Alien-Monolithic prop. A short obsidian standing stone
//! inscribed with a glowing glyph. Scatter clutter marking the site; the glyph
//! is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLYPH_CYAN, OBSIDIAN, obsidian};

pub struct GlyphStone;

impl CatalogueEntry for GlyphStone {
    fn slug(&self) -> &'static str {
        "glyph_stone"
    }
    fn name(&self) -> &'static str {
        "Glyph Stone"
    }
    fn description(&self) -> &'static str {
        "Short obsidian standing stone inscribed with a glowing glyph."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MONOLITH_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Obsidian standing stone — the root.
        prim(
            solid(cuboid_tapered([0.9, 1.8, 0.4], 0.08, obsidian(OBSIDIAN))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
        // Glowing glyph on the face — emissive.
        prim(
            cuboid_tapered([0.4, 1.0, 0.44], 0.0, glow(GLYPH_CYAN, 2.6)),
            [0.0, 0.95, 0.0],
            id_quat(),
        ),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GlyphStone.build(""), "glyph_stone");
    }
}
