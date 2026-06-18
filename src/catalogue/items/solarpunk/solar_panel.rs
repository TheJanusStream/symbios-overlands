//! Solar panel — a Solarpunk prop. A glossy photovoltaic array tilted to the
//! sun on a steel A-frame. Scatter clutter powering the eco-quarter.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{PV_BLUE, STEEL_GREY, pv, steel};

pub struct SolarPanel;

impl CatalogueEntry for SolarPanel {
    fn slug(&self) -> &'static str {
        "solar_panel"
    }
    fn name(&self) -> &'static str {
        "Solar Panel"
    }
    fn description(&self) -> &'static str {
        "Glossy photovoltaic array tilted to the sun on a steel A-frame."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
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
        // Tilted PV panel — the root.
        prim(
            solid(cuboid_tapered([2.6, 0.1, 1.7], 0.0, pv(PV_BLUE))),
            [0.0, 1.1, 0.0],
            quat_x(0.42),
        ),
    ];

    // Tall back legs.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.08, 1.5, 0.08], 0.0, steel(STEEL_GREY))),
            [sx * 1.1, 0.75, -0.6],
            id_quat(),
        ));
    }
    // Short front legs.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.08, 0.8, 0.08], 0.0, steel(STEEL_GREY))),
            [sx * 1.1, 0.4, 0.6],
            id_quat(),
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
        assert_sanitize_stable(&SolarPanel.build(""), "solar_panel");
    }
}
