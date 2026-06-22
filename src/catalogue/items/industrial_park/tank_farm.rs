//! Tank farm — an Industrial-Park secondary. A cluster of painted steel
//! storage tanks inside a low concrete containment bund, linked by pipework
//! and a riser, one relief stack hissing steam.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, prim_scaled, quat_z, solid, sphere,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, TANK_WHITE, concrete, fx, gauge_plate, tank_hoops,
    tank_steel, valve_wheel,
};

/// A pale-blue and a cream tank break the monochrome white cluster.
const TANK_CREAM: [f32; 3] = [0.78, 0.74, 0.62];
const TANK_BLUE: [f32; 3] = [0.50, 0.58, 0.64];

pub struct TankFarm;

impl CatalogueEntry for TankFarm {
    fn slug(&self) -> &'static str {
        "tank_farm"
    }
    fn name(&self) -> &'static str {
        "Tank Farm"
    }
    fn description(&self) -> &'static str {
        "Cluster of steel storage tanks in a concrete bund, linked by pipework."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad = 13.0_f32;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [pad, 0.4, pad - 1.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Low containment bund around the pad.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.4, 1.0, pad - 1.0],
                0.0,
                concrete([0.5, 0.5, 0.51]),
            )),
            [sx * pad * 0.5, 0.7, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [pad, 1.0, 0.4],
                0.0,
                concrete([0.5, 0.5, 0.51]),
            )),
            [0.0, 0.7, sz * (pad - 1.0) * 0.5],
            id_quat(),
        ));
    }

    // Storage tanks: (x, z, radius, height, livery).
    let base = 0.4_f32;
    let tanks = [
        (-3.0_f32, 1.5_f32, 2.2_f32, 6.0_f32, TANK_WHITE),
        (3.2, -1.0, 2.6, 7.0, TANK_CREAM),
        (0.5, -3.5, 1.6, 5.0, TANK_BLUE),
    ];
    for (tx, tz, r, h, color) in tanks {
        // Cylindrical shell.
        prims.push(prim(
            solid(cylinder_tapered(r, h, 20, 0.0, tank_steel(color))),
            [tx, base + h * 0.5, tz],
            id_quat(),
        ));
        // Dished (domed) roof — a flattened upper hemisphere, not a sharp cone.
        let cap = [color[0] * 0.9, color[1] * 0.9, color[2] * 0.9];
        prims.push(prim_scaled(
            with_cut(
                sphere(r * 0.99, 6, tank_steel(cap)),
                [0.0, 1.0],
                [0.5, 1.0],
                0.0,
            ),
            [tx, base + h, tz],
            id_quat(),
            [1.0, 0.5, 1.0],
        ));
        // Round hoop bands (no more square corners jutting past the wall).
        prims.extend(tank_hoops(tx, tz, base, r, h, 2, tank_steel(PIPE_GREY)));
    }

    // Access ladder up the -Z face of the big cream tank (centre x=3.2, the
    // tank's -Z wall sits at z = -1.0 - 2.6 = -3.6).
    let rail = tank_steel([0.3, 0.3, 0.32]);
    let lh = 7.0_f32;
    for sx in [-0.22_f32, 0.22] {
        prims.push(prim(
            solid(cylinder_tapered(0.04, lh, 6, 0.0, rail.clone())),
            [3.2 + sx, base + lh * 0.5, -3.62],
            id_quat(),
        ));
    }
    for k in 0..8 {
        prims.push(prim(
            solid(cuboid_tapered([0.5, 0.05, 0.05], 0.0, rail.clone())),
            [3.2, base + 0.6 + k as f32 * 0.8, -3.62],
            id_quat(),
        ));
    }

    // Pipe manifold linking the tank bases, with a spoked hand-wheel valve and
    // an elbow riser stepping up off it.
    prims.push(prim(
        solid(cylinder_tapered(0.18, 6.6, 12, 0.0, tank_steel(PIPE_GREY))),
        [0.1, 0.95, 1.0],
        quat_z(FRAC_PI_2),
    ));
    prims.push(valve_wheel(
        [0.1, 1.55, 1.0],
        id_quat(),
        0.42,
        tank_steel([0.66, 0.46, 0.2]),
    ));
    // Elbow: a short vertical leg up off the header into the cream tank.
    prims.push(prim(
        solid(cylinder_tapered(0.16, 1.2, 12, 0.0, tank_steel(PIPE_GREY))),
        [2.0, 1.5, 1.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.16, 1.3, 12, 0.0, tank_steel(PIPE_GREY))),
        [2.6, 2.0, 1.0],
        quat_z(FRAC_PI_2),
    ));

    // Lit control gauge on the bund's -Z front face — flat, so it reads.
    prims.extend(gauge_plate(
        [0.0, 1.1, -(pad - 1.0) * 0.5 - 0.04],
        0.5,
        LAMP_AMBER,
    ));

    // Relief stack hissing steam.
    let relief = [4.4_f32, -3.5_f32];
    prims.push(prim(
        solid(cylinder_tapered(0.18, 3.0, 10, 0.0, tank_steel(PIPE_GREY))),
        [relief[0], base + 1.5, relief[1]],
        id_quat(),
    ));
    let mut hiss = fx::stack_vent([relief[0], base + 3.2, relief[1]], 0x57EA_DEE0);
    hiss.audio = fx::steam_hiss();
    prims.push(hiss);

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TankFarm.build(""), "tank_farm");
    }
}
