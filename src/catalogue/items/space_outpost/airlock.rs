//! Airlock — a Space-Outpost prop. A standalone pressure-lock chamber with a
//! lit hatch port and hazard banding. Scatter clutter linking the modules.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HAZARD_YELLOW, HULL_WHITE, VIEWPORT_LIT, hull, painted};

pub struct Airlock;

impl CatalogueEntry for Airlock {
    fn slug(&self) -> &'static str {
        "airlock"
    }
    fn name(&self) -> &'static str {
        "Airlock"
    }
    fn description(&self) -> &'static str {
        "Standalone pressure-lock chamber with a lit hatch port and hazard banding."
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
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Hull chamber — the root.
        prim(
            solid(cylinder_tapered(1.2, 2.2, 16, 0.0, hull(HULL_WHITE))),
            [0.0, 1.1, 0.0],
            id_quat(),
        ),
    ];

    // Hazard band around the chamber.
    prims.push(prim(
        solid(cylinder_tapered(1.24, 0.3, 16, 0.0, painted(HAZARD_YELLOW))),
        [0.0, 1.8, 0.0],
        id_quat(),
    ));

    // Hatch frame + lit port on the +Z face.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 1.6, 0.2], 0.0, hull(HULL_WHITE))),
        [0.0, 0.9, 1.15],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.5, 0.15], 0.0, glow(VIEWPORT_LIT, 1.8)),
        [0.0, 1.2, 1.28],
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
        assert_sanitize_stable(&Airlock.build(""), "airlock");
    }
}
