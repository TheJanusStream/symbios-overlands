//! Crystal shrine — a High-Fantasy secondary. An open stone shrine of four
//! pillars sheltering a great glowing crystal cluster on a gold-ringed plinth,
//! singing softly. Its crystal is emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the plinth.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CRYSTAL_CYAN, GOLD, RUNE_GOLD, STONE_GREY, crystal, fx, gold, rune_marks, stone};

pub struct CrystalShrine;

impl CatalogueEntry for CrystalShrine {
    fn slug(&self) -> &'static str {
        "crystal_shrine"
    }
    fn name(&self) -> &'static str {
        "Crystal Shrine"
    }
    fn description(&self) -> &'static str {
        "Open stone shrine sheltering a glowing crystal cluster on a gold-ringed plinth."
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
            clearance: 5.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let step_h = 0.35_f32;
    let plinth_h = 0.8_f32;
    let plinth_top = step_h + plinth_h;
    let pillar_h = 3.4_f32;
    let cap_y = plinth_top + pillar_h; // pillar top / cornice underside

    let mut prims = vec![
        // Broad mossy step — the root.
        prim(
            solid(cuboid_tapered([4.8, step_h, 4.8], 0.0, stone(STONE_GREY))),
            [0.0, step_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Stone plinth on the step.
    prims.push(prim(
        solid(cuboid_tapered([3.8, plinth_h, 3.8], 0.0, stone(STONE_GREY))),
        [0.0, step_h + plinth_h * 0.5, 0.0],
        id_quat(),
    ));

    // Four corner pillars — base, slender shaft, capital.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            let (px, pz) = (sx * 1.55, sz * 1.55);
            // Base plinth.
            prims.push(prim(
                solid(cuboid_tapered([0.62, 0.28, 0.62], 0.1, stone(STONE_GREY))),
                [px, plinth_top + 0.14, pz],
                id_quat(),
            ));
            // Shaft.
            prims.push(prim(
                solid(cylinder_tapered(
                    0.24,
                    pillar_h - 0.5,
                    12,
                    0.12,
                    stone(STONE_GREY),
                )),
                [px, plinth_top + 0.28 + (pillar_h - 0.5) * 0.5, pz],
                id_quat(),
            ));
            // Capital.
            prims.push(prim(
                solid(cuboid_tapered([0.56, 0.26, 0.56], 0.0, stone(STONE_GREY))),
                [px, cap_y - 0.05, pz],
                id_quat(),
            ));
        }
    }

    // Cornice ring beam carrying the roof.
    prims.push(prim(
        solid(cuboid_tapered([4.0, 0.3, 4.0], 0.0, stone(STONE_GREY))),
        [0.0, cap_y + 0.15, 0.0],
        id_quat(),
    ));
    // Peaked pyramid roof + gold finial spike.
    prims.push(prim(
        solid(cuboid_tapered([4.0, 1.7, 4.0], 0.99, stone(STONE_GREY))),
        [0.0, cap_y + 0.3 + 0.85, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.1, 0.7, 6, 0.4, gold(GOLD))),
        [0.0, cap_y + 2.3, 0.0],
        id_quat(),
    ));
    // Crystal finial crowning the spire.
    prims.push(crystal(
        [0.0, cap_y + 2.6, 0.0],
        0.16,
        0.8,
        id_quat(),
        glow(CRYSTAL_CYAN, 1.8),
    ));

    // Gold ring around the crystal base.
    prims.push(prim(
        solid(torus(0.12, 0.95, gold(GOLD))),
        [0.0, plinth_top + 0.12, 0.0],
        id_quat(),
    ));

    // Glowing faceted crystal cluster — a tall central shard flanked by lesser
    // ones leaning out at wild angles.
    prims.push(crystal(
        [0.0, plinth_top, 0.0],
        0.46,
        2.9,
        id_quat(),
        glow(CRYSTAL_CYAN, 1.8),
    ));
    for (cx, cz, h, tilt) in [
        (0.62_f32, 0.18_f32, 1.7_f32, 0.22_f32),
        (-0.5, 0.42, 1.4, -0.26),
        (0.12, -0.62, 1.2, 0.2),
    ] {
        prims.push(crystal(
            [cx, plinth_top, cz],
            0.24,
            h,
            crate::catalogue::items::util::quat_x(tilt),
            glow(CRYSTAL_CYAN, 1.6),
        ));
    }

    // Glowing rune band carved into the plinth's front (−Z) face.
    let runes = rune_marks(
        [0.0, step_h + plinth_h * 0.55, -1.95],
        0.42,
        glow(RUNE_GOLD, 1.7),
    );
    prims.extend(runes);

    let mut root = assemble(prims);
    // Signature life: the crystal's shimmer and rising mana motes.
    root.audio = fx::crystal_shimmer();
    root.children
        .push(fx::mana_motes([0.0, plinth_top + 1.5, 0.0], 0x0A1A_C512));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CrystalShrine.build(""), "crystal_shrine");
    }
}
