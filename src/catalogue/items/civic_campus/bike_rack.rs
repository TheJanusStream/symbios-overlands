//! Bike rack — a Civic/Campus prop. A ground rail of steel inverted-U
//! hoops. Scatter clutter outside the halls and the library.
//!
//! Each hoop is a torus stood upright by a single
//! [`quat_x`] of π/2 (which lays its ring plane across the rail).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STEEL_GREY, painted, steel};

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
    // Bolted ground feet at the rail ends.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.3, 0.06, 0.4], 0.0, steel(STEEL_GREY))),
            [sx * 1.4, 0.03, 0.0],
            id_quat(),
        ));
    }

    // Four upright hoops along the rail.
    for x in [-1.2_f32, -0.4, 0.4, 1.2] {
        prims.push(prim(
            solid(torus(0.04, 0.4, steel(STEEL_GREY))),
            [x, 0.45, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    // A parked bicycle straddling the rail in the middle gap, for life.
    prims.extend(bicycle(0.0));

    assemble(prims)
}

/// A simple parked bicycle centred at `cx`, pointing along Z (its wheels
/// straddle the rail): two upright wheels, a diamond frame, a saddle and
/// handlebars. Returned for the [`assemble`] list (children, never the root).
fn bicycle(cx: f32) -> Vec<Generator> {
    let tyre = || painted([0.09, 0.09, 0.10]);
    let frame = || painted([0.18, 0.34, 0.52]);
    let wheel_r = 0.30_f32;
    let axle_y = wheel_r + 0.02;
    let (rear_z, front_z) = (-0.5_f32, 0.5_f32);
    vec![
        // Wheels (rings standing in the YZ plane so they roll along Z).
        prim(
            solid(torus(0.035, wheel_r, tyre())),
            [cx, axle_y, rear_z],
            quat_z(FRAC_PI_2),
        ),
        prim(
            solid(torus(0.035, wheel_r, tyre())),
            [cx, axle_y, front_z],
            quat_z(FRAC_PI_2),
        ),
        // Seat tube over the rear wheel.
        prim(
            solid(cylinder_tapered(0.025, 0.5, 6, 0.0, frame())),
            [cx, axle_y + 0.25, rear_z + 0.05],
            id_quat(),
        ),
        // Head tube / fork over the front wheel.
        prim(
            solid(cylinder_tapered(0.025, 0.55, 6, 0.0, frame())),
            [cx, axle_y + 0.27, front_z - 0.05],
            id_quat(),
        ),
        // Top tube linking the two.
        prim(
            solid(cuboid_tapered([0.045, 0.045, 1.0], 0.0, frame())),
            [cx, axle_y + 0.46, 0.0],
            id_quat(),
        ),
        // Down tube along the bottom.
        prim(
            solid(cuboid_tapered([0.045, 0.045, 0.95], 0.0, frame())),
            [cx, axle_y + 0.06, 0.0],
            id_quat(),
        ),
        // Saddle.
        prim(
            solid(cuboid_tapered([0.1, 0.06, 0.3], 0.0, tyre())),
            [cx, axle_y + 0.52, rear_z + 0.05],
            id_quat(),
        ),
        // Handlebars.
        prim(
            solid(cuboid_tapered([0.4, 0.04, 0.04], 0.0, tyre())),
            [cx, axle_y + 0.56, front_z - 0.05],
            id_quat(),
        ),
    ]
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
