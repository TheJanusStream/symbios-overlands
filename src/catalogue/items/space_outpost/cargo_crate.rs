//! Cargo crate — a Space-Outpost prop. A stack of hull supply containers with
//! hazard stencils. Scatter clutter of the base's stores.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, hull, painted};

pub struct CargoCrate;

impl CatalogueEntry for CargoCrate {
    fn slug(&self) -> &'static str {
        "cargo_crate"
    }
    fn name(&self) -> &'static str {
        "Cargo Crate"
    }
    fn description(&self) -> &'static str {
        "Stack of hull supply containers with hazard stencils."
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
            clearance: 1.2,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Large base crate — the root.
        prim(
            solid(cuboid_tapered([1.2, 1.0, 1.2], 0.0, hull(HULL_PANEL))),
            [0.0, 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Hazard stripe on the base crate.
    prims.push(prim(
        cuboid_tapered([1.22, 0.2, 1.22], 0.0, painted(HAZARD_YELLOW)),
        [0.0, 0.75, 0.0],
        id_quat(),
    ));

    // A second crate alongside.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.9, 1.0], 0.0, hull(HULL_WHITE))),
        [1.15, 0.45, 0.1],
        id_quat(),
    ));
    // A smaller crate stacked on top.
    prims.push(prim(
        solid(cuboid_tapered([0.8, 0.7, 0.8], 0.0, hull(HULL_PANEL))),
        [0.05, 1.35, 0.1],
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
        assert_sanitize_stable(&CargoCrate.build(""), "cargo_crate");
    }
}
