//! Solar wreck — a Space-Outpost *poor* secondary. A collapsed solar array,
//! its steel frame buckled and panels cracked and toppled. The dead power
//! farm of the wreck site.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{PV_BLUE, SCORCH, STEEL_DARK, pv, steel};

pub struct SolarWreck;

impl CatalogueEntry for SolarWreck {
    fn slug(&self) -> &'static str {
        "solar_wreck"
    }
    fn name(&self) -> &'static str {
        "Solar Wreck"
    }
    fn description(&self) -> &'static str {
        "Collapsed solar array, its steel frame buckled and panels cracked and toppled."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Buckled torque tube — the root, leaning.
        prim(
            solid(cuboid_tapered([5.0, 0.18, 0.18], 0.0, steel(STEEL_DARK))),
            [0.0, 0.9, 0.0],
            quat_x(0.2),
        ),
    ];

    // A couple of leaning support posts.
    for x in [-1.8_f32, 1.4] {
        prims.push(prim(
            solid(cuboid_tapered([0.16, 1.6, 0.16], 0.0, steel(STEEL_DARK))),
            [x, 0.8, 0.0],
            quat_x(0.25),
        ));
    }

    // A cracked panel still hanging at an angle.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 0.08, 2.6], 0.0, pv(PV_BLUE))),
        [-0.6, 1.2, 0.2],
        quat_x(0.7),
    ));
    // A panel toppled flat on the ground, scorched.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 0.08, 2.6], 0.0, pv(SCORCH))),
        [2.0, 0.1, 0.6],
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
        assert_sanitize_stable(&SolarWreck.build(""), "solar_wreck");
    }
}
