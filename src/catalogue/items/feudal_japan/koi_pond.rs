//! Koi pond — a Feudal-Japan prop. A boulder-rimmed pool of still dark
//! water with a few koi gliding under the surface, lily pads, and a low
//! stone stepping-bridge across one edge.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_y, solid,
    sphere, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{KOI_ORANGE, LACQUER_RED, STONE_GREY, WATER_BLUE, lacquer, rough_stone, timber, water};

/// Lily-pad green.
const LILY_GREEN: [f32; 3] = [0.22, 0.40, 0.20];

pub struct KoiPond;

impl CatalogueEntry for KoiPond {
    fn slug(&self) -> &'static str {
        "koi_pond"
    }
    fn name(&self) -> &'static str {
        "Koi Pond"
    }
    fn description(&self) -> &'static str {
        "Boulder-rimmed pool of still water with koi, lily pads, and a stone bridge."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let r = 2.2_f32;

    let mut prims = vec![
        // Still water surface — the root.
        prim(
            solid(cylinder_tapered(r, 0.22, 28, 0.0, water(WATER_BLUE))),
            [0.0, 0.13, 0.0],
            id_quat(),
        ),
    ];

    // Low fieldstone rim — small stones ringing the water's edge rather than
    // a boulder pile that buries the pool.
    let rim = 14;
    for k in 0..rim {
        let a = k as f32 / rim as f32 * TAU;
        let rr = r + 0.02;
        let s = 0.22 + (k % 3) as f32 * 0.05;
        prims.push(prim(
            solid(sphere(s, 3, rough_stone(STONE_GREY))),
            [a.cos() * rr, 0.04, a.sin() * rr],
            id_quat(),
        ));
    }

    // A few koi gliding just under the surface, riding proud of the water so
    // their colour reads.
    let koi = [
        (-0.7_f32, 0.5_f32, 0.5_f32),
        (0.8, -0.5, 1.2),
        (0.1, 1.0, -0.6),
    ];
    for (x, z, yaw) in koi {
        prims.push(prim(
            cuboid_tapered([0.62, 0.14, 0.2], 0.4, lacquer(KOI_ORANGE)),
            [x, 0.22, z],
            quat_y(yaw),
        ));
    }

    // Lily pads floating on the surface.
    for (x, z) in [(-1.35_f32, -0.7_f32), (1.1, 0.9), (0.4, -1.45)] {
        prims.push(prim(
            cylinder_tapered(0.32, 0.05, 12, 0.0, timber(LILY_GREEN)),
            [x, 0.24, z],
            id_quat(),
        ));
    }

    // Arched red taikobashi (drum bridge): a semicircular lacquer arch
    // spanning the pond, with a thin handrail arch down each side.
    let major = 1.6_f32;
    // Yawed off-axis (and a fraction thicker) so the arch reads from every
    // view instead of collapsing to an edge-on red sliver.
    let yaw = 0.5_f32;
    let stand = quat_mul(quat_y(yaw), quat_x(-FRAC_PI_2));
    let deck = with_cut(
        torus(0.26, major, lacquer(LACQUER_RED)),
        [0.0, 0.5],
        [0.0, 1.0],
        0.0,
    );
    prims.push(prim(solid(deck), [0.0, 0.12, 0.0], stand));
    for sz in [-1.0_f32, 1.0] {
        let rail = with_cut(
            torus(0.05, major - 0.04, lacquer(LACQUER_RED)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        );
        let off = sz * 0.42;
        prims.push(prim(rail, [off * yaw.sin(), 0.5, off * yaw.cos()], stand));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&KoiPond.build(""), "koi_pond");
    }
}
