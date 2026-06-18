//! Glyph rubble — an Alien-Monolithic *poor* prop. A scatter of shattered
//! dead-stone fragments, their glyph-grooves dark and cold. The debris of the
//! dormant site.
//!
//! A couple of fragments lie tipped with a [`quat_x`].

use crate::catalogue::items::util::{assemble, cuboid_tapered, prim, quat_x, solid};
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
        // Largest fragment — the root, half-buried and tipped.
        prim(
            solid(cuboid_tapered([1.0, 1.4, 0.6], 0.2, stone(DEAD_STONE))),
            [0.0, 0.5, 0.0],
            quat_x(0.5),
        ),
    ];
    // Dark glyph groove on the largest fragment (no glow).
    prims.push(prim(
        cuboid_tapered([0.14, 0.9, 0.62], 0.0, stone([0.12, 0.12, 0.14])),
        [0.0, 0.55, 0.05],
        quat_x(0.5),
    ));

    // Smaller scattered fragments.
    for (fx, fz, s, tilt) in [
        (0.9_f32, 0.3_f32, 0.5_f32, 0.0_f32),
        (-0.7, 0.4, 0.45, 0.8),
        (0.2, -0.8, 0.4, 0.0),
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
