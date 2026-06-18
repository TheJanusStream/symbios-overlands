//! Wagon — a Wild-West prop. A covered wagon: a plank bed under an arched
//! canvas tilt on iron-tyred wheels. Scatter clutter parked about the town.
//!
//! Wheels run their axles along Z via a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CANVAS_TAN, IRON_DARK, WOOD_RAW, canvas, clapboard, iron};

pub struct Wagon;

impl CatalogueEntry for Wagon {
    fn slug(&self) -> &'static str {
        "wagon"
    }
    fn name(&self) -> &'static str {
        "Wagon"
    }
    fn description(&self) -> &'static str {
        "Covered wagon: a plank bed under an arched canvas tilt on iron-tyred wheels."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Plank bed — the root (running along X).
        prim(
            solid(cuboid_tapered([3.2, 0.5, 1.4], 0.0, clapboard(WOOD_RAW))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
    ];
    // Arched canvas tilt over the bed.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 1.5, 1.5], 0.4, canvas(CANVAS_TAN))),
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    // Four iron-tyred wheels (axles along Z; rear larger than front).
    for (x, rad) in [(-1.2_f32, 0.55_f32), (1.2, 0.4)] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(rad, 0.16, 10, 0.0, clapboard(WOOD_RAW))),
                [x, rad, sz * 0.8],
                quat_x(FRAC_PI_2),
            ));
            prims.push(prim(
                solid(torus(0.05, rad, iron(IRON_DARK))),
                [x, rad, sz * 0.8],
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
        assert_sanitize_stable(&Wagon.build(""), "wagon");
    }
}
