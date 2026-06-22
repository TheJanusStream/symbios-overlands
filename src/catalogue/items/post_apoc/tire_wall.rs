//! Tyre wall — a Post-apocalyptic prop. A rampart of stacked, half-buried
//! tyres packed into a low barrier. Scatter clutter shoring up the holdout.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, id_quat, prim, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ASH_GREY, STEEL_GREY, TIRE_BLACK, rusted, tarp, tyre_stack};

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
        // First tyre — the root, lying flat at the left end of the bottom row.
        prim(
            solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
            [-1.35, 0.18, 0.0],
            id_quat(),
        ),
    ];
    // Dirt rammed into the root tyre's bore — the packed-out barrier read.
    prims.push(prim(
        solid(cylinder_tapered(0.3, 0.14, 10, 0.0, tarp(ASH_GREY))),
        [-1.35, 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
        [-1.35, 0.5, 0.0],
        id_quat(),
    ));

    // Three two-high tyre columns packing out the rampart along X, each lower
    // bore rammed with dirt.
    for cx in [-0.45_f32, 0.45, 1.35] {
        prims.extend(tyre_stack([cx, 0.18, 0.0], 0.32));
    }
    // Capping tyres bridging the gaps on the top course, half-buried in dirt.
    for cx in [-0.9_f32, 0.0, 0.9] {
        prims.push(prim(
            solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
            [cx, 0.84, 0.0],
            id_quat(),
        ));
    }
    // A leaning scrap stake driven down through the wall to pin it.
    prims.push(prim(
        solid(cylinder_tapered(0.06, 1.8, 5, 0.0, rusted(STEEL_GREY))),
        [0.9, 0.8, 0.12],
        quat_z(0.22),
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
