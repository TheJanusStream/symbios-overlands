//! Levitating platform — an Alien-Monolithic secondary. A black obsidian slab
//! hovering above a glowing base ring, its underside and rim glyphs aglow. The
//! suspended dais of the site; its glow is emissive trim the ruin pass can
//! darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ENERGY_BLUE, GLYPH_CYAN, OBSIDIAN, fx, obsidian};

pub struct LevitatingPlatform;

impl CatalogueEntry for LevitatingPlatform {
    fn slug(&self) -> &'static str {
        "levitating_platform"
    }
    fn name(&self) -> &'static str {
        "Levitating Platform"
    }
    fn description(&self) -> &'static str {
        "Obsidian slab hovering above a glowing base ring, underside and rim aglow."
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
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plat_y = 2.6_f32;

    let mut prims = vec![
        // Obsidian base disc — the root.
        prim(
            solid(cylinder_tapered(2.0, 0.3, 24, 0.0, obsidian(OBSIDIAN))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];
    // Glowing base ring — emissive.
    prims.push(prim(
        torus(0.1, 1.7, glow(ENERGY_BLUE, 2.4)),
        [0.0, 0.34, 0.0],
        id_quat(),
    ));

    // Hovering platform slab.
    prims.push(prim(
        solid(cuboid_tapered([5.0, 0.6, 5.0], 0.0, obsidian(OBSIDIAN))),
        [0.0, plat_y, 0.0],
        id_quat(),
    ));
    // Glowing underside — emissive.
    prims.push(prim(
        cuboid_tapered([4.4, 0.12, 4.4], 0.0, glow(ENERGY_BLUE, 2.2)),
        [0.0, plat_y - 0.32, 0.0],
        id_quat(),
    ));
    // Glowing rim glyphs along two edges — emissive.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([4.6, 0.16, 0.12], 0.0, glow(GLYPH_CYAN, 2.4)),
            [0.0, plat_y + 0.3, sz * 2.4],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: energy motes rising in the levitation field.
    root.children
        .push(fx::energy_motes([0.0, 1.2, 0.0], 0x0A30_9012));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&LevitatingPlatform.build(""), "levitating_platform");
    }
}
