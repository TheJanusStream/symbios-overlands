//! Energy node — an Alien-Monolithic prop. A glowing orb suspended above a
//! black pedestal within a glowing ring. Scatter clutter of the site; the orb
//! is emissive trim the ruin pass can darken.

use crate::catalogue::items::fantasy::rune_marks;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ENERGY_BLUE, GLYPH_CYAN, OBSIDIAN, obsidian};

pub struct EnergyNode;

impl CatalogueEntry for EnergyNode {
    fn slug(&self) -> &'static str {
        "energy_node"
    }
    fn name(&self) -> &'static str {
        "Energy Node"
    }
    fn description(&self) -> &'static str {
        "Glowing orb suspended above a black pedestal within a glowing ring."
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
            clearance: 0.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Obsidian pedestal — the root.
        prim(
            solid(cuboid_tapered([0.6, 0.8, 0.6], 0.2, obsidian(OBSIDIAN))),
            [0.0, 0.4, 0.0],
            id_quat(),
        ),
        // Thin glowing suspension strut from the pedestal to the orb.
        prim(
            cylinder_tapered(0.04, 0.45, 8, 0.0, glow(ENERGY_BLUE, 2.0)),
            [0.0, 0.95, 0.0],
            id_quat(),
        ),
        // Glowing suspension ring around the orb.
        prim(
            torus(0.06, 0.42, glow(ENERGY_BLUE, 2.2)),
            [0.0, 1.25, 0.0],
            id_quat(),
        ),
        // Suspended glowing orb — emissive, rounder (res 6) than the old
        // blocky res-3 ball.
        prim(
            sphere(0.28, 6, glow(ENERGY_BLUE, 3.0)),
            [0.0, 1.25, 0.0],
            id_quat(),
        ),
    ];
    // A glyph inscribed on the pedestal's −Z hero face — emissive.
    prims.extend(rune_marks([0.0, 0.42, -0.32], 0.45, glow(GLYPH_CYAN, 2.3)));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&EnergyNode.build(""), "energy_node");
    }
}
