//! Wagon — a Wild-West prop. A prairie schooner: a plank bed with raised
//! sideboards under an arched canvas bonnet on bow ribs, rolling on
//! spoked iron-tyred wheels with a draft tongue out front. Scatter clutter
//! parked about the town.
//!
//! Wheels run their axles along Z via a [`quat_x`] of π/2; the bonnet is a
//! half-cylinder ([`with_cut`] path-cut) laid along the bed's length, its bow
//! ribs half-toruses yawed to arch across the width.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_y, quat_z,
    solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CANVAS_TAN, IRON_DARK, WOOD_RAW, canvas, clapboard, iron};

pub struct Wagon;

impl CatalogueEntry for Wagon {
    fn slug(&self) -> &'static str {
        "wagon"
    }
    fn name(&self) -> &'static str {
        "Wagon"
    }
    fn description(&self) -> &'static str {
        "Covered wagon: a plank bed under an arched canvas tilt on iron-tyred wheels."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let bed_top = 1.15_f32;

    let mut prims = vec![
        // Plank bed — the root (running along X).
        prim(
            solid(cuboid_tapered([3.2, 0.5, 1.4], 0.0, clapboard(WOOD_RAW))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
    ];
    // Raised sideboards and end boards on the bed.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([3.3, 0.42, 0.08], 0.0, clapboard(WOOD_RAW))),
            [0.0, bed_top + 0.05, sz * 0.7],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.08, 0.42, 1.4], 0.0, clapboard(WOOD_RAW))),
            [sx * 1.65, bed_top + 0.05, 0.0],
            id_quat(),
        ));
    }

    // Arched canvas bonnet — a half-cylinder laid along the bed's length.
    prims.push(prim(
        solid(with_cut(
            cylinder_tapered(0.8, 3.0, 16, 0.0, canvas(CANVAS_TAN)),
            [0.5, 1.0],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, bed_top + 0.1, 0.0],
        quat_z(FRAC_PI_2),
    ));
    // Dark bow ribs arching across the width at each end + the middle.
    for x in [-1.45_f32, 0.0, 1.45] {
        prims.push(prim(
            solid(with_cut(
                torus(0.04, 0.82, clapboard([0.32, 0.24, 0.15])),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            )),
            [x, bed_top + 0.1, 0.0],
            quat_mul(quat_y(FRAC_PI_2), quat_x(-FRAC_PI_2)),
        ));
    }

    // Driver's seat plank at the front of the bed.
    prims.push(prim(
        solid(cuboid_tapered([0.45, 0.12, 1.2], 0.0, clapboard(WOOD_RAW))),
        [1.45, bed_top + 0.18, 0.0],
        id_quat(),
    ));

    // Four spoked iron-tyred wheels (axles along Z; rear larger than front).
    for (x, rad) in [(-1.25_f32, 0.6_f32), (1.25, 0.45)] {
        for sz in [-1.0_f32, 1.0] {
            let c = [x, rad, sz * 0.78];
            // Iron tyre.
            prims.push(prim(
                solid(torus(0.06, rad, iron(IRON_DARK))),
                c,
                quat_x(FRAC_PI_2),
            ));
            // Hub.
            prims.push(prim(
                solid(cylinder_tapered(0.1, 0.24, 8, 0.0, iron(IRON_DARK))),
                c,
                quat_x(FRAC_PI_2),
            ));
            // Wooden spokes — three diameter bars = six spokes.
            for k in 0..3 {
                let a = k as f32 / 3.0 * std::f32::consts::PI;
                prims.push(prim(
                    solid(cuboid_tapered(
                        [0.04, 2.0 * rad * 0.92, 0.04],
                        0.0,
                        clapboard(WOOD_RAW),
                    )),
                    c,
                    quat_z(a),
                ));
            }
        }
    }

    // Draft tongue + singletree out the front.
    prims.push(prim(
        solid(cuboid_tapered([1.5, 0.12, 0.12], 0.0, clapboard(WOOD_RAW))),
        [2.3, 0.55, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.12, 0.12, 0.95], 0.0, clapboard(WOOD_RAW))),
        [3.0, 0.55, 0.0],
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
        assert_sanitize_stable(&Wagon.build(""), "wagon");
    }
}
