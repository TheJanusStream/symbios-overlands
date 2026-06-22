//! Gravestone — a Gothic-Horror prop. A single weathered headstone leaning
//! over a low grave mound. Scatter clutter strewn through the necropolis.
//!
//! The stone leans with a single [`quat_x`].

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_MOSS, mossy};

pub struct Gravestone;

impl CatalogueEntry for Gravestone {
    fn slug(&self) -> &'static str {
        "gravestone"
    }
    fn name(&self) -> &'static str {
        "Gravestone"
    }
    fn description(&self) -> &'static str {
        "Weathered headstone leaning over a low grave mound."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
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

fn build_tree() -> Generator {
    let ms = || mossy(STONE_MOSS);
    let lean = 0.13_f32;
    let mut prims = vec![
        // Heaped grave mound — the root (rounded low hump).
        prim(
            solid(cylinder_tapered(0.82, 0.34, 16, 0.6, ms())),
            [0.0, 0.14, 0.35],
            id_quat(),
        ),
    ];

    // Low kerb stones bordering the grave.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 0.16, 1.7], 0.0, ms())),
            [s * 0.7, 0.1, 0.35],
            id_quat(),
        ));
    }

    // Leaning round-topped headstone.
    let depth = 0.17_f32;
    let h = 1.1_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.72, h, depth], 0.05, ms())),
        [0.0, 0.2 + h * 0.5, -0.5],
        quat_x(lean),
    ));
    prims.push(prim(
        solid(with_cut(
            cylinder_tapered(0.36, depth, 14, 0.0, ms()),
            [0.5, 1.0],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, 0.2 + h, -0.5],
        quat_x(lean + FRAC_PI_2),
    ));

    // A small leaning footstone at the foot of the grave.
    prims.push(prim(
        solid(cuboid_tapered([0.46, 0.36, 0.13], 0.08, ms())),
        [0.0, 0.32, 1.25],
        quat_x(-0.12),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Gravestone.build(""), "gravestone");
    }
}
