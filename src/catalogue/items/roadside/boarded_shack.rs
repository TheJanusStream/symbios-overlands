//! Boarded shack — a Roadside *poor* secondary. A shuttered clapboard
//! roadside store, its door and window planked over, under a sagging rusted
//! roof with a faded sign still nailed up. The failed business of the
//! busted shoulder.

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, DRIFT_GREY, PLANK_WOOD, RUST_BROWN, STEEL_GREY, concrete, corrugated, plank,
    steel,
};

pub struct BoardedShack;

impl CatalogueEntry for BoardedShack {
    fn slug(&self) -> &'static str {
        "boarded_shack"
    }
    fn name(&self) -> &'static str {
        "Boarded Shack"
    }
    fn description(&self) -> &'static str {
        "Shuttered clapboard store, door and window planked over under a rusted roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let wall_h = 2.6_f32;
    let wall_y = slab_h + wall_h * 0.5;
    let wall_top = slab_h + wall_h;
    let front = -1.75_f32; // boarded −Z (camera) wall face

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [5.0, slab_h, 4.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Weathered clapboard walls.
    prims.push(prim(
        solid(cuboid_tapered([4.5, wall_h, 3.5], 0.0, plank(DRIFT_GREY))),
        [0.0, wall_y, 0.0],
        id_quat(),
    ));

    // Boarded-over door (three slats) on the −Z front.
    for y in [slab_h + 0.6, slab_h + 1.25, slab_h + 1.9] {
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.26, 0.1], 0.0, plank(PLANK_WOOD))),
            [-1.1, y, front - 0.06],
            id_quat(),
        ));
    }
    // Boarded-over window beside it (two slats).
    for y in [slab_h + 1.3, slab_h + 1.75] {
        prims.push(prim(
            solid(cuboid_tapered([1.3, 0.24, 0.1], 0.0, plank(PLANK_WOOD))),
            [1.1, y, front - 0.06],
            id_quat(),
        ));
    }

    // Faded sign nailed above the boards, a painted patch peeling on it.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 0.6, 0.1], 0.0, plank(PLANK_WOOD))),
        [0.0, wall_top - 0.25, front - 0.06],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.9, 0.34, 0.04], 0.0, plank([0.6, 0.55, 0.46])),
        [-0.1, wall_top - 0.25, front - 0.12],
        id_quat(),
    ));

    // Sagging rusted corrugated gable roof.
    prims.push(gable_roof(
        [5.3, 1.3, 4.3],
        [0.0, wall_top + 0.55, 0.0],
        corrugated(RUST_BROWN),
    ));

    // Rusted stovepipe poking through the roof at the +X end, with a cap.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.1, 8, 0.0, corrugated(RUST_BROWN))),
        [1.7, wall_top + 1.2, 0.4],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.18, 0.12, 8, 0.0, corrugated(RUST_BROWN))),
        [1.7, wall_top + 1.8, 0.4],
        id_quat(),
    ));

    // A sagging broken gutter along the −Z eave (one end dropped).
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.1, 0.18], 0.0, steel(STEEL_GREY))),
        [0.0, wall_top - 0.02, front - 0.18],
        quat_z(0.06),
    ));

    // A leaning prop post shoring up the −Z front corner.
    prims.push(prim(
        solid(cuboid_tapered([0.14, 2.4, 0.14], 0.0, plank(PLANK_WOOD))),
        [-2.0, 1.2, front - 0.5],
        quat_z(0.16),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BoardedShack.build(""), "boarded_shack");
    }
}
