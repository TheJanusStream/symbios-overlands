//! Monolith shard — an Alien-Monolithic prop. A splinter of black obsidian
//! jutting from the ground at a sharp angle, a glyph still lit along its edge.
//! Scatter clutter of the site; the glyph is emissive trim the ruin pass can
//! darken.
//!
//! The leaning shard is a child of a flat ground chip (its [`quat_x`] lean
//! must not sit on the `assemble` root, or it would scramble every sibling).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_mul, quat_x, quat_z,
    solid,
};
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
    let lean = quat_x(0.35);

    let prims = vec![
        // Flat obsidian ground chip — the root (identity rotation, so the
        // leaning shard's tilt stays on the shard alone).
        prim(
            solid(cylinder_tapered(0.6, 0.12, 12, 0.0, obsidian(OBSIDIAN))),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
        // Leaning obsidian shard — a child carrying the tilt.
        prim(
            solid(cuboid_tapered([0.6, 2.6, 0.5], 0.4, obsidian(OBSIDIAN))),
            [0.0, 1.2, 0.0],
            lean,
        ),
        // Glowing glyph stave up the shard's −Z edge — emissive.
        prim(
            cuboid_tapered([0.1, 1.6, 0.06], 0.0, glow(GLYPH_CYAN, 2.5)),
            [0.0, 1.25, -0.22],
            lean,
        ),
        // Angled glyph branch off the stave.
        prim(
            cuboid_tapered([0.5, 0.1, 0.06], 0.0, glow(GLYPH_CYAN, 2.5)),
            [0.16, 1.55, -0.27],
            quat_mul(lean, quat_z(-0.5)),
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
