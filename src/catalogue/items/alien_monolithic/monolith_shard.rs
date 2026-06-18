//! Monolith shard — an Alien-Monolithic prop. A splinter of black obsidian
//! jutting from the ground at a sharp angle, a glyph still lit along its edge.
//! Scatter clutter of the site; the glyph is emissive trim the ruin pass can
//! darken.
//!
//! The shard leans with a [`quat_x`].

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLYPH_CYAN, OBSIDIAN, obsidian};

pub struct MonolithShard;

impl CatalogueEntry for MonolithShard {
    fn slug(&self) -> &'static str {
        "monolith_shard"
    }
    fn name(&self) -> &'static str {
        "Monolith Shard"
    }
    fn description(&self) -> &'static str {
        "Splinter of black obsidian jutting at a sharp angle, a glyph lit along its edge."
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
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Leaning obsidian shard — the root.
        prim(
            solid(cuboid_tapered([0.6, 2.6, 0.5], 0.4, obsidian(OBSIDIAN))),
            [0.0, 1.2, 0.0],
            quat_x(0.35),
        ),
        // Glowing glyph along the shard's edge — emissive.
        prim(
            cuboid_tapered([0.12, 1.8, 0.52], 0.3, glow(GLYPH_CYAN, 2.6)),
            [0.0, 1.25, 0.1],
            quat_x(0.35),
        ),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&MonolithShard.build(""), "monolith_shard");
    }
}
