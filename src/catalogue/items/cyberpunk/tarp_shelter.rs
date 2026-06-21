//! Tarp shelter — a Cyberpunk *poor* secondary. Four lashed poles under a
//! sagging plastic tarp with drooping side flaps, a crate and barrel of
//! salvage, and a dim hanging lamp warmed by a burn-barrel; a makeshift
//! undercity stall.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, RUST_BROWN, TARP_BLUE, fx, metal, rust, tarp};

pub struct TarpShelter;

impl CatalogueEntry for TarpShelter {
    fn slug(&self) -> &'static str {
        "tarp_shelter"
    }
    fn name(&self) -> &'static str {
        "Tarp Shelter"
    }
    fn description(&self) -> &'static str {
        "Lashed poles under a sagging tarp with salvage crates and a dim lamp."
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
    let pole_h = 2.4_f32;
    let half = 1.4_f32;
    let pole = || solid(cylinder_tapered(0.08, pole_h, 6, 0.0, metal(DARK_METAL)));

    let mut prims = vec![prim(pole(), [-half, pole_h * 0.5, -half], id_quat())];
    for (sx, sz) in [(1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            pole(),
            [sx * half, pole_h * 0.5, sz * half],
            id_quat(),
        ));
    }

    // Sagging tarp — the shallow bottom cap of a big sphere, so the membrane
    // dips lowest in the middle and lifts to the pole tops, reading as draped
    // cloth instead of a flat slab. (`profile_cut` keeps a thin latitude band
    // off the south pole; the big radius makes that cap wide and shallow.)
    prims.push(prim(
        with_cut(
            sphere(6.8, 6, tarp(TARP_BLUE)),
            [0.0, 1.0],
            [0.0, 0.095],
            0.0,
        ),
        [0.0, 8.9, 0.0],
        id_quat(),
    ));

    // Drooping side flaps hanging off two edges.
    prims.push(prim(
        cuboid_tapered([2.9, 0.05, 1.2], 0.0, tarp(TARP_BLUE)),
        [0.0, 1.7, half + 0.15],
        quat_x(1.25),
    ));
    prims.push(prim(
        cuboid_tapered([1.0, 0.05, 1.8], 0.0, tarp([0.30, 0.26, 0.20])),
        [-half - 0.1, 1.85, 0.2],
        quat_x(0.2),
    ));

    // Salvage crate + a rusted barrel underneath.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.9, 0.9], 0.0, rust(RUST_BROWN))),
        [0.55, 0.45, 0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(
            0.4,
            1.0,
            12,
            0.0,
            rust([0.34, 0.30, 0.22]),
        )),
        [-0.5, 0.5, 0.6],
        id_quat(),
    ));

    // Dim lamp hanging under the tarp.
    prims.push(prim(
        sphere(0.16, 3, glow(NEON_CYAN, 3.0)),
        [0.2, pole_h - 0.45, 0.0],
        id_quat(),
    ));
    // A burn-barrel brazier in the corner keeping the shelter warm.
    prims.push(fx::brazier_flame([-0.7, 0.4, -0.5], 0xB7A2_F1A3));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TarpShelter.build(""), "tarp_shelter");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &TarpShelter.build("")
        ));
    }
}
