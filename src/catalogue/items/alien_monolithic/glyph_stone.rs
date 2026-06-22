//! Glyph stone — an Alien-Monolithic prop. A short obsidian standing stone
//! inscribed with a glowing glyph. Scatter clutter marking the site; the glyph
//! is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLYPH_CYAN, OBSIDIAN, glyph_column, obsidian};

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
    let mut prims = vec![
        // Obsidian standing stone — the root, its top tapered to a leaning
        // wedge so it reads as a hewn standing stone, not a plain plinth.
        prim(
            solid(cuboid_tapered([0.9, 1.8, 0.4], 0.14, obsidian(OBSIDIAN))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
    ];
    // Inscribed glyphs down the −Z hero face — asymmetric alien script
    // standing proud of the dark stone, not the old blank light panel.
    for g in glyph_column(0.0, 0.55, 1.35, -0.24, &[0.7, 0.55], glow(GLYPH_CYAN, 2.4)) {
        prims.push(g);
    }

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
