//! Solar array — a Space-Outpost secondary. A row of large tilted PV panels
//! on a steel torque-tube frame. The power farm of the base.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base frame.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{HULL_PANEL, PV_BLUE, STATUS_GREEN, STEEL_DARK, hull, pv, pv_panel, steel};

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

    // Three large framed PV panels, cell grids facing the camera (−Z).
    for x in [-2.8_f32, 0.0, 2.8] {
        let mut panel = pv_panel(2.6, 3.0, pv(PV_BLUE), steel(STEEL_DARK));
        panel.transform.translation = Fp3([x, 1.55, 0.0]);
        panel.transform.rotation = quat_x(-0.5);
        prims.push(panel);
    }

    // Combiner box at the array foot with a green status LED — emissive.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.8, 0.45], 0.0, hull(HULL_PANEL))),
        [4.0, 0.4, 1.3],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.08, 4, glow(STATUS_GREEN, 2.0)),
        [4.0, 0.6, 1.55],
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
        assert_sanitize_stable(&SolarArray.build(""), "solar_array");
    }
}
