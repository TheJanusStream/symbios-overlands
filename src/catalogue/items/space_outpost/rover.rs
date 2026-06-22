//! Rover — a Space-Outpost prop. A six-wheeled exploration rover with a solar
//! deck, a sensor mast and a whip antenna. Scatter clutter parked around the
//! base.
//!
//! The rover drives along X, so its wheel axles run along Z — each wheel is a
//! cylinder turned by a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{HULL_PANEL, HULL_WHITE, PV_BLUE, STEEL_DARK, VIEWPORT_LIT, hull, pv, pv_panel, steel};

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
            solid(cuboid_tapered([2.6, 0.55, 1.5], 0.06, hull(HULL_WHITE))),
            [0.0, 0.85, 0.0],
            id_quat(),
        ),
    ];
    // Raised equipment bay.
    prims.push(prim(
        solid(cuboid_tapered([1.8, 0.4, 1.2], 0.1, hull(HULL_PANEL))),
        [0.2, 1.2, 0.0],
        id_quat(),
    ));
    // Framed solar deck on top.
    let mut deck = pv_panel(2.3, 1.2, pv(PV_BLUE), steel(STEEL_DARK));
    deck.transform.translation = Fp3([0.0, 1.45, 0.0]);
    prims.push(deck);

    // Rocker-bogie suspension bars + six wheels with hub caps (axles along Z).
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([2.5, 0.1, 0.12], 0.0, steel(STEEL_DARK))),
            [0.0, 0.5, sz * 0.72],
            id_quat(),
        ));
        for x in [-0.95_f32, 0.0, 0.95] {
            prims.push(prim(
                solid(cuboid_tapered([0.1, 0.32, 0.1], 0.0, steel(STEEL_DARK))),
                [x, 0.46, sz * 0.72],
                id_quat(),
            ));
            prims.push(prim(
                solid(cylinder_tapered(0.4, 0.34, 12, 0.0, steel(STEEL_DARK))),
                [x, 0.4, sz * 0.8],
                quat_x(FRAC_PI_2),
            ));
            prims.push(prim(
                solid(cylinder_tapered(0.15, 0.4, 8, 0.3, hull(HULL_PANEL))),
                [x, 0.4, sz * 0.8],
                quat_x(FRAC_PI_2),
            ));
        }
    }

    // Sensor mast + camera head with stereo eyes looking out the −Z front.
    prims.push(prim(
        solid(cylinder_tapered(0.07, 1.2, 6, 0.0, steel(STEEL_DARK))),
        [-0.7, 2.0, -0.35],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.55, 0.3, 0.32], 0.0, hull(HULL_WHITE))),
        [-0.7, 2.65, -0.42],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            sphere(0.07, 4, glow(VIEWPORT_LIT, 2.2)),
            [-0.7 + sx * 0.14, 2.65, -0.59],
            id_quat(),
        ));
    }

    // Folded robotic sample arm on the −Z front.
    prims.push(prim(
        solid(cuboid_tapered([0.12, 0.12, 0.7], 0.0, steel(STEEL_DARK))),
        [0.7, 1.05, -0.85],
        quat_x(0.7),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.1, 0.55], 0.0, steel(STEEL_DARK))),
        [0.7, 0.72, -1.2],
        quat_x(-0.4),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.12, 0.2, 8, 0.5, hull(HULL_PANEL))),
        [0.7, 0.55, -1.36],
        quat_x(FRAC_PI_2),
    ));

    // Finned RTG + whip antenna at the +X stern.
    prims.push(prim(
        solid(cylinder_tapered(0.17, 0.55, 8, 0.0, steel(STEEL_DARK))),
        [1.25, 1.35, 0.45],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.025, 1.4, 4, 0.0, steel(STEEL_DARK))),
        [1.15, 1.95, 0.55],
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
