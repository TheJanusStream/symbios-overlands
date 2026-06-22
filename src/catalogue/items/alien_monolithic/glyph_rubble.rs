//! Glyph rubble — an Alien-Monolithic *poor* prop. A scatter of shattered
//! dead-stone fragments, their glyph-grooves dark and cold. The debris of the
//! dormant site.
//!
//! The tipped fragments hang off a flat embedded base fragment — their
//! [`quat_x`] tilts must not sit on the `assemble` root, or it would scramble
//! every sibling into the root's frame.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEAD_STONE, stone};

pub struct GlyphRubble;

impl CatalogueEntry for GlyphRubble {
    fn slug(&self) -> &'static str {
        "glyph_rubble"
    }
    fn name(&self) -> &'static str {
        "Glyph Rubble"
    }
    fn description(&self) -> &'static str {
        "Scatter of shattered dead-stone fragments, their glyph-grooves dark and cold."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MONOLITH_POOR
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
    let mut prims = vec![
        // Flat half-buried base fragment — the root (identity rotation, so the
        // tipped fragments' tilts stay on themselves alone).
        prim(
            solid(cuboid_tapered([1.3, 0.5, 1.0], 0.15, stone(DEAD_STONE))),
            [0.0, 0.22, 0.0],
            id_quat(),
        ),
    ];

    // The largest broken shard, tipped and leaning on the base.
    let lean = quat_x(0.55);
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.3, 0.55], 0.2, stone(DEAD_STONE))),
        [0.15, 0.65, 0.1],
        lean,
    ));
    // Dark glyph groove down the leaning shard's −Z face (no glow).
    prims.push(prim(
        cuboid_tapered([0.12, 0.85, 0.06], 0.0, stone([0.12, 0.12, 0.14])),
        [0.15, 0.7, -0.18],
        lean,
    ));

    // Smaller scattered fragments.
    for (fx, fz, s, tilt) in [
        (0.95_f32, 0.35_f32, 0.5_f32, 0.0_f32),
        (-0.8, 0.45, 0.45, 0.8),
        (0.25, -0.85, 0.4, 0.3),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([s, s * 0.7, s], 0.3, stone(DEAD_STONE))),
            [fx, s * 0.35, fz],
            quat_x(tilt),
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
        assert_sanitize_stable(&GlyphRubble.build(""), "glyph_rubble");
    }
}
