//! Pump house — a Steampunk secondary. A tall brick engine house with arched
//! lit windows, a beam engine's rocking beam projecting from the gable, a
//! banded chimney and copper pipework. The waterworks of the quarter.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the engine house.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, id_quat, prim, quat_x, quat_z,
    solid, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRASS, BRICK_SOOT, COPPER_ORANGE, GLASS_AMBER, IRON_DARK, brass, brick, copper, fx, glass, iron,
};

pub struct PumpHouse;

impl CatalogueEntry for PumpHouse {
    fn slug(&self) -> &'static str {
        "pump_house"
    }
    fn name(&self) -> &'static str {
        "Pump House"
    }
    fn description(&self) -> &'static str {
        "Tall brick engine house with arched lit windows, a beam engine and a chimney."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body_h = 6.5_f32;
    let body_top = body_h;
    // Hero (−Z) front wall face sits at z = -2.5; detail rides proud of it.
    let front = -2.5_f32;

    let mut prims = vec![
        // Brick engine house — the root.
        prim(
            solid(cuboid_tapered([7.0, body_h, 5.0], 0.0, brick(BRICK_SOOT))),
            [0.0, body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Brass cornice band + pitched iron gable roof (ridge along X).
    prims.push(prim(
        solid(cuboid_tapered([7.2, 0.3, 5.2], 0.0, brass(BRASS))),
        [0.0, body_top, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [7.4, 1.7, 5.4],
            [0.0, 0.9],
            iron(IRON_DARK),
        )),
        [0.0, body_top + 0.85, 0.0],
        id_quat(),
    ));

    // Tall arched lit windows flanking the −Z hero front — pushed bright so
    // they read as lit even on the shadowed front face.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([1.3, 2.7, 0.15], 0.0, glass(GLASS_AMBER, 3.2)),
            [sx * 2.1, 2.65, front - 0.05],
            id_quat(),
        ));
        // Semicircle brick arch hood seating on the window head.
        prims.push(prim(
            solid(with_cut(
                torus(0.16, 0.75, brick(BRICK_SOOT)),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            )),
            [sx * 2.1, 3.75, front - 0.05],
            quat_x(-FRAC_PI_2),
        ));
    }

    // Beam-engine flywheel on the −Z wall: a heavy smooth rim, spokes, hub.
    let fw = [0.0_f32, 2.4, front - 0.35];
    prims.push(prim(
        solid(tube(0.95, 0.78, 0.24, 18, iron(IRON_DARK))),
        fw,
        quat_x(FRAC_PI_2),
    ));
    for k in 0..3 {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 1.84, 0.1], 0.0, iron(IRON_DARK))),
            fw,
            quat_z(k as f32 / 3.0 * PI),
        ));
    }
    prims.push(prim(
        solid(cylinder_tapered(0.2, 0.34, 10, 0.0, brass(BRASS))),
        [fw[0], fw[1], fw[2] - 0.05],
        quat_x(FRAC_PI_2),
    ));

    // Rocking beam projecting from the gable over the well, with a pump rod,
    // offset in X so the rod doesn't visually impale the centred flywheel.
    let beam_x = 1.5_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.6, 1.2, 0.6], 0.0, iron(IRON_DARK))),
        [beam_x, body_top + 0.5, front + 1.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.45, 0.45, 3.6], 0.0, iron(IRON_DARK))),
        [beam_x, body_top + 1.3, front - 0.6],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.1, 3.4, 8, 0.0, brass(BRASS))),
        [beam_x, body_top - 0.5, front - 2.4],
        id_quat(),
    ));
    // Stone well coping the pump rod descends into.
    prims.push(prim(
        solid(cuboid_tapered([1.2, 0.7, 1.2], 0.05, brick(BRICK_SOOT))),
        [beam_x, 0.35, front - 2.4],
        id_quat(),
    ));

    // Banded brick chimney with a hollow pot beside the house.
    let chimney_h = 8.0;
    prims.push(prim(
        solid(cylinder_tapered(
            0.7,
            chimney_h,
            12,
            0.16,
            brick(BRICK_SOOT),
        )),
        [4.2, chimney_h * 0.5, 1.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.1, 0.6, brass(BRASS))),
        [4.2, chimney_h - 0.8, 1.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(tube(0.45, 0.32, 0.8, 12, iron(IRON_DARK))),
        [4.2, chimney_h + 0.3, 1.2],
        id_quat(),
    ));

    // Hollow copper pipe along the side wall.
    prims.push(prim(
        solid(tube(0.18, 0.11, body_h - 0.4, 8, copper(COPPER_ORANGE))),
        [3.6, (body_h - 0.4) * 0.5, front + 0.4],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the engine chug, steam from the chimney pot.
    root.audio = fx::engine_chug();
    root.children
        .push(fx::steam_vent([4.2, chimney_h + 0.9, 1.2], 0x57EA_9009));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PumpHouse.build(""), "pump_house");
    }
}
