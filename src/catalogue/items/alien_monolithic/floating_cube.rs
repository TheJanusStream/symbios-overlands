//! Floating cube — an Alien-Monolithic prop. A black obsidian cube hovering
//! above a glowing ground-mark, lit from a core within. Scatter clutter of the
//! site; the core is emissive trim the ruin pass can darken.

use crate::catalogue::items::fantasy::rune_marks;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ENERGY_BLUE, GLYPH_CYAN, OBSIDIAN, obsidian};

pub struct FloatingCube;

impl CatalogueEntry for FloatingCube {
    fn slug(&self) -> &'static str {
        "floating_cube"
    }
    fn name(&self) -> &'static str {
        "Floating Cube"
    }
    fn description(&self) -> &'static str {
        "Black obsidian cube hovering above a glowing ground-mark, lit from a core within."
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
            clearance: 0.8,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let cy = 1.4_f32; // cube centre
    let h = 0.45_f32; // cube half-extent

    let mut prims = vec![
        // Glowing ground-mark — the root.
        prim(
            cylinder_tapered(0.7, 0.05, 16, 0.0, glow(ENERGY_BLUE, 2.0)),
            [0.0, 0.03, 0.0],
            id_quat(),
        ),
        // Hovering obsidian cube.
        prim(
            solid(cuboid_tapered([0.9, 0.9, 0.9], 0.0, obsidian(OBSIDIAN))),
            [0.0, cy, 0.0],
            id_quat(),
        ),
        // Glowing core within the cube — emissive ambiance.
        prim(
            sphere(0.22, 3, glow(GLYPH_CYAN, 2.8)),
            [0.0, cy, 0.0],
            id_quat(),
        ),
    ];
    // Glowing edge seams down the four vertical corners — the cube's powered
    // core leaking through, so "lit from within" actually reads (the bare
    // sealed core was invisible inside the solid obsidian).
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([0.07, 0.96, 0.07], 0.0, glow(GLYPH_CYAN, 2.2)),
                [sx * h, cy, sz * h],
                id_quat(),
            ));
        }
    }
    // A glyph inscribed on the −Z hero face — emissive.
    prims.extend(rune_marks(
        [0.0, cy, -(h + 0.02)],
        0.5,
        glow(GLYPH_CYAN, 2.4),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FloatingCube.build(""), "floating_cube");
    }
}
