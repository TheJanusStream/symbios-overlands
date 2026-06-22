//! Coal hopper — a Steampunk prop. A riveted iron coal bunker on legs with a
//! funnel chute and a heap of coal beneath. Scatter clutter feeding the
//! furnaces.
//!
//! The chute is a cone flipped apex-down with a [`quat_x`] of π.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, glow, id_quat, prim, quat_x, solid, sphere, torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, GAUGE_AMBER, IRON_DARK, brass, iron};

/// Matte black of the heaped coal.
const COAL: [f32; 3] = [0.08, 0.08, 0.09];

pub struct CoalHopper;

impl CatalogueEntry for CoalHopper {
    fn slug(&self) -> &'static str {
        "coal_hopper"
    }
    fn name(&self) -> &'static str {
        "Coal Hopper"
    }
    fn description(&self) -> &'static str {
        "Riveted iron coal bunker on legs with a funnel chute and a heap of coal."
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
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Iron bunker body — the root, slightly battered (tapered).
        prim(
            solid(cuboid_tapered([1.6, 1.4, 1.6], 0.06, iron(IRON_DARK))),
            [0.0, 1.85, 0.0],
            id_quat(),
        ),
    ];
    // Brass rim band + a hinged top hatch.
    prims.push(prim(
        solid(cuboid_tapered([1.72, 0.22, 1.72], 0.0, brass(BRASS))),
        [0.0, 2.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.06, 0.62, brass(BRASS))),
        [0.0, 2.62, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.95, 0.1, 0.95], 0.0, iron(IRON_DARK))),
        [0.12, 2.66, 0.1],
        quat_x(0.16),
    ));

    // Four iron legs with a horizontal brace ring.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.15, 1.2, 0.15], 0.0, iron(IRON_DARK))),
                [sx * 0.65, 0.6, sz * 0.65],
                id_quat(),
            ));
        }
    }
    for sz in [-0.65_f32, 0.65] {
        prims.push(prim(
            solid(cuboid_tapered([1.45, 0.1, 0.1], 0.0, iron(IRON_DARK))),
            [0.0, 0.5, sz],
            id_quat(),
        ));
    }
    for sx in [-0.65_f32, 0.65] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.1, 1.45], 0.0, iron(IRON_DARK))),
            [sx, 0.5, 0.0],
            id_quat(),
        ));
    }

    // Riveted seam straps + brass stud rows on the visible faces, and a small
    // lit gauge on the −Z hero face — lift it above a plain iron silo.
    for (cx, cz, vert) in [
        (-0.42_f32, -0.81_f32, true),
        (0.42, -0.81, true),
        (0.81, 0.3, false),
    ] {
        let size = if vert {
            [0.12, 1.3, 0.05]
        } else {
            [0.05, 1.3, 0.12]
        };
        prims.push(prim(
            solid(cuboid_tapered(size, 0.0, iron(IRON_DARK))),
            [cx, 1.85, cz],
            id_quat(),
        ));
        for k in 0..4 {
            let y = 1.32 + k as f32 * 0.36;
            let pos = if vert {
                [cx, y, cz - 0.03]
            } else {
                [cx - 0.03, y, cz]
            };
            prims.push(prim(solid(sphere(0.05, 6, brass(BRASS))), pos, id_quat()));
        }
    }
    prims.push(prim(
        solid(cuboid_tapered([0.42, 0.42, 0.06], 0.0, iron(IRON_DARK))),
        [0.0, 2.05, -0.83],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.28, 0.28, 0.05], 0.0, glow(GAUGE_AMBER, 2.0)),
        [0.0, 2.05, -0.86],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.03, 0.2, brass(BRASS))),
        [0.0, 2.05, -0.88],
        quat_x(FRAC_PI_2),
    ));

    // Funnel chute under the body, apex down, with a hollow discharge spout.
    prims.push(prim(
        solid(cone(0.8, 1.05, 12, iron(IRON_DARK))),
        [0.0, 1.0, 0.0],
        quat_x(PI),
    ));
    prims.push(prim(
        solid(tube(0.17, 0.1, 0.62, 8, iron(IRON_DARK))),
        [0.0, 0.28, 0.0],
        id_quat(),
    ));

    // Heap of heaped coal lumps; the largest sits directly under the spout so
    // the discharge visibly feeds the pile (no gap to empty ground).
    for (cx, cz, s) in [
        (0.0_f32, 0.0_f32, 0.62_f32),
        (0.42, 0.2, 0.3),
        (-0.32, -0.26, 0.34),
        (0.18, -0.34, 0.26),
        (-0.28, 0.3, 0.24),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([s, s * 0.55, s], 0.35, iron(COAL))),
            [cx, s * 0.28, cz],
            quat_x(cx),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CoalHopper.build(""), "coal_hopper");
    }
}
