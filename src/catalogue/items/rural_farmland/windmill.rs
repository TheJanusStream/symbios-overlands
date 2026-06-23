//! Windmill — a Rural/Farmland secondary. An American farm wind pump: an open
//! steel lattice tower carrying a multi-blade fan wheel and a tail vane that
//! turns lazily, creaking and groaning in the breeze, to draw water for the
//! stock.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::coastal_resort::{POOL_AQUA, water};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_GREY, TRACTOR_GREEN, enamel, fx, stone};

/// Galvanised steel for the tower and fan.
const STEEL: [f32; 3] = [0.58, 0.60, 0.62];

pub struct Windmill;

impl CatalogueEntry for Windmill {
    fn slug(&self) -> &'static str {
        "windmill"
    }
    fn name(&self) -> &'static str {
        "Windmill"
    }
    fn description(&self) -> &'static str {
        "Steel lattice wind pump with a multi-blade fan wheel and a tail vane."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let tower_h = 10.0_f32;
    let half = 1.0_f32;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered([3.0, 0.3, 3.0], 0.0, stone(STONE_GREY))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Four vertical lattice legs.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, tower_h, 0.12], 0.0, enamel(STEEL))),
            [sx * half, 0.3 + tower_h * 0.5, sz * half],
            id_quat(),
        ));
    }
    // Square ring braces up the tower.
    for k in 1..=4 {
        let y = 0.3 + tower_h * (k as f32 / 4.5);
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([0.08, 0.08, 2.0 * half], 0.0, enamel(STEEL)),
                [sx * half, y, 0.0],
                id_quat(),
            ));
        }
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([2.0 * half, 0.08, 0.08], 0.0, enamel(STEEL)),
                [0.0, y, sz * half],
                id_quat(),
            ));
        }
    }
    // Two diagonal cross-braces on the front face.
    for (cy, ang) in [(2.3_f32, 0.6_f32), (5.7, 0.6)] {
        prims.push(prim(
            cuboid_tapered([0.07, 3.4, 0.07], 0.0, enamel(STEEL)),
            [0.0, cy, half],
            quat_x(ang),
        ));
    }

    // Fan wheel at the top, facing the −Z front (the camera) so the multi-blade
    // wheel reads head-on; the tail vane trails to the +Z back. A rotated
    // cylinder/torus is fine here — these are non-first children, not the root.
    let hub_y = 0.3 + tower_h + 0.4;
    let hub_z = -(half + 0.6);
    let blade_z = hub_z - 0.08; // blades stand proud on the front face
    // Wheel rim disc and hub.
    prims.push(prim(
        solid(cylinder_tapered(
            1.7,
            0.12,
            24,
            0.0,
            enamel([0.66, 0.68, 0.70]),
        )),
        [0.0, hub_y, hub_z],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.34, 0.5, 12, 0.0, enamel(STEEL))),
        [0.0, hub_y, hub_z],
        quat_x(FRAC_PI_2),
    ));
    // Radial sheet-steel blades around the wheel face.
    let blades = 16;
    for k in 0..blades {
        let th = k as f32 / blades as f32 * TAU;
        prims.push(prim(
            cuboid_tapered([1.1, 0.26, 0.03], 0.0, enamel([0.8, 0.82, 0.84])),
            [0.95 * th.cos(), hub_y + 0.95 * th.sin(), blade_z],
            quat_z(th),
        ));
    }
    // Outer band ring catching the blade tips.
    prims.push(prim(
        torus(0.05, 1.55, enamel(STEEL)),
        [0.0, hub_y, blade_z],
        quat_x(FRAC_PI_2),
    ));

    // Tail boom and vane trailing to the +Z back.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.1, 2.2], 0.0, enamel(STEEL))),
        [0.0, hub_y, hub_z + 1.6],
        id_quat(),
    ));
    let mut vane = prim(
        solid(cuboid_tapered([0.06, 1.1, 1.5], 0.0, enamel(TRACTOR_GREEN))),
        [0.0, hub_y, hub_z + 2.6],
        id_quat(),
    );
    vane.audio = fx::windmill_creak();
    prims.push(vane);

    // Galvanised stock tank the pump fills — an open-topped ring of water
    // (a real open vessel, not a sealed solid).
    let tank_x = 2.7_f32;
    prims.push(prim(
        solid(tube(0.95, 0.82, 0.7, 20, enamel(STEEL))),
        [tank_x, 0.35, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cylinder_tapered(0.84, 0.05, 20, 0.0, water(POOL_AQUA)),
        [tank_x, 0.6, 0.0],
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
        assert_sanitize_stable(&Windmill.build(""), "windmill");
    }
}
