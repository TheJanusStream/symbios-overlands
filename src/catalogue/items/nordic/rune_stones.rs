//! Rune stones — a Nordic secondary. A small cluster of weathered standing
//! stones, the tallest carved on its shore-facing front with a glowing runic
//! serpent ring and glyph columns; a memorial raised beside the steading.
//! Dressed ashlar, each satellite stone leaning a little off true.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_x, quat_y, solid, torus,
};
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
        "Cluster of standing stones carved with a faintly glowing runic serpent."
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
    let depth = 0.55_f32;
    let zf = -(depth * 0.5 + 0.02); // carved face is the -Z (shore) hero front

    // Central memorial stone (root), the tallest, carved with runes. Kept
    // upright so the satellite stones don't inherit a root lean.
    let mut prims = vec![prim(
        solid(cuboid_tapered([1.4, 4.2, depth], 0.16, stone(STONE_COLD))),
        [0.0, 2.1, 0.0],
        id_quat(),
    )];

    // Glowing runic serpent ring on the front face (the Jelling-stone loop).
    prims.push(prim(
        torus(0.055, 0.52, glow(RUNE_GLOW, 1.9)),
        [0.0, 2.5, zf],
        quat_x(FRAC_PI_2),
    ));
    // Glyph column running down inside the ring.
    for k in 0..3 {
        prims.push(prim(
            cuboid_tapered([0.16, 0.5, 0.05], 0.0, glow(RUNE_GLOW, 1.7)),
            [0.0, 1.85 + k as f32 * 0.62, zf],
            id_quat(),
        ));
    }
    // Short flanking rune bands below the ring.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.22, 0.14, 0.05], 0.0, glow(RUNE_GLOW, 1.6)),
            [sx * 0.42, 1.05, zf],
            id_quat(),
        ));
    }

    // Satellite stones, shorter and boulder-rough, each yawed off-axis and
    // strongly tapered to a weathered crown.
    let ring: [(f32, f32, f32, f32); 4] = [
        // (x, z, height, yaw)
        (-1.9, 0.7, 2.7, 0.5),
        (2.0, 0.4, 3.0, -0.4),
        (-1.2, -1.7, 2.3, 1.1),
        (1.5, -1.9, 2.5, -0.9),
    ];
    for (x, z, h, yaw) in ring {
        prims.push(prim(
            solid(cuboid_tapered([0.95, h, 0.5], 0.38, stone(STONE_GREY))),
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
