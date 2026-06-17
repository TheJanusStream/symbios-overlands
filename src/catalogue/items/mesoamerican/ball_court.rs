//! Ball court — a Mesoamerican secondary. A sunken playing alley flanked by
//! two battered stone benches and high vertical walls, each wall carrying a
//! carved stone ring goal, with a marker disc set at centre court. The arena
//! of the sacred ballgame.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{LIMESTONE_PALE, STONE_GREY, STUCCO_CREAM, STUCCO_RED, limestone, painted};

pub struct BallCourt;

impl CatalogueEntry for BallCourt {
    fn slug(&self) -> &'static str {
        "ball_court"
    }
    fn name(&self) -> &'static str {
        "Ball Court"
    }
    fn description(&self) -> &'static str {
        "Sunken stone alley flanked by sloped benches and ring-goal walls."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let len = 16.0_f32; // along Z
    let wall_h = 4.0;

    let mut prims = vec![
        // Playing-alley floor — the root.
        prim(
            solid(cuboid_tapered(
                [6.0, 0.3, len],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    for sx in [-1.0_f32, 1.0] {
        // Lower sloped bench.
        prims.push(prim(
            solid(cuboid_tapered(
                [2.0, 1.4, len],
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [sx * 2.6, 0.7, 0.0],
            id_quat(),
        ));
        // High vertical wall above the bench.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.1, wall_h, len],
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [sx * 4.1, 1.4 + wall_h * 0.5, 0.0],
            id_quat(),
        ));
        // Carved stone ring goal mounted high on the inner face, standing
        // vertical (hole faces along the court).
        prims.push(prim(
            torus(0.16, 0.6, painted(STUCCO_RED)),
            [sx * 3.5, 1.4 + wall_h * 0.7, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    // Centre-court marker disc.
    prims.push(prim(
        solid(cylinder_tapered(0.7, 0.16, 16, 0.0, painted(STONE_GREY))),
        [0.0, 0.32, 0.0],
        id_quat(),
    ));
    // End-zone marker stones.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.0, 0.6, 0.6], 0.1, limestone(STONE_GREY))),
            [0.0, 0.45, sz * (len * 0.5 - 0.6)],
            id_quat(),
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
        assert_sanitize_stable(&BallCourt.build(""), "ball_court");
    }
}
