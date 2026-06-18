//! Ash pit — a Post-apocalyptic *poor* prop. A cold, dead fire pit ringed with
//! stones, heaped with grey ash, charred wood and a few bones. The spent
//! hearth of the drifter's camp.
//!
//! A couple of charred logs lie tipped with a [`quat_x`].

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ASH_GREY, PLANK_GREY, STEEL_GREY, plank, rusted, tarp};

/// Pale bone colour.
const BONE: [f32; 3] = [0.78, 0.76, 0.68];

pub struct AshPit;

impl CatalogueEntry for AshPit {
    fn slug(&self) -> &'static str {
        "ash_pit"
    }
    fn name(&self) -> &'static str {
        "Ash Pit"
    }
    fn description(&self) -> &'static str {
        "Cold fire pit ringed with stones, heaped with ash, charred wood and a few bones."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_POOR
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
        // Ash bed — the root.
        prim(
            solid(cylinder_tapered(0.7, 0.1, 14, 0.0, tarp(ASH_GREY))),
            [0.0, 0.05, 0.0],
            id_quat(),
        ),
    ];

    // Ring of stones around the pit.
    for k in 0..7 {
        let a = k as f32 / 7.0 * TAU;
        prims.push(prim(
            solid(cuboid_tapered([0.18, 0.16, 0.18], 0.1, rusted(STEEL_GREY))),
            [a.cos() * 0.7, 0.08, a.sin() * 0.7],
            id_quat(),
        ));
    }

    // Charred logs lying across the ash.
    for (z, tilt) in [(-0.1_f32, 1.4_f32), (0.2, 1.5)] {
        prims.push(prim(
            solid(cylinder_tapered(0.07, 0.8, 6, 0.0, plank(PLANK_GREY))),
            [0.0, 0.14, z],
            quat_x(tilt),
        ));
    }
    // A couple of bones half-buried in the ash.
    prims.push(prim(
        solid(cylinder_tapered(0.04, 0.5, 6, 0.0, tarp(BONE))),
        [0.25, 0.12, -0.2],
        quat_x(1.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&AshPit.build(""), "ash_pit");
    }
}
