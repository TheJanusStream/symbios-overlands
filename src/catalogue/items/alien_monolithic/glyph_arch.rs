//! Glyph arch — an Alien-Monolithic secondary. A black obsidian gateway, its
//! jambs and lintel carved with glowing glyphs across a shimmering threshold.
//! Its glow is emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the left jamb.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLYPH_VIOLET, OBSIDIAN, glyph_column, obsidian};

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
    let jamb_x = 2.2_f32; // jamb centre
    let half_span = 1.75_f32; // arch radius = inner jamb face
    let zf = -(0.45 + 0.04); // proud of the −Z hero front

    let mut prims = vec![
        // Left jamb — the root.
        prim(
            solid(cuboid_tapered([0.9, leg_h, 0.9], 0.06, obsidian(OBSIDIAN))),
            [-jamb_x, leg_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Right jamb.
    prims.push(prim(
        solid(cuboid_tapered([0.9, leg_h, 0.9], 0.06, obsidian(OBSIDIAN))),
        [jamb_x, leg_h * 0.5, 0.0],
        id_quat(),
    ));
    // Semicircular obsidian arch springing from the jamb tops — a real arched
    // gateway, not the old flat post-and-lintel rectangle.
    prims.push(prim(
        solid(with_cut(
            torus(0.45, half_span, obsidian(OBSIDIAN)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, leg_h, 0.0],
        quat_x(-std::f32::consts::FRAC_PI_2),
    ));

    // Inscribed glyph columns down the −Z face of each jamb — emissive.
    for sx in [-1.0_f32, 1.0] {
        for g in glyph_column(
            sx * jamb_x,
            1.0,
            leg_h - 0.8,
            zf,
            &[0.9, 0.7, 1.0],
            glow(GLYPH_VIOLET, 2.0),
        ) {
            prims.push(g);
        }
    }
    // Glowing arch seam — a thin luminous semicircle just proud of the −Z
    // front, tracing the gateway's threshold ring.
    prims.push(prim(
        with_cut(
            torus(0.07, half_span, glow(GLYPH_VIOLET, 2.2)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, leg_h, zf],
        quat_x(-std::f32::consts::FRAC_PI_2),
    ));
    // Keystone glyph at the apex front — emissive.
    for g in glyph_column(
        0.0,
        leg_h + half_span - 0.7,
        leg_h + half_span - 0.7,
        zf - 0.04,
        &[0.8],
        glow(GLYPH_VIOLET, 2.2),
    ) {
        prims.push(g);
    }
    // Shimmering threshold field in the opening — emissive, deep violet at a
    // low strength so it reads as charged energy, not a washed-out lavender
    // panel (a broad flat emissive face blooms pale if driven hard).
    prims.push(prim(
        cuboid_tapered([3.3, leg_h - 0.4, 0.08], 0.0, glow(GLYPH_VIOLET, 1.0)),
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
