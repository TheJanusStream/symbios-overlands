//! Tendril — an Alien-Organic prop. A thick flesh tendril coiling up out of
//! the creep, lesser feelers branching off it. Scatter clutter writhing across
//! the colony floor.
//!
//! Segments lean with a [`quat_x`] to give the tendril its coil.

use crate::catalogue::items::util::{assemble, cylinder_tapered, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{FLESH_PINK, FLESH_RED, flesh};

pub struct Tendril;

impl CatalogueEntry for Tendril {
    fn slug(&self) -> &'static str {
        "tendril"
    }
    fn name(&self) -> &'static str {
        "Tendril"
    }
    fn description(&self) -> &'static str {
        "Thick flesh tendril coiling up from the creep, lesser feelers branching off."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Root segment — rising and curling.
        prim(
            solid(cylinder_tapered(0.28, 1.4, 6, 0.25, flesh(FLESH_RED))),
            [0.0, 0.7, 0.0],
            quat_x(0.3),
        ),
    ];
    // Upper segments curling further over.
    prims.push(prim(
        solid(cylinder_tapered(0.2, 1.2, 6, 0.3, flesh(FLESH_RED))),
        [0.0, 1.7, 0.45],
        quat_x(0.8),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.13, 0.9, 6, 0.5, flesh(FLESH_PINK))),
        [0.0, 2.2, 1.1],
        quat_x(1.3),
    ));

    // Two lesser feelers branching off.
    for (z, tilt) in [(0.2_f32, -0.6_f32), (0.6, 0.9)] {
        prims.push(prim(
            solid(cylinder_tapered(0.08, 0.8, 5, 0.5, flesh(FLESH_PINK))),
            [0.0, 1.2, z],
            quat_x(tilt),
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
        assert_sanitize_stable(&Tendril.build(""), "tendril");
    }
}
