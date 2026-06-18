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
    let body_h = 4.0_f32;
    let body_d = 6.0_f32;
    let body_top = slab_h + body_h;
    let front_z = body_d * 0.5;

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
            [6.0, body_h, body_d],
            0.0,
            clapboard(CLAP_TAN),
        )),
        [0.0, slab_h + body_h * 0.5, -0.3],
        id_quat(),
    ));
    // Tin roof + tall false front.
    prims.push(prim(
        solid(cuboid_tapered([6.4, 0.4, body_d + 0.4], 0.0, tin(TIN_GREY))),
        [0.0, body_top + 0.2, -0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([6.4, 5.4, 0.4], 0.0, clapboard(CLAP_TAN))),
        [0.0, slab_h + 2.7, front_z + 0.1],
        id_quat(),
    ));

    // Lit shopfront window + door.
    prims.push(prim(
        cuboid_tapered([3.0, 1.6, 0.15], 0.0, glass(GLASS_WARM, 1.6)),
        [1.2, slab_h + 1.5, front_z + 0.06],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.0, 2.1, 0.15], 0.0, clapboard(WOOD_RAW))),
        [-1.8, slab_h + 1.05, front_z + 0.06],
        id_quat(),
    ));
    // Sign band on the false front.
    prims.push(prim(
        solid(cuboid_tapered([4.5, 0.7, 0.12], 0.0, clapboard(CLAP_WHITE))),
        [0.0, slab_h + 4.4, front_z + 0.14],
        id_quat(),
    ));

    // Covered boardwalk porch.
    prims.push(prim(
        solid(cuboid_tapered([6.4, 0.2, 2.2], 0.0, clapboard(WOOD_RAW))),
        [0.0, slab_h + 2.7, front_z + 1.2],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.18, 2.6, 0.18], 0.0, clapboard(WOOD_RAW))),
            [sx * 2.8, slab_h + 1.35, front_z + 2.1],
            id_quat(),
        ));
    }
    // A couple of barrels of goods out front.
    for bx in [-2.4_f32, 2.4] {
        prims.push(prim(
            solid(cylinder_tapered(0.4, 0.9, 10, 0.08, clapboard(WOOD_RAW))),
            [bx, slab_h + 0.45, front_z + 1.6],
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
        assert_sanitize_stable(&GeneralStore.build(""), "general_store");
    }
}
