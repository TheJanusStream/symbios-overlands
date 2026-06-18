//! Rover — a Space-Outpost prop. A six-wheeled exploration rover with a solar
//! deck, a sensor mast and a whip antenna. Scatter clutter parked around the
//! base.
//!
//! The rover drives along X, so its wheel axles run along Z — each wheel is a
//! cylinder turned by a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HULL_WHITE, PV_BLUE, STEEL_DARK, hull, pv, steel};

pub struct Rover;

impl CatalogueEntry for Rover {
    fn slug(&self) -> &'static str {
        "rover"
    }
    fn name(&self) -> &'static str {
        "Rover"
    }
    fn description(&self) -> &'static str {
        "Six-wheeled exploration rover with a solar deck, sensor mast and antenna."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Hull body — the root.
        prim(
            solid(cuboid_tapered([2.6, 0.7, 1.4], 0.05, hull(HULL_WHITE))),
            [0.0, 0.85, 0.0],
            id_quat(),
        ),
    ];

    // Six wheels (axles along Z).
    for x in [-0.9_f32, 0.0, 0.9] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(0.38, 0.3, 10, 0.0, steel(STEEL_DARK))),
                [x, 0.38, sz * 0.75],
                quat_x(FRAC_PI_2),
            ));
        }
    }

    // Solar deck on the body.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 0.06, 1.1], 0.0, pv(PV_BLUE))),
        [0.0, 1.24, 0.0],
        id_quat(),
    ));

    // Sensor mast + camera head.
    prims.push(prim(
        solid(cylinder_tapered(0.07, 1.0, 6, 0.0, steel(STEEL_DARK))),
        [-0.9, 1.7, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.4, 0.25, 0.3], 0.0, hull(HULL_WHITE))),
        [-0.9, 2.3, 0.0],
        id_quat(),
    ));
    // Whip antenna.
    prims.push(prim(
        solid(cylinder_tapered(0.03, 1.4, 4, 0.0, steel(STEEL_DARK))),
        [1.0, 1.8, 0.4],
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
        assert_sanitize_stable(&Rover.build(""), "rover");
    }
}
