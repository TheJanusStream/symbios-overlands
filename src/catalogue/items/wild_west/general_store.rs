//! General store — a Wild-West secondary. A clapboard store with a false
//! front, a covered boardwalk porch, a lit window and goods stacked out front.
//! Its window is emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAP_TAN, CLAP_WHITE, GLASS_WARM, TIN_GREY, WOOD_RAW, clapboard, glass, tin};

pub struct GeneralStore;

impl CatalogueEntry for GeneralStore {
    fn slug(&self) -> &'static str {
        "general_store"
    }
    fn name(&self) -> &'static str {
        "General Store"
    }
    fn description(&self) -> &'static str {
        "Clapboard store with a false front, a boardwalk porch, a lit window and stacked goods."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_w = 6.0_f32;
    let body_h = 4.0_f32;
    let body_d = 6.0_f32;
    let body_top = slab_h + body_h;
    // Render FRONT = −Z — the shopfront, sign and porch all face −Z.
    let front_z = -body_d * 0.5;

    let mut prims = vec![
        // Clapboard slab — the root.
        prim(
            solid(cuboid_tapered([7.0, slab_h, 7.0], 0.0, clapboard(WOOD_RAW))),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Clapboard body.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w, body_h, body_d],
            0.0,
            clapboard(CLAP_TAN),
        )),
        [0.0, slab_h + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // White corner pilasters.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.28, body_h, 0.28],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [
                sx * (body_w * 0.5 - 0.04),
                slab_h + body_h * 0.5,
                front_z + 0.05,
            ],
            id_quat(),
        ));
    }
    // Tin roof tucked behind the false front.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.4, 0.4, body_d + 0.4],
            0.0,
            tin(TIN_GREY),
        )),
        [0.0, body_top + 0.2, 0.0],
        id_quat(),
    ));

    // Tall stepped false front with an overhanging cornice + sign band.
    let ff_z = front_z - 0.2;
    let ff_face = ff_z - 0.2;
    let ff_h = 6.3_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.6, ff_h, 0.4],
            0.0,
            clapboard(CLAP_TAN),
        )),
        [0.0, slab_h + ff_h * 0.5, ff_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 1.0, 0.32, 0.75],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + ff_h + 0.14, ff_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [4.7, 0.95, 0.16],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + 4.9, ff_face - 0.08],
        id_quat(),
    ));

    // Glazed shopfront: a wide lit display window beside a glazed door + transom.
    prims.push(prim(
        solid(cuboid_tapered([3.5, 2.1, 0.08], 0.0, clapboard(CLAP_WHITE))),
        [0.85, slab_h + 1.55, front_z - 0.02],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.2, 1.8, 0.12], 0.0, glass(GLASS_WARM, 3.2)),
        [0.85, slab_h + 1.6, front_z - 0.06],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [1.1, 2.2, 0.12],
            0.0,
            clapboard([0.34, 0.24, 0.14]),
        )),
        [-1.95, slab_h + 1.1, front_z - 0.06],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.0, 0.5, 0.1], 0.0, glass(GLASS_WARM, 2.7)),
        [-1.95, slab_h + 2.5, front_z - 0.08],
        id_quat(),
    ));

    // Covered boardwalk porch — flat awning on posts over a raised walk.
    let porch_y = slab_h + 2.7;
    let porch_front = front_z - 1.9;
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.6, 0.18, 2.0],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, porch_y, front_z - 0.9],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.2, porch_y, 0.2],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [sx * 2.9, porch_y * 0.5, porch_front + 0.1],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.6, 0.16, 1.0],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, slab_h + 0.08, porch_front + 0.4],
        id_quat(),
    ));

    // Goods stacked out front: two barrels and a crate.
    for bx in [-2.5_f32, 2.6] {
        prims.push(prim(
            solid(cylinder_tapered(0.38, 0.9, 12, 0.08, clapboard(WOOD_RAW))),
            [bx, slab_h + 0.45, porch_front + 0.3],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.7, 0.7], 0.0, clapboard(CLAP_TAN))),
        [2.0, slab_h + 0.35, porch_front + 0.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.55, 0.6], 0.0, clapboard(WOOD_RAW))),
        [2.1, slab_h + 0.92, porch_front + 0.25],
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
        assert_sanitize_stable(&GeneralStore.build(""), "general_store");
    }
}
