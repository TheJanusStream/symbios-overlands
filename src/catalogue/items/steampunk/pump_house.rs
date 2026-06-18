//! Pump house — a Steampunk secondary. A tall brick engine house with arched
//! lit windows, a beam engine's rocking beam projecting from the gable, a
//! banded chimney and copper pipework. The waterworks of the quarter.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the engine house.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, torus,
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

    let mut prims = vec![
        // Brick engine house — the root.
        prim(
            solid(cuboid_tapered([7.0, body_h, 5.0], 0.0, brick(BRICK_SOOT))),
            [0.0, body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Pitched iron roof.
    prims.push(prim(
        solid(cuboid_tapered([7.4, 1.4, 5.4], 0.5, iron(IRON_DARK))),
        [0.0, body_top + 0.7, 0.0],
        id_quat(),
    ));

    // Tall arched lit windows on the +Z front.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([1.4, 3.0, 0.15], 0.0, glass(GLASS_AMBER, 1.3)),
            [sx * 1.8, 2.4, 2.55],
            id_quat(),
        ));
    }

    // Beam engine: an iron beam projecting from the gable, with a hanging rod.
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.5, 4.0], 0.0, iron(IRON_DARK))),
        [0.0, body_top + 0.4, 2.6],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.1, 2.4, 8, 0.0, brass(BRASS))),
        [0.0, body_top - 0.8, 4.2],
        id_quat(),
    ));

    // Banded brick chimney beside the house.
    let chimney_h = 8.0;
    prims.push(prim(
        solid(cylinder_tapered(
            0.7,
            chimney_h,
            12,
            0.16,
            brick(BRICK_SOOT),
        )),
        [4.2, chimney_h * 0.5, -1.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.1, 0.62, brass(BRASS))),
        [4.2, chimney_h - 0.6, -1.0],
        id_quat(),
    ));

    // Copper pipe along the side wall.
    prims.push(prim(
        solid(cylinder_tapered(
            0.18,
            body_h,
            8,
            0.0,
            copper(COPPER_ORANGE),
        )),
        [3.4, body_h * 0.5, 2.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the engine chug, steam from the chimney.
    root.audio = fx::engine_chug();
    root.children
        .push(fx::steam_vent([4.2, chimney_h + 0.3, -1.0], 0x57EA_9009));
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
