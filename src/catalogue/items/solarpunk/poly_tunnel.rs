//! Poly-tunnel — a Solarpunk *poor* secondary. A plastic-sheet greenhouse
//! stretched over steel hoops, rows of crops inside. The makeshift glasshouse
//! of the grassroots commune.
//!
//! The tunnel shell is a cylinder tipped on its side with a [`quat_x`] of
//! π/2; its lower half sits below grade so it reads as an arched tunnel.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CROP_GREEN, GLASS_CLEAN, STEEL_GREY, foliage, glass, steel};

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
    let axis_y = 1.0_f32;
    let radius = 1.8_f32;

    let mut prims = vec![
        // Translucent plastic shell — the root, laid along Z.
        prim(
            solid(cylinder_tapered(
                radius,
                6.0,
                16,
                0.0,
                glass(GLASS_CLEAN, 0.0),
            )),
            [0.0, axis_y, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Steel hoops showing through the sheet.
    for z in [-2.4_f32, 0.0, 2.4] {
        prims.push(prim(
            solid(torus(0.05, radius, steel(STEEL_GREY))),
            [0.0, axis_y, z],
            quat_x(FRAC_PI_2),
        ));
    }

    // End panels closing the arch.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered(
                [radius * 2.0, radius * 1.6, 0.1],
                0.0,
                glass(GLASS_CLEAN, 0.0),
            ),
            [0.0, axis_y - 0.2, sz * 3.0],
            id_quat(),
        ));
    }

    // Two rows of crops inside.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.6, 0.5, 5.0], 0.0, foliage(CROP_GREEN))),
            [sx * 0.7, 0.25, 0.0],
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
        assert_sanitize_stable(&PolyTunnel.build(""), "poly_tunnel");
    }
}
