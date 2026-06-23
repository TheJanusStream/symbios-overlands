//! Rec court — the Sports/Recreation *poor* landmark. A cracked asphalt
//! multi-use court with faded markings, a bent basketball hoop and a sagging
//! chain-link fence. The hardscrabble counterpart to the
//! [`stadium`](super::stadium): same sport, opposite end of the prosperity
//! axis (`Poor`), so a destitute sports room grows the municipal court
//! instead of the ground.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the court slab.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ASPHALT_DARK, CHAIN_GREY, COURT_BLUE, HOOP_ORANGE, LINE_WHITE, STEEL_GREY, asphalt, chainlink,
    enamel, painted, steel,
};

/// Near-black of the cracks veining the worn asphalt.
const CRACK_DARK: [f32; 3] = [0.05, 0.05, 0.06];

pub struct RecCourt;

impl CatalogueEntry for RecCourt {
    fn slug(&self) -> &'static str {
        "rec_court"
    }
    fn name(&self) -> &'static str {
        "Rec Court"
    }
    fn description(&self) -> &'static str {
        "Cracked asphalt court with faded markings, a bent hoop and a sagging fence."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.2_f32;

    let mut prims = vec![
        // Cracked asphalt court — the root.
        prim(
            solid(cuboid_tapered(
                [14.0, pad_h, 9.0],
                0.0,
                asphalt(ASPHALT_DARK),
            )),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Faded painted key + centre line + free-throw circle.
    prims.push(prim(
        cuboid_tapered([4.0, 0.05, 3.0], 0.0, painted(COURT_BLUE)),
        [-4.5, pad_h + 0.03, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.25, 0.06, 9.0], 0.0, painted(LINE_WHITE)),
        [0.0, pad_h + 0.04, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.04, 1.2, painted(LINE_WHITE)),
        [-2.5, pad_h + 0.04, 0.0],
        quat_x(FRAC_PI_2),
    ));
    // A few cracks veining the worn surface (Poor).
    for (i, (x, z, yaw)) in [
        (-3.0_f32, 2.6_f32, 0.4_f32),
        (2.0, -2.2, -0.6),
        (4.5, 1.5, 0.2),
    ]
    .iter()
    .enumerate()
    {
        let len = 2.4 + i as f32 * 0.4;
        prims.push(prim(
            cuboid_tapered([len, 0.05, 0.07], 0.0, painted(CRACK_DARK)),
            [*x, pad_h + 0.045, *z],
            quat_y(*yaw),
        ));
    }

    // Basketball hoop on a leaning pole at the −X end, facing the −Z front so
    // the board, target square, rim and net all read to the camera.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 3.4, 8, 0.05, steel(STEEL_GREY))),
        [-6.0, pad_h + 1.7, 0.45],
        quat_x(0.08),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.6, 1.0, 0.12], 0.0, painted(LINE_WHITE))),
        [-6.0, pad_h + 3.0, 0.0],
        id_quat(),
    ));
    // Painted target square, proud of the board's −Z face.
    prims.push(prim(
        cuboid_tapered([0.7, 0.5, 0.04], 0.0, enamel(HOOP_ORANGE)),
        [-6.0, pad_h + 2.95, -0.1],
        id_quat(),
    ));
    // Rim sticking out toward the camera, with a sagging chain-link net.
    prims.push(prim(
        solid(torus(0.05, 0.35, enamel(HOOP_ORANGE))),
        [-6.0, pad_h + 2.65, -0.38],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cylinder_tapered(0.32, 0.45, 8, 0.55, chainlink(CHAIN_GREY)),
        [-6.0, pad_h + 2.4, -0.38],
        id_quat(),
    ));

    // Sagging chain-link fence along the two ends.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.05, 2.4, 9.0], 0.0, chainlink(CHAIN_GREY)),
            [sx * 7.0, pad_h + 1.2, 0.0],
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
        assert_sanitize_stable(&RecCourt.build(""), "rec_court");
    }
}
