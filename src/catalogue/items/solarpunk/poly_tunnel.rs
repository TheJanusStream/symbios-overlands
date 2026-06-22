//! Poly-tunnel — a Solarpunk *poor* secondary. A plastic-sheet greenhouse
//! stretched over steel hoops, rows of crops inside. The makeshift glasshouse
//! of the grassroots commune.
//!
//! The shell is an open round-up half-cylinder ([`with_cut`] `path_cut`) tipped
//! along Z with a [`quat_x`] of π/2, arching over the crops so the rows read
//! through and under the sheet. It is a rotation-safe *child*: the flat earth
//! floor pad is the [`assemble`] root (a tilted shell can never be the root, or
//! its rotation spins every other piece — the prior version did exactly that).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CROP_GREEN, GLASS_CLEAN, SOIL_DARK, STEEL_GREY, crop_tufts, foliage, glass, steel};

pub struct PolyTunnel;

impl CatalogueEntry for PolyTunnel {
    fn slug(&self) -> &'static str {
        "poly_tunnel"
    }
    fn name(&self) -> &'static str {
        "Poly-Tunnel"
    }
    fn description(&self) -> &'static str {
        "Plastic-sheet greenhouse over steel hoops with rows of crops inside."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let radius = 1.85_f32;
    let length = 6.0_f32;
    let floor_h = 0.12_f32;

    let mut prims = vec![
        // Earth floor pad — the flat root. Rooting here keeps the arched shell
        // a rotation-safe child instead of a rotated root.
        prim(
            solid(cuboid_tapered(
                [radius * 2.0, floor_h, length],
                0.0,
                foliage(SOIL_DARK),
            )),
            [0.0, floor_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Translucent plastic-sheet vault (round side up) arching over the crops,
    // laid along Z; the flat cut side sits at the floor so the rows read.
    prims.push(prim(
        solid(with_cut(
            cylinder_tapered(radius, length, 18, 0.0, glass(GLASS_CLEAN, 0.25)),
            [0.5, 1.0],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, floor_h, 0.0],
        quat_x(FRAC_PI_2),
    ));
    // Steel hoops as arch ribs showing through the sheet.
    for z in [-2.6_f32, -0.9, 0.9, 2.6] {
        prims.push(prim(
            solid(with_cut(
                torus(0.05, radius, steel(STEEL_GREY)),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            )),
            [0.0, floor_h, z],
            quat_x(-FRAC_PI_2),
        ));
    }

    // A glazed end panel closing the +Z back; an open doorway frame at the −Z
    // front so the crop rows read into the hero camera.
    prims.push(prim(
        cuboid_tapered(
            [radius * 1.9, radius * 1.4, 0.08],
            0.0,
            glass(GLASS_CLEAN, 0.25),
        ),
        [0.0, floor_h + radius * 0.7, length * 0.5],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, radius * 1.5, 0.12],
                0.0,
                steel(STEEL_GREY),
            )),
            [sx * 0.78, floor_h + radius * 0.75, -length * 0.5],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([1.74, 0.12, 0.12], 0.0, steel(STEEL_GREY))),
        [0.0, floor_h + radius * 1.45, -length * 0.5],
        id_quat(),
    ));

    // Two rows of leafy crops in raised earth beds.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.7, 0.3, 5.2], 0.0, foliage(SOIL_DARK))),
            [sx * 0.72, floor_h + 0.15, 0.0],
            id_quat(),
        ));
        prims.extend(crop_tufts(
            [sx * 0.72, floor_h + 0.3, 0.0],
            [0.55, 5.0],
            2,
            8,
            0.5,
            foliage(CROP_GREEN),
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
        assert_sanitize_stable(&PolyTunnel.build(""), "poly_tunnel");
    }
}
