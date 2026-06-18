//! Glow-mushroom — a High-Fantasy prop. A cluster of luminous toadstools, pale
//! stems under glowing caps. Scatter clutter lighting the arcane quarter; the
//! caps are emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{assemble, cone, cylinder_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MUSH_GLOW, STONE_MOSS, matte};

/// Pale stem colour.
const STEM: [f32; 3] = [0.82, 0.84, 0.78];

pub struct GlowMushroom;

impl CatalogueEntry for GlowMushroom {
    fn slug(&self) -> &'static str {
        "glow_mushroom"
    }
    fn name(&self) -> &'static str {
        "Glow-Mushroom"
    }
    fn description(&self) -> &'static str {
        "Cluster of luminous toadstools, pale stems under glowing caps."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
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

/// One mushroom (stem + glowing cap) returned for the assemble list.
fn mushroom(pos: [f32; 3], scale: f32) -> Generator {
    let stem_h = 0.6 * scale;
    let mut stem = prim(
        solid(cylinder_tapered(0.08 * scale, stem_h, 6, 0.1, matte(STEM))),
        pos,
        id_quat(),
    );
    stem.children.push(prim(
        cone(0.3 * scale, 0.4 * scale, 8, glow(MUSH_GLOW, 2.2)),
        [0.0, stem_h * 0.5 + 0.1 * scale, 0.0],
        id_quat(),
    ));
    stem
}

fn build_tree() -> Generator {
    let prims = vec![
        // A mossy clump base — the root.
        mushroom([0.0, 0.3, 0.0], 1.2),
        mushroom([0.5, 0.2, 0.2], 0.8),
        mushroom([-0.4, 0.18, 0.3], 0.7),
        mushroom([0.2, 0.15, -0.4], 0.6),
    ];
    let mut root = assemble(prims);
    // A few flecks of moss at the base.
    root.children.push(prim(
        solid(cylinder_tapered(0.7, 0.06, 12, 0.0, matte(STONE_MOSS))),
        [0.0, -0.27, 0.0],
        id_quat(),
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GlowMushroom.build(""), "glow_mushroom");
    }
}
