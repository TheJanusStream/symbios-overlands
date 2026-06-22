//! Mana font — a High-Fantasy prop. A small stone basin brimming with glowing
//! mana around a crystal-tipped spout. Scatter clutter of the arcane quarter;
//! the pool and tip are emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CRYSTAL_CYAN, MANA_TEAL, RUNE_GOLD, STONE_GREY, crystal, rune_marks, stone};

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
    let mut prims = vec![
        // Fluted pedestal foot — the root.
        prim(
            solid(cylinder_tapered(0.7, 0.4, 12, 0.45, stone(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Stone basin drum.
        prim(
            solid(cylinder_tapered(1.0, 0.55, 16, 0.06, stone(STONE_GREY))),
            [0.0, 0.62, 0.0],
            id_quat(),
        ),
        // Stone rim.
        prim(
            solid(torus(0.1, 0.97, stone(STONE_GREY))),
            [0.0, 0.88, 0.0],
            id_quat(),
        ),
        // Glowing mana pool — emissive.
        prim(
            cylinder_tapered(0.9, 0.1, 16, 0.0, glow(MANA_TEAL, 1.8)),
            [0.0, 0.86, 0.0],
            id_quat(),
        ),
        // Central spout column.
        prim(
            solid(cylinder_tapered(0.17, 0.6, 8, 0.12, stone(STONE_GREY))),
            [0.0, 1.18, 0.0],
            id_quat(),
        ),
    ];

    // Faceted crystal tip on the spout — emissive.
    prims.push(crystal(
        [0.0, 1.45, 0.0],
        0.16,
        0.8,
        id_quat(),
        glow(CRYSTAL_CYAN, 1.8),
    ));
    // Lesser crystals rising from the pool around the spout.
    for (cx, cz, tilt) in [
        (0.45_f32, 0.18_f32, 0.5_f32),
        (-0.4, 0.26, -0.55),
        (0.12, -0.46, 0.45),
    ] {
        prims.push(crystal(
            [cx, 0.9, cz],
            0.08,
            0.5,
            quat_z(tilt),
            glow(CRYSTAL_CYAN, 1.6),
        ));
    }

    // A glowing rune carved into the basin's −Z front face.
    prims.extend(rune_marks([0.0, 0.6, -1.0], 0.34, glow(RUNE_GOLD, 1.8)));

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
