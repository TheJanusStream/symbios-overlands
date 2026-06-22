//! Pipework — a Steampunk prop. An L-shaped run of hollow copper pipe on iron
//! leg-brackets, with brass flange joints, gate-valve wheels and a riser with
//! a lit gauge. Scatter clutter threading the works.
//!
//! The horizontal run is a [`tube`] laid along X with a [`quat_z`] of π/2; the
//! riser stands on its end.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, sphere,
    torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, COPPER_ORANGE, GAUGE_AMBER, IRON_DARK, brass, copper, glass, iron};

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
    let run_y = 1.1_f32;
    // The root must have identity rotation: assemble() only rebases child
    // translations, so a rotated root spins every child into its frame. A flat
    // skid baseplate is the root; the run (laid along X) is a child.
    let mut prims = vec![prim(
        solid(cuboid_tapered([3.2, 0.12, 0.6], 0.0, iron(IRON_DARK))),
        [0.0, 0.06, 0.0],
        id_quat(),
    )];

    // Horizontal copper run along X — a real hollow pipe.
    prims.push(prim(
        solid(tube(0.28, 0.18, 3.0, 12, copper(COPPER_ORANGE))),
        [0.0, run_y, 0.0],
        quat_z(FRAC_PI_2),
    ));

    // Three iron support brackets standing on the skid, brass saddle clamps.
    for x in [-1.2_f32, 0.0, 1.2] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, run_y, 0.22], 0.0, iron(IRON_DARK))),
            [x, run_y * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.05, 0.3, brass(BRASS))),
            [x, run_y, 0.0],
            quat_z(FRAC_PI_2),
        ));
    }

    // Brass flange joints bolting the pipe segments together.
    for x in [-0.6_f32, 0.6] {
        prims.push(prim(
            solid(torus(0.07, 0.33, brass(BRASS))),
            [x, run_y, 0.0],
            quat_z(FRAC_PI_2),
        ));
    }

    // Copper elbow ball + a hollow vertical riser at the right end.
    prims.push(prim(
        solid(sphere(0.32, 6, copper(COPPER_ORANGE))),
        [1.5, run_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(tube(0.28, 0.18, 1.5, 12, copper(COPPER_ORANGE))),
        [1.5, run_y + 0.75, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.07, 0.33, brass(BRASS))),
        [1.5, run_y + 1.5, 0.0],
        id_quat(),
    ));

    // Brass gate-valve wheels on bonnet stems atop the run.
    for x in [-0.9_f32, 0.3] {
        prims.push(prim(
            solid(cylinder_tapered(0.08, 0.45, 8, 0.0, brass(BRASS))),
            [x, run_y + 0.35, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.05, 0.26, brass(BRASS))),
            [x, run_y + 0.62, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.48, 0.04, 0.05], 0.0, brass(BRASS))),
            [x, run_y + 0.62, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.05, 0.04, 0.48], 0.0, brass(BRASS))),
            [x, run_y + 0.62, 0.0],
            id_quat(),
        ));
    }

    // Lit pressure gauge on the riser facing −Z, its dark dial seated into the
    // pipe wall so it doesn't read as a floating tab from the side.
    prims.push(prim(
        solid(cylinder_tapered(0.2, 0.16, 12, 0.0, iron(IRON_DARK))),
        [1.5, run_y + 0.95, -0.26],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cylinder_tapered(0.16, 0.05, 12, 0.0, glass(GAUGE_AMBER, 2.2)),
        [1.5, run_y + 0.95, -0.36],
        quat_x(FRAC_PI_2),
    ));

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
