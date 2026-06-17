//! Koi pond — a Feudal-Japan prop. A boulder-rimmed pool of still dark
//! water with a few koi gliding under the surface, lily pads, and a low
//! stone stepping-bridge across one edge.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{KOI_ORANGE, STONE_GREY, WATER_BLUE, lacquer, rough_stone, stone, timber, water};

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
    let r = 2.2_f32;

    let mut prims = vec![
        // Still water surface — the root.
        prim(
            solid(cylinder_tapered(r, 0.18, 24, 0.0, water(WATER_BLUE))),
            [0.0, 0.12, 0.0],
            id_quat(),
        ),
    ];

    // Boulder rim around the edge.
    let rim = 7;
    for k in 0..rim {
        let a = k as f32 / rim as f32 * std::f32::consts::TAU;
        let rr = r + 0.25;
        let s = 0.45 + (k % 3) as f32 * 0.12;
        prims.push(prim(
            solid(sphere(s, 3, rough_stone(STONE_GREY))),
            [a.cos() * rr, 0.18, a.sin() * rr],
            id_quat(),
        ));
    }

    // A few koi gliding just under the surface.
    let koi = [
        (-0.6_f32, 0.4_f32, 0.5_f32),
        (0.7, -0.5, 1.2),
        (0.1, 0.9, -0.6),
    ];
    for (x, z, yaw) in koi {
        prims.push(prim(
            cuboid_tapered([0.55, 0.12, 0.18], 0.4, lacquer(KOI_ORANGE)),
            [x, 0.16, z],
            quat_y(yaw),
        ));
    }

    // Lily pads floating on the surface.
    for (x, z) in [(-1.2_f32, -0.8_f32), (1.0, 0.9)] {
        prims.push(prim(
            cylinder_tapered(0.35, 0.04, 12, 0.0, timber(LILY_GREEN)),
            [x, 0.21, z],
            id_quat(),
        ));
    }

    // Low stone stepping-bridge across one edge.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.18, r * 2.2], 0.0, stone(STONE_GREY))),
        [r * 0.4, 0.45, 0.0],
        id_quat(),
    ));

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
