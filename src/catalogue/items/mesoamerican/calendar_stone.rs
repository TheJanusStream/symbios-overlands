//! Calendar stone — a Mesoamerican prop. A great round sun-stone stood
//! upright on a plinth: concentric carved rings of glyphs around a beaten-
//! gold sun face at the centre. The reckoning of the ages in stone.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    GOLD_WARM, JADE_GREEN, LIMESTONE_PALE, OBSIDIAN_BLACK, STONE_GREY, cobble, gold, jade,
    limestone, obsidian, painted,
};

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
    let r_disc = 1.35_f32;
    let zf = -0.22_f32; // front carving plane (carved face = −Z)

    let mut prims = vec![
        // Plinth — the root.
        prim(
            solid(cuboid_tapered([1.9, 0.5, 1.2], 0.0, limestone(STONE_GREY))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
        // Upright sun-stone disc, carved face toward the front (−Z).
        prim(
            solid(cylinder_tapered(
                r_disc,
                0.4,
                28,
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, disc_y, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Raised outer border ring and two inner concentric ring bands.
    prims.push(prim(
        torus(0.1, r_disc * 0.95, cobble(STONE_GREY)),
        [0.0, disc_y, zf],
        quat_x(FRAC_PI_2),
    ));
    for rr in [0.9_f32, 0.58] {
        prims.push(prim(
            torus(0.07, rr, cobble(STONE_GREY)),
            [0.0, disc_y, zf],
            quat_x(FRAC_PI_2),
        ));
    }

    // A ring of day-sign glyph blocks between the two inner bands.
    let glyphs = 8;
    for i in 0..glyphs {
        let a = i as f32 / glyphs as f32 * TAU;
        prims.push(prim(
            cuboid_tapered([0.17, 0.17, 0.1], 0.0, cobble(STONE_GREY)),
            [a.cos() * 0.74, disc_y + a.sin() * 0.74, zf],
            id_quat(),
        ));
    }

    // Four raised era-cartouches at the diagonals — the four previous suns.
    for i in 0..4 {
        let a = (i as f32 + 0.5) / 4.0 * TAU;
        prims.push(prim(
            cuboid_tapered([0.27, 0.27, 0.12], 0.05, cobble(STONE_GREY)),
            [a.cos() * 0.46, disc_y + a.sin() * 0.46, zf - 0.02],
            id_quat(),
        ));
    }

    // Pointed sun rays radiating at the eight directions, just inside the rim.
    let rays = 8;
    for i in 0..rays {
        let a = i as f32 / rays as f32 * TAU;
        prims.push(prim(
            solid(cone(0.13, 0.32, 6, cobble(STONE_GREY))),
            [
                a.cos() * (r_disc - 0.32),
                disc_y + a.sin() * (r_disc - 0.32),
                zf + 0.02,
            ],
            quat_z(a - FRAC_PI_2),
        ));
    }

    // Central beaten-gold sun face (Tonatiuh): a gold disc with jade eyes, a
    // dark mouth, and a protruding obsidian sacrificial tongue.
    prims.push(prim(
        solid(cylinder_tapered(0.34, 0.16, 18, 0.0, gold(GOLD_WARM))),
        [0.0, disc_y, zf - 0.08],
        quat_x(FRAC_PI_2),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.08, 0.1, 0.08], 0.0, jade(JADE_GREEN)),
            [sx * 0.13, disc_y + 0.06, zf - 0.16],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([0.2, 0.07, 0.06], 0.0, painted([0.1, 0.07, 0.06])),
        [0.0, disc_y - 0.05, zf - 0.16],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.07, 0.17, 0.06], 0.0, obsidian(OBSIDIAN_BLACK)),
        [0.0, disc_y - 0.17, zf - 0.14],
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
        assert_sanitize_stable(&CalendarStone.build(""), "calendar_stone");
    }
}
