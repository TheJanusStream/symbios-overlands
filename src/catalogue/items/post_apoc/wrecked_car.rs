//! Wrecked car — a Post-apocalyptic prop. A rusted, stripped, burnt-out car
//! body sagging on flat tyres. Scatter clutter strewn across the wasteland.
//!
//! Wheels run their axles along Z via a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CAR_RUST, TIRE_BLACK, rusted, tarp};

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
        // Lower body — the root.
        prim(
            solid(cuboid_tapered([3.6, 0.7, 1.6], 0.05, rusted(CAR_RUST))),
            [0.0, 0.6, 0.0],
            id_quat(),
        ),
    ];
    // Caved-in cabin.
    prims.push(prim(
        solid(cuboid_tapered([1.8, 0.7, 1.4], 0.3, rusted(CAR_RUST))),
        [-0.2, 1.2, 0.0],
        id_quat(),
    ));

    // Four flat tyres (axles along Z).
    for x in [-1.2_f32, 1.2] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(0.34, 0.3, 8, 0.0, tarp(TIRE_BLACK))),
                [x, 0.3, sz * 0.75],
                quat_x(FRAC_PI_2),
            ));
        }
    }

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
