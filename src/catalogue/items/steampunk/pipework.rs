//! Pipework — a Steampunk prop. A run of copper pipe with a riser, brass
//! valve wheels and iron support brackets. Scatter clutter threading the
//! works.
//!
//! The pipe runs are tapered cylinders tipped on their side with a
//! [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, COPPER_ORANGE, IRON_DARK, brass, copper, iron};

pub struct Pipework;

impl CatalogueEntry for Pipework {
    fn slug(&self) -> &'static str {
        "pipework"
    }
    fn name(&self) -> &'static str {
        "Pipework"
    }
    fn description(&self) -> &'static str {
        "A run of copper pipe with a riser, brass valve wheels and support brackets."
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
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Horizontal copper run — the root, laid along Z.
        prim(
            solid(cylinder_tapered(0.3, 3.4, 10, 0.0, copper(COPPER_ORANGE))),
            [0.0, 0.7, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Vertical riser at one end.
    prims.push(prim(
        solid(cylinder_tapered(0.3, 1.6, 10, 0.0, copper(COPPER_ORANGE))),
        [0.0, 1.5, -1.7],
        id_quat(),
    ));
    // Iron elbow box at the bend.
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.5, 0.5], 0.0, iron(IRON_DARK))),
        [0.0, 0.7, -1.7],
        id_quat(),
    ));

    // Brass valve wheels along the run.
    for z in [-0.6_f32, 0.9] {
        prims.push(prim(
            solid(torus(0.06, 0.32, brass(BRASS))),
            [0.0, 1.05, z],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(0.08, 0.4, 6, 0.0, brass(BRASS))),
            [0.0, 0.9, z],
            id_quat(),
        ));
    }

    // Two iron support brackets.
    for z in [-1.3_f32, 1.3] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 0.7, 0.2], 0.0, iron(IRON_DARK))),
            [0.0, 0.35, z],
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
        assert_sanitize_stable(&Pipework.build(""), "pipework");
    }
}
