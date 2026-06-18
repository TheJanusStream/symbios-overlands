//! Solar array — a Space-Outpost secondary. A row of large tilted PV panels
//! on a steel torque-tube frame. The power farm of the base.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base frame.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{PV_BLUE, STEEL_DARK, pv, steel};

pub struct SolarArray;

impl CatalogueEntry for SolarArray {
    fn slug(&self) -> &'static str {
        "solar_array"
    }
    fn name(&self) -> &'static str {
        "Solar Array"
    }
    fn description(&self) -> &'static str {
        "Row of large tilted PV panels on a steel torque-tube frame."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Steel base frame — the root.
        prim(
            solid(cuboid_tapered([9.0, 0.3, 3.5], 0.0, steel(STEEL_DARK))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Horizontal torque tube on short posts.
    prims.push(prim(
        solid(cuboid_tapered([8.6, 0.2, 0.2], 0.0, steel(STEEL_DARK))),
        [0.0, 1.2, 0.0],
        id_quat(),
    ));
    for x in [-3.6_f32, 0.0, 3.6] {
        prims.push(prim(
            solid(cuboid_tapered([0.18, 1.2, 0.18], 0.0, steel(STEEL_DARK))),
            [x, 0.6, 0.0],
            id_quat(),
        ));
    }

    // Three large tilted PV panels along the tube.
    for x in [-2.8_f32, 0.0, 2.8] {
        prims.push(prim(
            solid(cuboid_tapered([2.6, 0.1, 3.0], 0.0, pv(PV_BLUE))),
            [x, 1.5, 0.0],
            quat_x(0.5),
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
        assert_sanitize_stable(&SolarArray.build(""), "solar_array");
    }
}
