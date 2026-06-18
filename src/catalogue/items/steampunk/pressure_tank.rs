//! Pressure tank — a Steampunk prop. A riveted iron boiler on saddle
//! supports, brass end caps and a valve on top. Scatter clutter beside the
//! works.
//!
//! The tank is a cylinder tipped on its side with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, IRON_DARK, brass, iron};

pub struct PressureTank;

impl CatalogueEntry for PressureTank {
    fn slug(&self) -> &'static str {
        "pressure_tank"
    }
    fn name(&self) -> &'static str {
        "Pressure Tank"
    }
    fn description(&self) -> &'static str {
        "Riveted iron boiler on saddle supports with brass end caps and a valve."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Iron tank — the root, laid along Z.
        prim(
            solid(cylinder_tapered(0.8, 2.8, 14, 0.0, iron(IRON_DARK))),
            [0.0, 1.0, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Brass end caps.
    for z in [-1.4_f32, 1.4] {
        prims.push(prim(
            solid(torus(0.12, 0.78, brass(BRASS))),
            [0.0, 1.0, z],
            quat_x(FRAC_PI_2),
        ));
    }

    // Two saddle supports.
    for z in [-0.9_f32, 0.9] {
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.5, 0.5], 0.0, iron(IRON_DARK))),
            [0.0, 0.25, z],
            id_quat(),
        ));
    }

    // Brass valve on top.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 0.5, 6, 0.0, brass(BRASS))),
        [0.0, 1.9, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.05, 0.22, brass(BRASS))),
        [0.0, 2.15, 0.0],
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
        assert_sanitize_stable(&PressureTank.build(""), "pressure_tank");
    }
}
