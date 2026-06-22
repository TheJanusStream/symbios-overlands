//! Scrap boiler — a Steampunk *poor* secondary. A rusted-through boiler tank
//! leaning on a makeshift cradle, rigged with patched copper pipes and a bent
//! chimney. The improvised still of the soot-yard.
//!
//! The tank is a cylinder tipped on its side with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, prim_scaled, quat_x, quat_z,
    solid, sphere, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{COPPER_ORANGE, FURNACE_ORANGE, IRON_DARK, WOOD_BROWN, copper, fx, iron, plank};

/// Heavy rust of the failing boiler.
const RUST: [f32; 3] = [0.45, 0.28, 0.16];

pub struct ScrapBoiler;

impl CatalogueEntry for ScrapBoiler {
    fn slug(&self) -> &'static str {
        "scrap_boiler"
    }
    fn name(&self) -> &'static str {
        "Scrap Boiler"
    }
    fn description(&self) -> &'static str {
        "Rusted boiler on a makeshift cradle, rigged with patched pipes and a bent chimney."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // The root must have identity rotation: assemble() only rebases child
    // translations, so a rotated root spins every child into its frame. A flat
    // timber ground sill is the root; the tank (laid along Z) is a child.
    let mut prims = vec![prim(
        solid(cuboid_tapered([0.5, 0.18, 2.4], 0.0, plank(WOOD_BROWN))),
        [0.0, 0.09, 0.0],
        id_quat(),
    )];

    // Rusted boiler tank, laid along Z.
    prims.push(prim(
        solid(cylinder_tapered(0.85, 2.6, 14, 0.0, iron(RUST))),
        [0.0, 1.1, 0.0],
        quat_x(FRAC_PI_2),
    ));

    // Dished boiler heads — profile-cut hemispheres flattened into shallow
    // dished caps (not full hemispheres, which read as a pill), bulging off
    // each end so the tank reads as a pressure vessel, not a flat tin can.
    for (z, rot) in [(1.3_f32, quat_x(FRAC_PI_2)), (-1.3, quat_x(-FRAC_PI_2))] {
        prims.push(prim_scaled(
            solid(with_cut(
                sphere(0.85, 6, iron(RUST)),
                [0.0, 1.0],
                [0.5, 1.0],
                0.0,
            )),
            [0.0, 1.1, z],
            rot,
            [1.0, 0.6, 1.0],
        ));
    }

    // Dark riveted hoop bands cinching the rusted plate.
    for z in [-0.7_f32, 0.0, 0.7] {
        prims.push(prim(
            solid(torus(0.05, 0.9, iron(IRON_DARK))),
            [0.0, 1.1, z],
            quat_x(FRAC_PI_2),
        ));
    }
    // A bolted-on patch plate over a rusted-through gap.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.12, 0.8], 0.0, iron(IRON_DARK))),
        [0.35, 1.82, 0.2],
        quat_x(-0.3),
    ));

    // Firebox stoked in front of the −Z (hero) head — a grounded stove box
    // with a flat glowing door, pulled clear of the head so the fire reads
    // (emission flush on the curved head z-fights and is lost).
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.8, 0.7], 0.05, iron(IRON_DARK))),
        [0.0, 0.4, -1.7],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.7, 0.55, 0.12], 0.0, glow(FURNACE_ORANGE, 3.2)),
        [0.0, 0.42, -2.07],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.78, 0.12, 0.74], 0.0, iron(IRON_DARK))),
        [0.0, 0.85, -1.7],
        id_quat(),
    ));

    // Makeshift timber V-cradle holding the tank off the ground.
    for z in [-0.9_f32, 0.9] {
        for (sx, tilt) in [(-1.0_f32, 0.42_f32), (1.0, -0.42)] {
            prims.push(prim(
                solid(cuboid_tapered([0.22, 1.3, 0.35], 0.0, plank(WOOD_BROWN))),
                [sx * 0.62, 0.55, z],
                quat_z(tilt),
            ));
        }
    }
    // Ground sills tying the cradle together.
    for sx in [-0.6_f32, 0.6] {
        prims.push(prim(
            solid(cuboid_tapered([0.25, 0.18, 2.3], 0.0, plank(WOOD_BROWN))),
            [sx, 0.09, 0.0],
            id_quat(),
        ));
    }

    // Bent iron stovepipe leaning off the top, with a flared cowl, smoking.
    prims.push(prim(
        solid(tube(0.2, 0.13, 2.2, 8, iron(IRON_DARK))),
        [0.35, 3.0, -0.5],
        quat_x(0.2),
    ));
    prims.push(prim(
        solid(torus(0.08, 0.28, iron(IRON_DARK))),
        [0.55, 4.05, -0.28],
        quat_x(0.2),
    ));

    // Patched copper feed pipe rigged across the tank, with a valve wheel.
    prims.push(prim(
        solid(tube(0.12, 0.07, 2.2, 8, copper(COPPER_ORANGE))),
        [0.9, 1.55, 0.0],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(torus(0.05, 0.22, copper(COPPER_ORANGE))),
        [0.9, 1.55, 1.15],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: soot seeping from the bent stovepipe.
    root.children
        .push(fx::furnace_smoke([0.6, 4.4, -0.25], 0x500F_B011));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ScrapBoiler.build(""), "scrap_boiler");
    }
}
