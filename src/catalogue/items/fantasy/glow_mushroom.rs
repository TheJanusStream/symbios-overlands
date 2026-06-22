//! Glow-mushroom — a High-Fantasy prop. A cluster of luminous toadstools, pale
//! stems under glowing caps. Scatter clutter lighting the arcane quarter; the
//! caps are emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{assemble, cylinder_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MUSH_GLOW, STONE_MOSS, matte, toadstool};

pub struct GlowMushroom;

impl CatalogueEntry for GlowMushroom {
    fn slug(&self) -> &'static str {
        "glow_mushroom"
    }
    fn name(&self) -> &'static str {
        "Glow-Mushroom"
    }
    fn description(&self) -> &'static str {
        "Cluster of luminous toadstools, pale stems under glowing caps."
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
            clearance: 0.8,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // A clump of luminous toadstools — domed bioluminescent caps on pale
    // stems, the tallest leading. The mossy clump is the assemble root so it
    // sits on the ground; the stools are rebased children rising from it.
    let prims = vec![
        // Mossy clump base — the root.
        prim(
            solid(cylinder_tapered(0.8, 0.1, 14, 0.3, matte(STONE_MOSS))),
            [0.0, 0.05, 0.0],
            id_quat(),
        ),
        toadstool([0.0, 0.08, 0.0], 1.3, glow(MUSH_GLOW, 1.4), false),
        toadstool([0.52, 0.08, 0.22], 0.85, glow(MUSH_GLOW, 1.4), false),
        toadstool([-0.44, 0.08, 0.3], 0.7, glow(MUSH_GLOW, 1.5), false),
        toadstool([0.22, 0.08, -0.42], 0.6, glow(MUSH_GLOW, 1.5), false),
    ];
    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GlowMushroom.build(""), "glow_mushroom");
    }
}
