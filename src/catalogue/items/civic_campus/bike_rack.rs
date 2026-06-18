//! Bike rack — a Civic/Campus prop. A ground rail of steel inverted-U
//! hoops. Scatter clutter outside the halls and the library.
//!
//! Each hoop is a torus stood upright by a single
//! [`quat_x`] of π/2 (which lays its ring plane across the rail).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STEEL_GREY, steel};

pub struct BikeRack;

impl CatalogueEntry for BikeRack {
    fn slug(&self) -> &'static str {
        "bike_rack"
    }
    fn name(&self) -> &'static str {
        "Bike Rack"
    }
    fn description(&self) -> &'static str {
        "Steel ground rail of inverted-U bike hoops."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Steel ground rail — the root.
        prim(
            solid(cuboid_tapered([3.0, 0.1, 0.15], 0.0, steel(STEEL_GREY))),
            [0.0, 0.05, 0.0],
            id_quat(),
        ),
    ];

    // Four upright hoops along the rail.
    for x in [-1.2_f32, -0.4, 0.4, 1.2] {
        prims.push(prim(
            solid(torus(0.04, 0.4, steel(STEEL_GREY))),
            [x, 0.45, 0.0],
            quat_x(FRAC_PI_2),
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
        assert_sanitize_stable(&BikeRack.build(""), "bike_rack");
    }
}
