//! Buoy — a Coastal-Resort prop. A red-and-white channel marker: a conical
//! enamel float with a painted band, a short topmast and a steel cage ball,
//! beached on a patch of sand.

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{AWNING_WHITE, BUOY_RED, SAND_TAN, STEEL_GREY, enamel, sand, steel};

/// Green starboard-hand navigation light atop the marker.
const NAV_GREEN: [f32; 3] = [0.30, 0.95, 0.45];

pub struct Buoy;

impl CatalogueEntry for Buoy {
    fn slug(&self) -> &'static str {
        "buoy"
    }
    fn name(&self) -> &'static str {
        "Buoy"
    }
    fn description(&self) -> &'static str {
        "Red-and-white conical channel marker with a topmast and cage ball."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Sand patch — the root.
        prim(
            solid(cylinder_tapered(0.8, 0.08, 16, 0.0, sand(SAND_TAN))),
            [0.0, 0.04, 0.0],
            id_quat(),
        ),
        // Base collar where the float meets the sand.
        prim(
            solid(cylinder_tapered(0.56, 0.14, 14, 0.0, enamel(AWNING_WHITE))),
            [0.0, 0.12, 0.0],
            id_quat(),
        ),
        // Conical enamel float.
        prim(
            solid(cone(0.5, 1.2, 14, enamel(BUOY_RED))),
            [0.0, 0.6, 0.0],
            id_quat(),
        ),
        // Painted white band around the float.
        prim(
            solid(cylinder_tapered(0.43, 0.26, 14, 0.0, enamel(AWNING_WHITE))),
            [0.0, 0.5, 0.0],
            id_quat(),
        ),
        // Steel topmast.
        prim(
            solid(cylinder_tapered(0.04, 0.8, 6, 0.0, steel(STEEL_GREY))),
            [0.0, 1.55, 0.0],
            id_quat(),
        ),
        // Cage-ball topmark.
        prim(
            solid(sphere(0.16, 3, steel(STEEL_GREY))),
            [0.0, 2.1, 0.0],
            id_quat(),
        ),
        // Glowing green navigation light below the topmark.
        prim(
            sphere(0.1, 4, glow(NAV_GREEN, 2.6)),
            [0.0, 1.78, 0.0],
            id_quat(),
        ),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Buoy.build(""), "buoy");
    }
}
