//! Calendar stone — a Mesoamerican prop. A great round sun-stone stood
//! upright on a plinth: concentric carved rings of glyphs around a beaten-
//! gold sun face at the centre. The reckoning of the ages in stone.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GOLD_WARM, LIMESTONE_PALE, STONE_GREY, cobble, gold, limestone};

pub struct CalendarStone;

impl CatalogueEntry for CalendarStone {
    fn slug(&self) -> &'static str {
        "calendar_stone"
    }
    fn name(&self) -> &'static str {
        "Calendar Stone"
    }
    fn description(&self) -> &'static str {
        "Upright carved sun-stone with concentric glyph rings and a gold centre."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let disc_y = 1.9_f32;

    let mut prims = vec![
        // Plinth — the root.
        prim(
            solid(cuboid_tapered([1.8, 0.5, 1.1], 0.0, limestone(STONE_GREY))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
        // Upright sun-stone disc, face toward +Z.
        prim(
            solid(cylinder_tapered(
                1.3,
                0.35,
                28,
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, disc_y, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Concentric carved glyph rings on the face.
    for r in [1.0_f32, 0.65] {
        prims.push(prim(
            torus(0.06, r, cobble(STONE_GREY)),
            [0.0, disc_y, 0.2],
            quat_x(FRAC_PI_2),
        ));
    }
    // Beaten-gold sun face at the centre.
    prims.push(prim(
        solid(sphere(0.32, 3, gold(GOLD_WARM))),
        [0.0, disc_y, 0.28],
        id_quat(),
    ));
    // Four cardinal glyph blocks around the rim.
    for (dx, dy) in [(0.0_f32, 1.1_f32), (1.1, 0.0), (0.0, -1.1), (-1.1, 0.0)] {
        prims.push(prim(
            cuboid_tapered([0.22, 0.22, 0.12], 0.0, cobble(STONE_GREY)),
            [dx, disc_y + dy, 0.22],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CalendarStone.build(""), "calendar_stone");
    }
}
