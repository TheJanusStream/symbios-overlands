//! Water tower — a Wild-West secondary. A timber tank on a braced four-leg
//! frame, banded with tin, capped by a conical roof and tapped by an iron
//! spout. A creak of old timber turns on the wind.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the first leg.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAP_TAN, IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, fx, iron, tin};

pub struct WaterTower;

impl CatalogueEntry for WaterTower {
    fn slug(&self) -> &'static str {
        "water_tower"
    }
    fn name(&self) -> &'static str {
        "Water Tower"
    }
    fn description(&self) -> &'static str {
        "Timber tank on a braced frame, banded with tin under a conical roof, with a spout."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let leg_h = 7.0_f32;
    let r = 1.8_f32;

    let mut prims = vec![
        // First leg — the root.
        prim(
            solid(cuboid_tapered([0.3, leg_h, 0.3], 0.0, clapboard(WOOD_RAW))),
            [-r, leg_h * 0.5, -r],
            id_quat(),
        ),
    ];
    for (sx, sz) in [(1.0_f32, -1.0_f32), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.3, leg_h, 0.3], 0.0, clapboard(WOOD_RAW))),
            [sx * r, leg_h * 0.5, sz * r],
            id_quat(),
        ));
    }
    // Cross-braces at two heights.
    for h in [leg_h * 0.35, leg_h * 0.75] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [2.0 * r, 0.12, 0.12],
                    0.0,
                    clapboard(WOOD_RAW),
                )),
                [0.0, h, sz * r],
                id_quat(),
            ));
        }
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.12, 0.12, 2.0 * r],
                    0.0,
                    clapboard(WOOD_RAW),
                )),
                [sx * r, h, 0.0],
                id_quat(),
            ));
        }
    }

    // Timber tank.
    prims.push(prim(
        solid(cylinder_tapered(2.4, 3.0, 16, 0.05, clapboard(CLAP_TAN))),
        [0.0, leg_h + 1.5, 0.0],
        id_quat(),
    ));
    // Tin hoop bands.
    for y in [leg_h + 0.6, leg_h + 2.4] {
        prims.push(prim(
            solid(torus(0.1, 2.3, tin(TIN_GREY))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }
    // Conical tin roof.
    prims.push(prim(
        solid(cone(2.6, 1.6, 16, tin(TIN_GREY))),
        [0.0, leg_h + 3.8, 0.0],
        id_quat(),
    ));
    // Iron spout hanging from the tank.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.2, 8, 0.0, iron(IRON_DARK))),
        [0.0, leg_h - 0.6, 2.3],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the old frame creaking on the wind.
    root.audio = fx::windmill_creak();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WaterTower.build(""), "water_tower");
    }
}
