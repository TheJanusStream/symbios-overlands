//! Tyre wall — a Post-apocalyptic prop. A rampart of stacked, half-buried
//! tyres packed into a low barrier. Scatter clutter shoring up the holdout.

use crate::catalogue::items::util::{assemble, id_quat, prim, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{TIRE_BLACK, tarp};

pub struct TireWall;

impl CatalogueEntry for TireWall {
    fn slug(&self) -> &'static str {
        "tire_wall"
    }
    fn name(&self) -> &'static str {
        "Tyre Wall"
    }
    fn description(&self) -> &'static str {
        "Rampart of stacked, half-buried tyres packed into a low barrier."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // First tyre — the root, lying flat at one end of the bottom row.
        prim(
            solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
            [-0.9, 0.18, 0.0],
            id_quat(),
        ),
    ];

    // Bottom row.
    for x in [0.0_f32, 0.9] {
        prims.push(prim(
            solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
            [x, 0.18, 0.0],
            id_quat(),
        ));
    }
    // Middle row, offset into the gaps.
    for x in [-0.45_f32, 0.45] {
        prims.push(prim(
            solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
            [x, 0.5, 0.0],
            id_quat(),
        ));
    }
    // Capping tyre.
    prims.push(prim(
        solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
        [0.0, 0.82, 0.0],
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
        assert_sanitize_stable(&TireWall.build(""), "tire_wall");
    }
}
