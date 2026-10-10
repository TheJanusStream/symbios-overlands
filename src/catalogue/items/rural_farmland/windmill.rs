//! Windmill - a Rural/Farmland secondary. An American farm wind pump: an open
//! steel lattice tower carrying a multi-blade fan wheel and a tail vane that
//! turns lazily, creaking and groaning in the breeze, to draw water for the
//! stock.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::coastal_resort::{POOL_AQUA, water};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, footing, id_quat, prim, quat_x, quat_z, solid,
    steady, torus, tube, turning, vane,
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
        // Concrete pad - the root.
        prim(
            solid(cuboid_tapered([3.0, 0.3, 3.0], 0.0, stone(STONE_GREY))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];
    prims.push(footing(3.0, 3.0, [0.0, 0.0], 5.0));

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
    // Two diagonal cross-braces in the back face, leg to leg across the
    // second and third bays up from the pad, between the first three ring
    // braces. (They used to turn about X, which stood them out of
    // the face at its middle, touching nothing: #1537.)
    let ring_y = |k: f32| 0.3 + tower_h * (k / 4.5);
    for (k, lean) in [(1.0_f32, -1.0_f32), (2.0, 1.0)] {
        let (low, high) = (ring_y(k), ring_y(k + 1.0));
        let rise = high - low;
        prims.push(prim(
            cuboid_tapered(
                [0.07, (4.0 * half * half + rise * rise).sqrt(), 0.07],
                0.0,
                enamel(STEEL),
            ),
            [0.0, (low + high) * 0.5, half],
            quat_z(lean * (2.0 * half).atan2(rise)),
        ));
    }
    // A cap plate on the leg tops, and a gearbox head on it that the tail
    // boom runs through: the head used to hang 0.35 m over the legs (#1537).
    // The head turns to face the wind (#1604), so the plate must clear the
    // wheel at every heading: 2.16 m across, still over the legs' outer faces
    // (1.06 m out), its corner (1.527 m from the axis) short of the rim
    // disc's back face (1.54 m out); and sunk half its thickness onto the
    // legs, its top (10.35 m) under the wheel's hub (10.36 m), which a
    // diagonal heading swings out over the corner.
    let leg_top = 0.3 + tower_h;
    prims.push(prim(
        solid(cuboid_tapered([2.16, 0.1, 2.16], 0.0, enamel(STEEL))),
        [0.0, leg_top, 0.0],
        id_quat(),
    ));

    // Fan wheel at the top, authored facing the −Z front (the camera) so the
    // multi-blade wheel reads head-on in a calm and on a contact sheet; the
    // tail vane trails to the +Z back. In a world the head turns the wheel
    // into the wind (below). A rotated cylinder/torus is fine here - these
    // are non-first children, not the root.
    let hub_y = 0.3 + tower_h + 0.4;
    let hub_z = -(half + 0.6);
    let blade_z = hub_z - 0.08; // blades stand proud on the front face
    let mut fan = vec![
        // Wheel rim disc and hub.
        prim(
            solid(cylinder_tapered(
                1.7,
                0.12,
                24,
                0.0,
                enamel([0.66, 0.68, 0.70]),
            )),
            [0.0, hub_y, hub_z],
            quat_x(FRAC_PI_2),
        ),
        prim(
            solid(cylinder_tapered(0.34, 0.5, 12, 0.0, enamel(STEEL))),
            [0.0, hub_y, hub_z],
            quat_x(FRAC_PI_2),
        ),
    ];
    // Radial sheet-steel blades around the wheel face.
    // Neighbours overlap near the hub, so every other blade stands 4 mm
    // further forward: in one plane their faces z-fought (#1537).
    let blades = 16;
    for k in 0..blades {
        let th = k as f32 / blades as f32 * TAU;
        let z = blade_z - if k % 2 == 1 { 0.004 } else { 0.0 };
        fan.push(prim(
            cuboid_tapered([1.1, 0.26, 0.03], 0.0, enamel([0.8, 0.82, 0.84])),
            [0.95 * th.cos(), hub_y + 0.95 * th.sin(), z],
            quat_z(th),
        ));
    }
    // Outer band ring catching the blade tips.
    fan.push(prim(
        torus(0.05, 1.55, enamel(STEEL)),
        [0.0, hub_y, blade_z],
        quat_x(FRAC_PI_2),
    ));
    // The wheel turns on its axle (#1604): a square shaft through the hub, its
    // nut proud of the face, and every blade and ring nested on it. Thin
    // enough to stay inside the tail boom it runs into, however it turns.
    let wheel = turning(
        prim(
            solid(cuboid_tapered([0.06, 0.06, 0.62], 0.0, enamel(STEEL))),
            [0.0, hub_y, hub_z],
            id_quat(),
        ),
        [0.0, 0.0, 1.0],
        steady(-40.0),
        fan,
    );

    // Tail boom and vane trailing to the +Z back. The boom starts 5 cm inside
    // the hub (its back face is at hub_z + 0.25) - it used to stop 25 cm
    // short - and runs through the gearbox head on the tower's cap.
    let boom = prim(
        solid(cuboid_tapered([0.1, 0.1, 2.5], 0.0, enamel(STEEL))),
        [0.0, hub_y, hub_z + 1.45],
        id_quat(),
    );
    let head_low = leg_top + 0.05 - 0.01;
    let head_high = hub_y + 0.05 + 0.06;
    // The vane stands 0.3 m up on the boom, so its foot clears the cap plate
    // it hung 15 cm through (#1537's review).
    let mut tail = prim(
        solid(cuboid_tapered([0.06, 1.1, 1.5], 0.0, enamel(TRACTOR_GREEN))),
        [0.0, hub_y + 0.3, hub_z + 2.6],
        id_quat(),
    );
    tail.audio = fx::windmill_creak();
    // The head the boom runs through turns on the tower's axis until the
    // tail vane (+Z) points downwind and the wheel faces into the room's
    // wind, hunting a little as it gusts (#1604). Every heading is clear:
    // the rim disc's back face stays 1.54 m out, past the legs' outer
    // corners (1.50 m) and the cap plate's (1.527 m).
    prims.push(turning(
        prim(
            solid(cuboid_tapered(
                [0.36, head_high - head_low, 0.5],
                0.0,
                enamel(STEEL),
            )),
            [0.0, (head_low + head_high) * 0.5, 0.0],
            id_quat(),
        ),
        [0.0, 1.0, 0.0],
        vane([0.0, 0.0, 1.0]),
        vec![wheel, boom, tail],
    ));

    // Galvanised stock tank the pump fills - an open-topped ring of water
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
