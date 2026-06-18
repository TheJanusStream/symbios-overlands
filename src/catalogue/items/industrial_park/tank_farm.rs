//! Tank farm — an Industrial-Park secondary. A cluster of painted steel
//! storage tanks inside a low concrete containment bund, linked by pipework
//! and a riser, one relief stack hissing steam.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, PIPE_GREY, TANK_WHITE, concrete, fx, tank_steel};

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

    // Storage tanks: (x, z, radius, height).
    let tanks = [
        (-3.0_f32, 1.5_f32, 2.2_f32, 6.0_f32),
        (3.2, -1.0, 2.6, 7.0),
        (0.5, -3.5, 1.6, 5.0),
    ];
    for (tx, tz, r, h) in tanks {
        prims.push(prim(
            solid(cylinder_tapered(r, h, 20, 0.0, tank_steel(TANK_WHITE))),
            [tx, 0.4 + h * 0.5, tz],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(r + 0.1, r * 0.5, 20, tank_steel([0.66, 0.66, 0.64]))),
            [tx, 0.4 + h + r * 0.25, tz],
            id_quat(),
        ));
        // Hoop bands.
        for k in 1..3 {
            prims.push(prim(
                cuboid_tapered(
                    [r * 2.0 + 0.06, 0.12, r * 2.0 + 0.06],
                    0.0,
                    tank_steel(PIPE_GREY),
                ),
                [tx, 0.4 + h * (k as f32 / 3.0), tz],
                id_quat(),
            ));
        }
    }

    // Pipework linking the tank bases, with a valve wheel.
    prims.push(prim(
        solid(cuboid_tapered([6.4, 0.3, 0.3], 0.0, tank_steel(PIPE_GREY))),
        [0.0, 0.9, 1.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.06, 0.3, tank_steel([0.7, 0.5, 0.2])),
        [0.0, 1.3, 1.0],
        id_quat(),
    ));

    // Relief stack hissing steam.
    let relief = [4.4_f32, -3.5_f32];
    prims.push(prim(
        solid(cylinder_tapered(0.18, 3.0, 10, 0.0, tank_steel(PIPE_GREY))),
        [relief[0], 0.4 + 1.5, relief[1]],
        id_quat(),
    ));
    let mut hiss = fx::stack_vent([relief[0], 0.4 + 3.2, relief[1]], 0x57EA_DEE0);
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
