//! Saloon — the Wild-West landmark and the kit's lit hero. A two-storey red
//! clapboard saloon with a tall false front, a covered porch and balcony, lit
//! amber windows and a hanging sign. ~10 m wide, so it anchors the boomtown
//! and reads as the saloon from across the home region. Its windows are the
//! trim escalation's ruin pass snuffs to a dark, shuttered front.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the floor slab.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, foundation_block, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLAP_RED, CLAP_WHITE, GLASS_WARM, IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, fx, glass, iron,
    tin,
};

pub struct Saloon;

impl CatalogueEntry for Saloon {
    fn slug(&self) -> &'static str {
        "saloon"
    }
    fn name(&self) -> &'static str {
        "Saloon"
    }
    fn description(&self) -> &'static str {
        "Two-storey clapboard saloon with a false front, porch balcony and lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 11.0,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_h = 6.0_f32;
    let body_d = 7.0_f32;
    let body_top = slab_h + body_h;
    let front_z = body_d * 0.5;

    let mut prims = vec![
        // Clapboard floor slab — the root.
        prim(
            solid(cuboid_tapered(
                [10.0, slab_h, 8.0],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(10.0, 8.0, [0.0, 0.0], 1.2));

    // Red clapboard body.
    prims.push(prim(
        solid(cuboid_tapered(
            [8.0, body_h, body_d],
            0.0,
            clapboard(CLAP_RED),
        )),
        [0.0, slab_h + body_h * 0.5, -0.3],
        id_quat(),
    ));
    // Low tin roof.
    prims.push(prim(
        solid(cuboid_tapered([8.4, 0.4, body_d + 0.4], 0.0, tin(TIN_GREY))),
        [0.0, body_top + 0.2, -0.3],
        id_quat(),
    ));
    // Tall false front rising above the roof.
    prims.push(prim(
        solid(cuboid_tapered([8.6, 8.4, 0.4], 0.0, clapboard(CLAP_RED))),
        [0.0, slab_h + 4.2, front_z + 0.1],
        id_quat(),
    ));

    // Lit windows on both floors — emissive.
    for fy in [slab_h + 1.6, slab_h + 4.2] {
        prims.push(prim(
            cuboid_tapered([6.0, 1.4, 0.15], 0.0, glass(GLASS_WARM, 1.7)),
            [0.0, fy, front_z + 0.05],
            id_quat(),
        ));
    }
    // Batwing doors at the centre of the ground floor.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.7, 1.4, 0.15], 0.0, clapboard(WOOD_RAW))),
            [sx * 0.45, slab_h + 0.7, front_z + 0.06],
            id_quat(),
        ));
    }

    // Covered porch: roof slab on posts, with a balcony rail above.
    prims.push(prim(
        solid(cuboid_tapered([8.4, 0.25, 2.4], 0.0, clapboard(WOOD_RAW))),
        [0.0, slab_h + 2.9, front_z + 1.3],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 2.9, 0.2], 0.0, clapboard(WOOD_RAW))),
            [sx * 3.6, slab_h + 1.45, front_z + 2.3],
            id_quat(),
        ));
    }
    // Balcony rail along the porch roof edge.
    prims.push(prim(
        cuboid_tapered([8.4, 0.5, 0.1], 0.0, clapboard(CLAP_WHITE)),
        [0.0, slab_h + 3.35, front_z + 2.4],
        id_quat(),
    ));

    // Hanging sign on iron brackets.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.6, 0.1], 0.0, iron(IRON_DARK))),
        [-2.6, slab_h + 3.2, front_z + 0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([2.4, 0.9, 0.12], 0.0, clapboard(CLAP_WHITE))),
        [-2.6, slab_h + 2.6, front_z + 0.4],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a dry prairie wind, dust skating the street.
    root.audio = fx::prairie_wind();
    root.children
        .push(fx::dust_drift([0.0, 0.3, front_z + 4.0], 0x0DE5_5A12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Saloon.build(""), "saloon");
    }

    #[test]
    fn has_lit_windows() {
        assert!(super::super::has_emissive(&Saloon.build("")));
    }
}
