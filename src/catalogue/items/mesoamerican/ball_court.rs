//! Ball court — a Mesoamerican secondary. A sunken playing alley flanked by
//! two battered stone benches and high vertical walls, each wall carrying a
//! carved stone ring goal, with a marker disc set at centre court. The arena
//! of the sacred ballgame.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, quat_z, solid, torus, wedge,
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
    let wall_h = 3.4;
    let alley_hw = 2.5; // alley floor half-width (X)
    let bench_run = 1.9; // horizontal run of the sloped talud
    let bench_h = 1.5; // talud rise to the wall base
    let bench_top = alley_hw + bench_run; // 4.4 — wall foot
    let wall_w = 1.0;

    let mut prims = vec![
        // Playing-alley floor — the root.
        prim(
            solid(cuboid_tapered(
                [alley_hw * 2.0, 0.3, len],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    for sx in [-1.0_f32, 1.0] {
        // Sloped talud bench — a wedge rising from the alley floor to the
        // wall foot, its vertical back against the wall and its slope facing
        // the court. Rotated so the wedge's width runs the court length and
        // its rise climbs outward toward the wall.
        let bench_rot = if sx > 0.0 {
            quat_y(-FRAC_PI_2)
        } else {
            quat_y(FRAC_PI_2)
        };
        prims.push(prim(
            solid(wedge([len, bench_h, bench_run], limestone(LIMESTONE_PALE))),
            [sx * (alley_hw + bench_run * 0.5), 0.3 + bench_h * 0.5, 0.0],
            bench_rot,
        ));
        // High vertical tablero wall standing on the talud's outer foot.
        prims.push(prim(
            solid(cuboid_tapered(
                [wall_w, wall_h, len],
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [
                sx * (bench_top + wall_w * 0.5),
                0.3 + bench_h + wall_h * 0.5,
                0.0,
            ],
            id_quat(),
        ));
        // A painted red band course along the wall foot (the talud-tablero
        // moulding).
        prims.push(prim(
            solid(cuboid_tapered([0.35, 0.6, len], 0.0, painted(STUCCO_RED))),
            [sx * (bench_top - 0.1), 0.3 + bench_h + 0.3, 0.0],
            id_quat(),
        ));
        // Carved stone ring goal — a chunky donut mounted flat on the inner
        // wall face, standing vertical with its hole facing across the court
        // toward the opposing ring (the Chichén-style mount).
        prims.push(prim(
            torus(0.26, 0.62, painted(STUCCO_RED)),
            [sx * (bench_top - 0.05), 0.3 + bench_h + wall_h * 0.55, 0.0],
            quat_z(FRAC_PI_2),
        ));
    }

    // Centre-court marker disc.
    prims.push(prim(
        solid(cylinder_tapered(0.75, 0.18, 16, 0.0, painted(STONE_GREY))),
        [0.0, 0.33, 0.0],
        id_quat(),
    ));
    // End-zone transverse banks — low taluds closing each end into the
    // I-shaped plan of a ball court, extending out past the side ranges to
    // form the serifs.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [(bench_top + wall_w) * 2.0, 1.1, 1.4],
                0.12,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, 0.55, sz * (len * 0.5 - 0.5)],
            id_quat(),
        ));
        // A carved marker stone set into the end-zone bank, facing the court.
        prims.push(prim(
            solid(cuboid_tapered([1.1, 0.7, 0.4], 0.1, painted(STONE_GREY))),
            [0.0, 0.9, sz * (len * 0.5 - 1.3)],
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
