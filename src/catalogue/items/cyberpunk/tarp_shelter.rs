//! Tarp shelter — a Cyberpunk *poor* secondary. Four lashed poles under a
//! sagging plastic tarp, a crate of salvage, and a single dim hanging lamp;
//! a makeshift undercity stall.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, RUST_BROWN, TARP_BLUE, metal, rust, tarp};

pub struct TarpShelter;

impl CatalogueEntry for TarpShelter {
    fn slug(&self) -> &'static str {
        "tarp_shelter"
    }
    fn name(&self) -> &'static str {
        "Tarp Shelter"
    }
    fn description(&self) -> &'static str {
        "Lashed poles under a sagging tarp with a salvage crate and a dim lamp."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 2.4;
    let half = 1.4;
    let pole = || solid(cylinder_tapered(0.08, pole_h, 6, 0.0, metal(DARK_METAL)));

    let mut prims = vec![
        // Four corner poles (first is the root).
        prim(pole(), [-half, pole_h * 0.5, -half], id_quat()),
    ];
    for (sx, sz) in [(1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            pole(),
            [sx * half, pole_h * 0.5, sz * half],
            id_quat(),
        ));
    }
    // Sagging tarp roof (gently sloped).
    prims.push(prim(
        cuboid_tapered([3.2, 0.06, 3.0], 0.0, tarp(TARP_BLUE)),
        [0.0, pole_h + 0.05, 0.0],
        quat_x(0.12),
    ));
    // Salvage crate underneath.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.9, 0.9], 0.0, rust(RUST_BROWN))),
        [0.5, 0.45, 0.3],
        id_quat(),
    ));
    // Dim lamp hanging from the ridge.
    prims.push(prim(
        sphere(0.16, 3, glow(NEON_CYAN, 3.0)),
        [0.0, pole_h - 0.3, 0.0],
        id_quat(),
    ));

    assemble(prims)
}
