//! Survivor lean-to — the Post-apocalyptic *poor* landmark. A desperate tarp-
//! and-scrap lean-to propped against a heap of rubble, a bedroll and a cold
//! fire ring beneath it. The drifter counterpart to the
//! [`fortified_ruin`](super::fortified_ruin): same wasteland, opposite end of
//! the prosperity axis (`Poor`), so a destitute room grows the lone hovel
//! instead of the defended holdout.
//!
//! The tarp and props lean with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ASH_GREY, CONCRETE_GREY, CORRUGATED_RUST, PLANK_GREY, RUST_BROWN, STEEL_GREY, TARP_FADED,
    concrete, plank, rubble_chunks, rusted, sheet, tarp,
};

pub struct SurvivorLeanTo;

impl CatalogueEntry for SurvivorLeanTo {
    fn slug(&self) -> &'static str {
        "survivor_lean_to"
    }
    fn name(&self) -> &'static str {
        "Survivor Lean-To"
    }
    fn description(&self) -> &'static str {
        "Desperate tarp-and-scrap lean-to against a rubble heap, a bedroll and cold fire ring."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Rubble heap it leans against — the root.
        prim(
            solid(cuboid_tapered(
                [2.6, 1.8, 2.2],
                0.4,
                concrete(CONCRETE_GREY),
            )),
            [-1.6, 0.9, 0.0],
            id_quat(),
        ),
    ];
    // A couple of broken concrete chunks + crumbled debris around the heap.
    for (cx, cz, s) in [(-2.4_f32, 1.0_f32, 0.8_f32), (-0.8, -1.0, 0.7)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [s, s * 0.8, s],
                0.3,
                concrete(CONCRETE_GREY),
            )),
            [cx, s * 0.4, cz],
            id_quat(),
        ));
    }
    prims.extend(rubble_chunks([-2.2, 0.0, -0.6], 1.1, 0.5, 4));

    // Two lean poles.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.08, 2.2, 6, 0.0, plank(PLANK_GREY))),
            [0.6, 1.0, sz * 0.9],
            quat_x(0.5),
        ));
    }
    // Stretched tarp sloping from the rubble down to the poles.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 0.1, 2.4], 0.0, tarp(TARP_FADED))),
        [-0.2, 1.5, 0.0],
        quat_x(0.5),
    ));
    // A salvaged corrugated-sheet panel patched over one end of the tarp,
    // lashed at a slightly steeper pitch — the desperate make-do roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.1, 0.06, 2.0],
            0.0,
            sheet(CORRUGATED_RUST),
        )),
        [-1.1, 1.78, 1.0],
        quat_x(0.62),
    ));

    // Bedroll under the shelter.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.7, 0.2, 1.6],
            0.1,
            tarp([0.34, 0.30, 0.26]),
        )),
        [-0.2, 0.15, 0.0],
        id_quat(),
    ));
    // A scavenged jerry can and a battered crate of belongings beside it.
    prims.push(prim(
        solid(cuboid_tapered([0.34, 0.5, 0.22], 0.0, rusted(RUST_BROWN))),
        [0.55, 0.25, -0.8],
        quat_y(0.3),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.4, 0.5], 0.05, plank(PLANK_GREY))),
        [0.5, 0.2, 0.9],
        quat_y(-0.2),
    ));
    // A billy-can propped on a stone at the fire ring.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 0.18, 8, 0.0, rusted(STEEL_GREY))),
        [1.05, 0.21, 0.35],
        id_quat(),
    ));
    // Cold fire ring of stones + ash.
    prims.push(prim(
        solid(cylinder_tapered(0.5, 0.12, 12, 0.0, tarp(ASH_GREY))),
        [1.2, 0.06, 0.0],
        id_quat(),
    ));
    for k in 0..5 {
        let a = k as f32 / 5.0 * std::f32::consts::TAU;
        prims.push(prim(
            solid(cuboid_tapered(
                [0.16, 0.16, 0.16],
                0.1,
                rusted([0.4, 0.4, 0.42]),
            )),
            [1.2 + a.cos() * 0.5, 0.1, a.sin() * 0.5],
            id_quat(),
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
        assert_sanitize_stable(&SurvivorLeanTo.build(""), "survivor_lean_to");
    }
}
