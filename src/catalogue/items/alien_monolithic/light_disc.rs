//! Light disc — an Alien-Monolithic prop. A flush obsidian-ringed disc glowing
//! with concentric light — a transit pad set into the ground. Scatter clutter
//! of the site; the disc is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ENERGY_BLUE, GLYPH_CYAN, OBSIDIAN, obsidian};

pub struct LightDisc;

impl CatalogueEntry for LightDisc {
    fn slug(&self) -> &'static str {
        "light_disc"
    }
    fn name(&self) -> &'static str {
        "Light Disc"
    }
    fn description(&self) -> &'static str {
        "Flush obsidian-ringed disc glowing with concentric light — a transit pad."
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
            clearance: 1.2,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Obsidian rim disc — the root.
        prim(
            solid(cylinder_tapered(1.3, 0.18, 24, 0.0, obsidian(OBSIDIAN))),
            [0.0, 0.09, 0.0],
            id_quat(),
        ),
        // Glowing inner disc — emissive.
        prim(
            cylinder_tapered(1.0, 0.06, 24, 0.0, glow(ENERGY_BLUE, 2.2)),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Concentric glowing ring — emissive.
        prim(
            torus(0.04, 0.6, glow(GLYPH_CYAN, 2.6)),
            [0.0, 0.23, 0.0],
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
        assert_sanitize_stable(&LightDisc.build(""), "light_disc");
    }
}
