//! Beacon — a Space-Outpost prop. A landing beacon: a steel mast topped by a
//! glowing red light with a small solar cell. Scatter clutter marking the
//! base perimeter; its light is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BEACON_RED, PV_BLUE, STEEL_DARK, pv, steel};

pub struct Beacon;

impl CatalogueEntry for Beacon {
    fn slug(&self) -> &'static str {
        "beacon"
    }
    fn name(&self) -> &'static str {
        "Beacon"
    }
    fn description(&self) -> &'static str {
        "Steel mast topped by a glowing red light with a small solar cell."
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
            clearance: 0.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Steel foot — the root.
        prim(
            solid(cuboid_tapered([0.5, 0.2, 0.5], 0.0, steel(STEEL_DARK))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
        // Mast.
        prim(
            solid(cylinder_tapered(0.1, 2.2, 6, 0.05, steel(STEEL_DARK))),
            [0.0, 1.2, 0.0],
            id_quat(),
        ),
        // Solar cell on the mast.
        prim(
            solid(cuboid_tapered([0.5, 0.05, 0.4], 0.0, pv(PV_BLUE))),
            [0.0, 2.0, 0.0],
            id_quat(),
        ),
        // Glowing red light at the top — emissive trim.
        prim(
            sphere(0.2, 3, glow(BEACON_RED, 3.0)),
            [0.0, 2.4, 0.0],
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
        assert_sanitize_stable(&Beacon.build(""), "beacon");
    }
}
