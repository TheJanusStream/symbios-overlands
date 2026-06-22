//! Water tower — a Wild-West secondary. A timber tank on a braced four-leg
//! frame, banded with tin, capped by a conical roof and tapped by an iron
//! spout. A creak of old timber turns on the wind.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the first leg.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, sphere,
    torus,
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
    // Diagonal X cross-braces on all four faces — the braced-tower signature.
    let span_h = leg_h - 0.6;
    let theta = span_h.atan2(2.0 * r);
    let brace_len = (span_h * span_h + 4.0 * r * r).sqrt();
    for sz in [-1.0_f32, 1.0] {
        for s in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [brace_len, 0.14, 0.14],
                    0.0,
                    clapboard(WOOD_RAW),
                )),
                [0.0, leg_h * 0.5, sz * r],
                quat_z(s * theta),
            ));
        }
    }
    for sx in [-1.0_f32, 1.0] {
        for s in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.14, 0.14, brace_len],
                    0.0,
                    clapboard(WOOD_RAW),
                )),
                [sx * r, leg_h * 0.5, 0.0],
                quat_x(s * theta),
            ));
        }
    }
    // Top tie-frame the tank sits on.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [2.0 * r + 0.3, 0.14, 0.14],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [0.0, leg_h - 0.1, sz * r],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.14, 0.14, 2.0 * r + 0.3],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [sx * r, leg_h - 0.1, 0.0],
            id_quat(),
        ));
    }

    // Timber tank.
    prims.push(prim(
        solid(cylinder_tapered(2.4, 3.0, 16, 0.04, clapboard(CLAP_TAN))),
        [0.0, leg_h + 1.5, 0.0],
        id_quat(),
    ));
    // Riveted tin hoop bands up the staves.
    for y in [leg_h + 0.4, leg_h + 1.2, leg_h + 2.0, leg_h + 2.7] {
        prims.push(prim(
            solid(torus(0.09, 2.42, tin(TIN_GREY))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }
    // Conical tin roof + iron finial.
    prims.push(prim(
        solid(cone(2.7, 1.7, 16, tin(TIN_GREY))),
        [0.0, leg_h + 3.85, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(0.22, 3, iron(IRON_DARK))),
        [0.0, leg_h + 4.8, 0.0],
        id_quat(),
    ));
    // Iron downspout + handwheel valve on the −Z (front) face.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.6, 8, 0.0, iron(IRON_DARK))),
        [0.0, leg_h - 0.4, -2.5],
        quat_x(-0.35),
    ));
    prims.push(prim(
        solid(torus(0.05, 0.22, iron(IRON_DARK))),
        [0.0, leg_h + 0.3, -2.45],
        quat_x(FRAC_PI_2),
    ));
    // Access ladder up the front-left leg.
    let lx = -r;
    let lz = -r - 0.18;
    for sx in [-0.18_f32, 0.18] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.06, leg_h - 0.4, 0.06],
                0.0,
                iron(IRON_DARK),
            )),
            [lx + sx, (leg_h - 0.4) * 0.5 + 0.2, lz],
            id_quat(),
        ));
    }
    let rungs = 9;
    for i in 0..rungs {
        let y = 0.5 + i as f32 / (rungs - 1) as f32 * (leg_h - 1.2);
        prims.push(prim(
            solid(cuboid_tapered([0.42, 0.05, 0.05], 0.0, iron(IRON_DARK))),
            [lx, y, lz],
            id_quat(),
        ));
    }

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
