//! Fuel barrels — a Post-apocalyptic prop. A clutch of rusted oil drums, two
//! standing and one toppled, ringed with ribbing. Scatter clutter of the
//! holdout's stores.
//!
//! The toppled drum lies on its side with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{RUST_BROWN, STEEL_GREY, rusted, tarp};

/// Dark spilled-fuel stain colour pooled on the ground.
const SPILL: [f32; 3] = [0.08, 0.07, 0.06];

pub struct FuelBarrels;

impl CatalogueEntry for FuelBarrels {
    fn slug(&self) -> &'static str {
        "fuel_barrels"
    }
    fn name(&self) -> &'static str {
        "Fuel Barrels"
    }
    fn description(&self) -> &'static str {
        "Clutch of rusted oil drums, two standing and one toppled, ringed with ribbing."
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
            clearance: 1.2,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A rusted drum with two ribbing rings about its `axis` quaternion. The rings
/// are children so they roll with the body when it is toppled.
fn drum(pos: [f32; 3], color: [f32; 3], axis: crate::pds::Fp4) -> Generator {
    let mut body = prim(
        solid(cylinder_tapered(0.34, 0.95, 12, 0.0, rusted(color))),
        pos,
        axis,
    );
    for ring_y in [-0.24_f32, 0.24] {
        body.children.push(prim(
            torus(0.03, 0.34, rusted(STEEL_GREY)),
            [0.0, ring_y, 0.0],
            id_quat(),
        ));
    }
    body
}

fn build_tree() -> Generator {
    let mut prims = vec![
        drum([0.0, 0.475, 0.0], RUST_BROWN, id_quat()),
        drum([0.7, 0.475, 0.2], STEEL_GREY, id_quat()),
    ];

    // One drum toppled on its side, laid along X so the camera (−Z) sees it
    // rolled over in profile rather than end-on, leaking a dark fuel stain.
    prims.push(drum([-0.6, 0.34, -0.5], RUST_BROWN, quat_z(FRAC_PI_2)));
    prims.push(prim(
        solid(cylinder_tapered(0.55, 0.02, 16, 0.0, tarp(SPILL))),
        [-0.95, 0.012, -0.5],
        id_quat(),
    ));

    // A dented jerry can stood beside the cluster.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.32, 0.46, 0.2],
            0.0,
            rusted([0.30, 0.36, 0.30]),
        )),
        [0.5, 0.23, -0.55],
        quat_y(0.4),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.05, 0.1, 6, 0.0, rusted(STEEL_GREY))),
        [0.5, 0.5, -0.62],
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
        assert_sanitize_stable(&FuelBarrels.build(""), "fuel_barrels");
    }
}
