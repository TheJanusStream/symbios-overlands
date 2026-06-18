//! Mana font — a High-Fantasy prop. A small stone basin brimming with glowing
//! mana around a crystal-tipped spout. Scatter clutter of the arcane quarter;
//! the pool and tip are emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CRYSTAL_CYAN, MANA_TEAL, STONE_GREY, stone};

pub struct ManaFont;

impl CatalogueEntry for ManaFont {
    fn slug(&self) -> &'static str {
        "mana_font"
    }
    fn name(&self) -> &'static str {
        "Mana Font"
    }
    fn description(&self) -> &'static str {
        "Stone basin brimming with glowing mana around a crystal-tipped spout."
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
        // Stone basin drum — the root.
        prim(
            solid(cylinder_tapered(1.0, 0.6, 16, 0.05, stone(STONE_GREY))),
            [0.0, 0.3, 0.0],
            id_quat(),
        ),
        // Stone rim.
        prim(
            solid(torus(0.1, 0.95, stone(STONE_GREY))),
            [0.0, 0.6, 0.0],
            id_quat(),
        ),
        // Glowing mana pool — emissive.
        prim(
            cylinder_tapered(0.88, 0.1, 16, 0.0, glow(MANA_TEAL, 2.5)),
            [0.0, 0.58, 0.0],
            id_quat(),
        ),
        // Central spout column.
        prim(
            solid(cylinder_tapered(0.16, 0.7, 8, 0.1, stone(STONE_GREY))),
            [0.0, 0.95, 0.0],
            id_quat(),
        ),
        // Glowing crystal tip — emissive.
        prim(
            sphere(0.22, 3, glow(CRYSTAL_CYAN, 3.0)),
            [0.0, 1.4, 0.0],
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
        assert_sanitize_stable(&ManaFont.build(""), "mana_font");
    }
}
