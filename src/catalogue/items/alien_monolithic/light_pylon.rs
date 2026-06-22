//! Light pylon — an Alien-Monolithic secondary. A tall tapering obsidian pylon
//! banded with glyphs and crowned by a glowing orb and a shaft of light. The
//! beacon of the site; its orb and beam are emissive trim the ruin pass can
//! darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLYPH_CYAN, OBSIDIAN, fx, glyph_column, obsidian};

pub struct LightPylon;

impl CatalogueEntry for LightPylon {
    fn slug(&self) -> &'static str {
        "light_pylon"
    }
    fn name(&self) -> &'static str {
        "Light Pylon"
    }
    fn description(&self) -> &'static str {
        "Tall tapering obsidian pylon banded with glyphs, crowned by a glowing orb and beam."
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
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.5_f32;
    let pylon_h = 11.0_f32;
    let pylon_top = base_h + pylon_h;

    let mut prims = vec![
        // Obsidian base — the root.
        prim(
            solid(cuboid_tapered([1.8, base_h, 1.8], 0.0, obsidian(OBSIDIAN))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Tapering pylon shaft.
    prims.push(prim(
        solid(cuboid_tapered([0.9, pylon_h, 0.9], 0.5, obsidian(OBSIDIAN))),
        [0.0, base_h + pylon_h * 0.5, 0.0],
        id_quat(),
    ));
    // Glowing energy collars ringing the shaft — emissive bands that read on
    // the needle far better than the old tiny face-bars; majors shrink with
    // the shaft's taper.
    for (k, major) in [0.56_f32, 0.46, 0.37].into_iter().enumerate() {
        let y = base_h + 2.5 + k as f32 * 3.0;
        prims.push(prim(
            torus(0.08, major, glow(GLYPH_CYAN, 2.4)),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }
    // A short inscribed glyph column up the −Z front of the lower shaft.
    for g in glyph_column(
        0.0,
        base_h + 1.4,
        base_h + 4.4,
        -0.52,
        &[0.6, 0.7, 0.55],
        glow(GLYPH_CYAN, 2.1),
    ) {
        prims.push(g);
    }

    // Glowing orb at the crown — emissive, rounder (res 6) than the old blocky
    // res-3 facet ball.
    prims.push(prim(
        sphere(0.5, 6, glow(GLYPH_CYAN, 3.0)),
        [0.0, pylon_top + 0.4, 0.0],
        id_quat(),
    ));
    // Horizontal halo ring around the orb — emissive.
    prims.push(prim(
        torus(0.05, 0.78, glow(GLYPH_CYAN, 2.3)),
        [0.0, pylon_top + 0.4, 0.0],
        id_quat(),
    ));
    // Thin shaft of light rising above — emissive.
    prims.push(prim(
        cylinder_tapered(0.1, 2.5, 6, 0.6, glow(GLYPH_CYAN, 2.6)),
        [0.0, pylon_top + 1.8, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a high power shimmer at the crown.
    root.audio = fx::power_shimmer();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&LightPylon.build(""), "light_pylon");
    }
}
