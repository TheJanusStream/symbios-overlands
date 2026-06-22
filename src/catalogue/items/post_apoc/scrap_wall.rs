//! Scrap wall — a Post-apocalyptic prop. A barrier of mismatched corrugated
//! and plate metal welded to leaning posts. Scatter clutter fencing the
//! holdout.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CORRUGATED_RUST, PLANK_GREY, RUST_BROWN, STEEL_GREY, TIRE_BLACK, plank, rusted, sheet, tarp,
};

pub struct ScrapWall;

impl CatalogueEntry for ScrapWall {
    fn slug(&self) -> &'static str {
        "scrap_wall"
    }
    fn name(&self) -> &'static str {
        "Scrap Wall"
    }
    fn description(&self) -> &'static str {
        "Barrier of mismatched corrugated and plate metal welded to leaning posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Tallest corrugated panel — the root.
        prim(
            solid(cuboid_tapered(
                [1.4, 2.4, 0.12],
                0.0,
                sheet(CORRUGATED_RUST),
            )),
            [-1.2, 1.2, 0.0],
            id_quat(),
        ),
    ];
    // Mismatched panels of varying height welded alongside, each leaning its
    // own way — the lurching, never-plumb line of a scavenged barrier.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 2.0, 0.14], 0.0, rusted(STEEL_GREY))),
        [0.2, 1.0, 0.05],
        quat_z(0.07),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.2, 1.6, 0.12], 0.0, sheet(RUST_BROWN))),
        [1.4, 0.8, -0.04],
        quat_z(-0.05),
    ));
    // A low salvaged plank patch filling a gap at the base.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.0, 0.1], 0.0, plank(PLANK_GREY))),
        [-0.55, 0.5, 0.08],
        quat_z(0.04),
    ));

    // Leaning support posts, actually canted now.
    for (i, x) in [-1.8_f32, 0.0, 1.9].into_iter().enumerate() {
        let lean = if i % 2 == 0 { 0.09 } else { -0.07 };
        prims.push(prim(
            solid(cuboid_tapered([0.12, 2.3, 0.12], 0.0, rusted(STEEL_GREY))),
            [x, 1.15, -0.12],
            quat_z(lean),
        ));
    }
    // A taut top wire strung between the posts, suggesting barbed defence.
    prims.push(prim(
        solid(cylinder_tapered(0.025, 3.9, 4, 0.0, rusted(STEEL_GREY))),
        [0.05, 2.35, -0.12],
        quat_z(std::f32::consts::FRAC_PI_2),
    ));
    // A hubcap wired to the steel panel, its face turned to the camera (−Z).
    prims.push(prim(
        solid(torus(0.05, 0.26, rusted(STEEL_GREY))),
        [0.2, 1.4, -0.2],
        quat_x(std::f32::consts::FRAC_PI_2),
    ));
    // A faded warning board nailed up at an angle.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.5, 0.04],
            0.0,
            tarp([0.55, 0.42, 0.12]),
        )),
        [1.4, 1.5, -0.12],
        quat_y(0.2),
    ));
    // A bald tyre slumped against the foot of the wall.
    prims.push(prim(
        solid(torus(0.16, 0.4, tarp(TIRE_BLACK))),
        [-1.9, 0.42, 0.45],
        quat_z(0.25),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ScrapWall.build(""), "scrap_wall");
    }
}
