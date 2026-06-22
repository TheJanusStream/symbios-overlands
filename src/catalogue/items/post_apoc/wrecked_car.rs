//! Wrecked car — a Post-apocalyptic prop. A rusted, stripped, burnt-out car
//! body sagging on flat tyres. Scatter clutter strewn across the wasteland.
//!
//! Wheels run their axles along Z via a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid, torus, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CAR_RUST, STEEL_GREY, TIRE_BLACK, rusted, tarp};

/// Burnt-out glass / charred void colour for smashed windows and scorch.
const CHARRED: [f32; 3] = [0.09, 0.08, 0.08];

pub struct WreckedCar;

impl CatalogueEntry for WreckedCar {
    fn slug(&self) -> &'static str {
        "wrecked_car"
    }
    fn name(&self) -> &'static str {
        "Wrecked Car"
    }
    fn description(&self) -> &'static str {
        "Rusted, stripped, burnt-out car body sagging on flat tyres."
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
            clearance: 2.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Lower body / chassis — the root.
        prim(
            solid(cuboid_tapered([3.6, 0.6, 1.6], 0.05, rusted(CAR_RUST))),
            [0.0, 0.55, 0.0],
            id_quat(),
        ),
    ];
    // Bonnet sloping down over the gutted engine bay (front, +X).
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.35, 1.5], 0.06, rusted(CAR_RUST))),
        [1.45, 0.95, 0.0],
        id_quat(),
    ));
    // Charred void where the engine and grille were torn out.
    prims.push(prim(
        solid(cuboid_tapered([0.3, 0.4, 1.1], 0.0, tarp(CHARRED))),
        [2.0, 0.78, 0.0],
        id_quat(),
    ));
    // Caved-in cabin, sagging and burnt.
    prims.push(prim(
        solid(cuboid_tapered([1.7, 0.65, 1.45], 0.25, rusted(CAR_RUST))),
        [-0.35, 1.15, 0.0],
        id_quat(),
    ));
    // Smashed-out window band (charred glass) on each cabin flank.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.42, 0.06], 0.0, tarp(CHARRED))),
            [-0.35, 1.2, sz * 0.66],
            id_quat(),
        ));
    }
    // Cracked windscreen, raked forward.
    prims.push(prim(
        solid(wedge([1.2, 0.5, 0.5], tarp(CHARRED))),
        [0.45, 1.15, 0.0],
        quat_y(FRAC_PI_2),
    ));
    // Front + rear bumpers, one hanging loose at an angle.
    prims.push(prim(
        solid(cuboid_tapered([0.2, 0.18, 1.7], 0.0, rusted(STEEL_GREY))),
        [1.95, 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.2, 0.18, 1.7], 0.0, rusted(STEEL_GREY))),
        [-1.9, 0.46, 0.1],
        quat_x(0.25),
    ));
    // A driver's door wrenched ajar, hung open toward the camera (−Z).
    prims.push(prim(
        solid(cuboid_tapered([1.1, 0.9, 0.1], 0.0, rusted(CAR_RUST))),
        [-0.5, 0.85, -0.95],
        quat_y(0.55),
    ));

    // Three flat tyres still on their axles (along Z); the front-left is gone.
    for (x, sz) in [(-1.2_f32, -1.0_f32), (-1.2, 1.0), (1.2, -1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.34, 0.3, 8, 0.0, tarp(TIRE_BLACK))),
            [x, 0.3, sz * 0.75],
            quat_x(FRAC_PI_2),
        ));
    }
    // The missing wheel thrown flat on the ground in front of the wreck (−Z).
    prims.push(prim(
        solid(torus(0.13, 0.32, tarp(TIRE_BLACK))),
        [1.9, 0.13, -1.4],
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
        assert_sanitize_stable(&WreckedCar.build(""), "wrecked_car");
    }
}
