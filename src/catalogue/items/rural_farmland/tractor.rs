//! Tractor — a Rural/Farmland prop. A classic farm tractor: a green body with
//! a hood and exhaust stack, a seat and steering wheel, big rear wheels and
//! small steers, parked in the yard.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{TRACTOR_GREEN, TRACTOR_YELLOW, enamel};

/// Tyre black.
const TIRE: [f32; 3] = [0.08, 0.08, 0.09];

pub struct Tractor;

impl CatalogueEntry for Tractor {
    fn slug(&self) -> &'static str {
        "tractor"
    }
    fn name(&self) -> &'static str {
        "Tractor"
    }
    fn description(&self) -> &'static str {
        "Classic green farm tractor with big rear wheels and an exhaust stack."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
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
        // Chassis — the root.
        prim(
            solid(cuboid_tapered([3.0, 0.5, 0.9], 0.05, enamel(TRACTOR_GREEN))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
        // Hood over the engine.
        prim(
            solid(cuboid_tapered([1.5, 0.8, 0.95], 0.1, enamel(TRACTOR_GREEN))),
            [0.85, 1.25, 0.0],
            id_quat(),
        ),
    ];

    // Big rear wheels and small front wheels (blocks, read as tyres).
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.42, 1.7, 1.7], 0.0, enamel(TIRE))),
            [-0.9, 0.85, sz * 0.78],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.3, 0.8, 0.8], 0.0, enamel(TIRE))),
            [1.35, 0.4, sz * 0.5],
            id_quat(),
        ));
    }
    // Yellow wheel hubs on the rear.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.44, 0.5, 0.5], 0.0, enamel(TRACTOR_YELLOW)),
            [-0.9, 0.85, sz * 0.78],
            id_quat(),
        ));
    }

    // Seat and steering wheel.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.15, 0.5],
            0.0,
            enamel([0.2, 0.2, 0.22]),
        )),
        [-0.45, 1.35, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.5, 0.1],
            0.0,
            enamel([0.2, 0.2, 0.22]),
        )),
        [-0.7, 1.6, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.05, 0.22, enamel([0.2, 0.2, 0.22])),
        [-0.1, 1.55, 0.0],
        quat_x(0.5),
    ));

    // Exhaust stack.
    prims.push(prim(
        solid(cylinder_tapered(
            0.06,
            0.9,
            8,
            0.0,
            enamel([0.3, 0.3, 0.32]),
        )),
        [1.2, 1.95, 0.0],
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
        assert_sanitize_stable(&Tractor.build(""), "tractor");
    }
}
