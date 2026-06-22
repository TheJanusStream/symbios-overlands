//! Light disc — an Alien-Monolithic prop. A flush obsidian-ringed disc glowing
//! with concentric light — a transit pad set into the ground. Scatter clutter
//! of the site; the disc is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_y, solid, sphere, torus,
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
    let mut prims = vec![
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
        // Two concentric glowing rings — emissive, proud of the disc.
        prim(
            torus(0.045, 0.92, glow(GLYPH_CYAN, 2.6)),
            [0.0, 0.24, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.04, 0.5, glow(GLYPH_CYAN, 2.6)),
            [0.0, 0.24, 0.0],
            id_quat(),
        ),
        // Glowing centre node — the transit focus.
        prim(
            sphere(0.16, 6, glow(GLYPH_CYAN, 2.8)),
            [0.0, 0.27, 0.0],
            id_quat(),
        ),
    ];
    // Radial glyph ticks spoking out between the rings — a transit pad's
    // bearing marks.
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        prims.push(prim(
            cuboid_tapered([0.08, 0.05, 0.26], 0.0, glow(GLYPH_CYAN, 2.4)),
            [a.cos() * 0.71, 0.24, a.sin() * 0.71],
            quat_y(-a),
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
        assert_sanitize_stable(&LightDisc.build(""), "light_disc");
    }
}
