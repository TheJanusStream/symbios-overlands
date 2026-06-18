//! Traffic cone — a Roadside prop. An orange enamel cone with a reflective
//! white band on a square base. The smallest scatter clutter of the strip.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONE_ORANGE, SIGN_WHITE, enamel};

pub struct TrafficCone;

impl CatalogueEntry for TrafficCone {
    fn slug(&self) -> &'static str {
        "traffic_cone"
    }
    fn name(&self) -> &'static str {
        "Traffic Cone"
    }
    fn description(&self) -> &'static str {
        "Orange enamel cone with a reflective white band on a square base."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.5,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Square enamel base — the root.
        prim(
            solid(cuboid_tapered([0.42, 0.08, 0.42], 0.0, enamel(CONE_ORANGE))),
            [0.0, 0.04, 0.0],
            id_quat(),
        ),
        // Orange cone.
        prim(
            solid(cone(0.18, 0.7, 12, enamel(CONE_ORANGE))),
            [0.0, 0.45, 0.0],
            id_quat(),
        ),
        // Reflective white band.
        prim(
            solid(cylinder_tapered(0.14, 0.12, 12, 0.2, enamel(SIGN_WHITE))),
            [0.0, 0.42, 0.0],
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
        assert_sanitize_stable(&TrafficCone.build(""), "traffic_cone");
    }
}
