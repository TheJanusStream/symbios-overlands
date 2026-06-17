//! Rune stones — a Nordic secondary. A small cluster of weathered standing
//! stones, the tallest carved with glyphs that hold a faint cold-blue
//! glimmer; a memorial raised beside the steading. Dressed ashlar, each
//! stone leaning a little off true.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_y, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_COLD, STONE_GREY, stone};

/// Cold rune-light worked into the carved faces.
const RUNE_GLOW: [f32; 3] = [0.42, 0.62, 0.92];

pub struct RuneStones;

impl CatalogueEntry for RuneStones {
    fn slug(&self) -> &'static str {
        "rune_stones"
    }
    fn name(&self) -> &'static str {
        "Rune Stones"
    }
    fn description(&self) -> &'static str {
        "Cluster of standing stones carved with faintly glowing runes."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Central memorial stone (root), the tallest, carved with runes. Kept
    // upright so the satellite stones don't inherit a root lean.
    let mut prims = vec![prim(
        solid(cuboid_tapered([1.3, 4.0, 0.5], 0.18, stone(STONE_COLD))),
        [0.0, 2.0, 0.0],
        id_quat(),
    )];
    // Three carved rune bands down its front face.
    for k in 0..3 {
        prims.push(prim(
            cuboid_tapered([0.7, 0.16, 0.06], 0.0, glow(RUNE_GLOW, 1.8)),
            [0.0, 1.4 + k as f32 * 0.7, 0.27],
            id_quat(),
        ));
    }

    // Satellite stones, shorter, each yawed off-axis around the centre.
    let ring: [(f32, f32, f32, f32); 4] = [
        // (x, z, height, yaw)
        (-1.8, 0.6, 2.8, 0.5),
        (1.9, 0.3, 3.1, -0.4),
        (-1.1, -1.7, 2.4, 1.1),
        (1.4, -1.9, 2.6, -0.9),
    ];
    for (x, z, h, yaw) in ring {
        prims.push(prim(
            solid(cuboid_tapered([0.9, h, 0.4], 0.2, stone(STONE_GREY))),
            [x, h * 0.5, z],
            quat_y(yaw),
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
        assert_sanitize_stable(&RuneStones.build(""), "rune_stones");
    }
}
