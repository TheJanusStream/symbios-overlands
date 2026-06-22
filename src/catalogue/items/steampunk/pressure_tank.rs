//! Pressure tank — a Steampunk prop. A riveted iron boiler on saddle
//! supports, brass end caps and a valve on top. Scatter clutter beside the
//! works.
//!
//! The tank is a cylinder tipped on its side with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, prim_scaled, quat_x, solid,
    sphere, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, GAUGE_AMBER, IRON_DARK, brass, iron};

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
    // The root must have identity rotation: assemble() only rebases child
    // translations, so a rotated root would spin every child into its frame.
    // A flat skid pallet is the root; the tank (laid along Z) is a child.
    let mut prims = vec![prim(
        solid(cuboid_tapered([1.5, 0.16, 2.5], 0.0, iron(IRON_DARK))),
        [0.0, 0.08, 0.0],
        id_quat(),
    )];

    // Riveted iron tank, laid along Z.
    prims.push(prim(
        solid(cylinder_tapered(0.8, 2.8, 14, 0.0, iron(IRON_DARK))),
        [0.0, 1.0, 0.0],
        quat_x(FRAC_PI_2),
    ));

    // Brass dished heads — profile-cut hemispheres flattened into shallow
    // dished caps bulging off each Z end.
    for (z, rot) in [(1.4_f32, quat_x(FRAC_PI_2)), (-1.4, quat_x(-FRAC_PI_2))] {
        prims.push(prim_scaled(
            solid(with_cut(
                sphere(0.8, 6, brass(BRASS)),
                [0.0, 1.0],
                [0.5, 1.0],
                0.0,
            )),
            [0.0, 1.0, z],
            rot,
            [1.0, 0.65, 1.0],
        ));
    }

    // Dark riveted hoop bands.
    for z in [-0.7_f32, 0.0, 0.7] {
        prims.push(prim(
            solid(torus(0.05, 0.84, iron(IRON_DARK))),
            [0.0, 1.0, z],
            quat_x(FRAC_PI_2),
        ));
    }

    // Two saddle supports on the skid base.
    for z in [-0.9_f32, 0.9] {
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.55, 0.5], 0.1, iron(IRON_DARK))),
            [0.0, 0.3, z],
            id_quat(),
        ));
    }

    // Brass valve on top — a hollow riser with a spoked hand-wheel (rim raised
    // above the spoke plane so the open quadrants read, not a solid disc).
    prims.push(prim(
        solid(tube(0.1, 0.05, 0.5, 8, brass(BRASS))),
        [0.0, 1.9, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.07, 0.18, 8, 0.0, brass(BRASS))),
        [0.0, 2.18, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.04, 0.05], 0.0, brass(BRASS))),
        [0.0, 2.14, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.05, 0.04, 0.5], 0.0, brass(BRASS))),
        [0.0, 2.14, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.04, 0.28, brass(BRASS))),
        [0.0, 2.19, 0.0],
        id_quat(),
    ));

    // Lit pressure gauge on the −Z (hero) head: a flat amber face on a dark
    // plate (flat emission reads; a disc flush on the curved head is lost),
    // seated into the dished head so it doesn't float from the side.
    prims.push(prim(
        solid(cuboid_tapered([0.66, 0.66, 0.1], 0.0, iron(IRON_DARK))),
        [0.0, 1.0, -1.95],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.5, 0.06], 0.0, glow(GAUGE_AMBER, 3.0)),
        [0.0, 1.0, -2.02],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.04, 0.32, brass(BRASS))),
        [0.0, 1.0, -2.05],
        quat_x(FRAC_PI_2),
    ));
    // Needle on the dial face.
    prims.push(prim(
        cuboid_tapered([0.04, 0.24, 0.04], 0.0, iron(IRON_DARK)),
        [0.05, 1.06, -2.07],
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
