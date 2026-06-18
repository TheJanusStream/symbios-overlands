//! Fuel barrels — a Post-apocalyptic prop. A clutch of rusted oil drums, two
//! standing and one toppled, ringed with ribbing. Scatter clutter of the
//! holdout's stores.
//!
//! The toppled drum lies on its side with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{RUST_BROWN, STEEL_GREY, rusted};

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

/// A standing rusted drum with two ribbing rings.
fn drum(pos: [f32; 3], color: [f32; 3]) -> Generator {
    let mut body = prim(
        solid(cylinder_tapered(0.34, 0.95, 12, 0.0, rusted(color))),
        pos,
        id_quat(),
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
        drum([0.0, 0.475, 0.0], RUST_BROWN),
        drum([0.7, 0.475, 0.2], STEEL_GREY),
    ];

    // One drum toppled on its side.
    prims.push(prim(
        solid(cylinder_tapered(0.34, 0.95, 12, 0.0, rusted(RUST_BROWN))),
        [-0.5, 0.34, -0.4],
        quat_x(FRAC_PI_2),
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
