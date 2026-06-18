//! Wind turbine — a Solarpunk secondary. A tall white tower with a nacelle
//! and a three-bladed rotor. The clean-energy mast of the eco-quarter.
//!
//! The rotor turns in the Y-Z plane (nacelle axis along X), so its three
//! blades radiate from the hub at 120° via [`quat_x`] alone — no Z-axis
//! rotation needed.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_PALE, STEEL_GREY, STEEL_WHITE, concrete, steel};

pub struct WindTurbine;

impl CatalogueEntry for WindTurbine {
    fn slug(&self) -> &'static str {
        "wind_turbine"
    }
    fn name(&self) -> &'static str {
        "Wind Turbine"
    }
    fn description(&self) -> &'static str {
        "Tall white tower with a nacelle and a three-bladed rotor."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 46.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.5_f32;
    let tower_h = 14.0_f32;
    let hub_y = base_h + tower_h;

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [2.0, base_h, 2.0],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // White tapering tower.
    prims.push(prim(
        solid(cylinder_tapered(0.6, tower_h, 12, 0.4, steel(STEEL_WHITE))),
        [0.0, base_h + tower_h * 0.5, 0.0],
        id_quat(),
    ));
    // Nacelle, axis along X.
    prims.push(prim(
        solid(cuboid_tapered([1.8, 0.9, 0.9], 0.1, steel(STEEL_WHITE))),
        [0.0, hub_y, 0.0],
        id_quat(),
    ));
    // Hub at the front of the nacelle.
    let hub = [1.0_f32, hub_y, 0.0];
    prims.push(prim(
        solid(sphere(0.35, 3, steel(STEEL_GREY))),
        hub,
        id_quat(),
    ));

    // Three blades radiating from the hub at 120° around the X axis.
    let blade_len = 5.5_f32;
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU;
        let c = blade_len * 0.5 + 0.4;
        prims.push(prim(
            solid(cuboid_tapered(
                [0.18, blade_len, 0.5],
                0.6,
                steel(STEEL_WHITE),
            )),
            [hub[0], hub[1] + a.cos() * c, hub[2] + a.sin() * c],
            quat_x(a),
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
        assert_sanitize_stable(&WindTurbine.build(""), "wind_turbine");
    }
}
