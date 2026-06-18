//! Glyph arch — an Alien-Monolithic secondary. A black obsidian gateway, its
//! jambs and lintel carved with glowing glyphs across a shimmering threshold.
//! Its glow is emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the left jamb.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLYPH_VIOLET, OBSIDIAN, obsidian};

pub struct GlyphArch;

impl CatalogueEntry for GlyphArch {
    fn slug(&self) -> &'static str {
        "glyph_arch"
    }
    fn name(&self) -> &'static str {
        "Glyph Arch"
    }
    fn description(&self) -> &'static str {
        "Black obsidian gateway, jambs and lintel carved with glowing glyphs over a threshold."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MONOLITH_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let leg_h = 5.0_f32;

    let mut prims = vec![
        // Left jamb — the root.
        prim(
            solid(cuboid_tapered([0.9, leg_h, 0.9], 0.06, obsidian(OBSIDIAN))),
            [-2.2, leg_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Right jamb.
    prims.push(prim(
        solid(cuboid_tapered([0.9, leg_h, 0.9], 0.06, obsidian(OBSIDIAN))),
        [2.2, leg_h * 0.5, 0.0],
        id_quat(),
    ));
    // Lintel.
    prims.push(prim(
        solid(cuboid_tapered([5.3, 1.0, 1.0], 0.0, obsidian(OBSIDIAN))),
        [0.0, leg_h + 0.3, 0.0],
        id_quat(),
    ));

    // Glowing glyphs down the jambs and across the lintel — emissive.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.16, leg_h - 1.0, 0.92], 0.0, glow(GLYPH_VIOLET, 2.6)),
            [sx * 2.2, leg_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([4.4, 0.18, 1.02], 0.0, glow(GLYPH_VIOLET, 2.6)),
        [0.0, leg_h + 0.3, 0.0],
        id_quat(),
    ));
    // Shimmering threshold field in the opening — emissive.
    prims.push(prim(
        cuboid_tapered([3.3, leg_h - 0.4, 0.1], 0.0, glow(GLYPH_VIOLET, 1.4)),
        [0.0, (leg_h - 0.4) * 0.5, 0.0],
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
        assert_sanitize_stable(&GlyphArch.build(""), "glyph_arch");
    }
}
