//! Shipping containers — an Industrial-Park prop. A few corrugated steel
//! intermodal containers stacked and offset, in mismatched faded liveries,
//! with cast corner blocks and locking-rod door ends.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp4, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{CONTAINER_BLUE, CONTAINER_GREEN, CONTAINER_RED, CONTAINER_RUST, cladding, tank_steel};

pub struct ShippingContainers;

impl CatalogueEntry for ShippingContainers {
    fn slug(&self) -> &'static str {
        "shipping_containers"
    }
    fn name(&self) -> &'static str {
        "Shipping Containers"
    }
    fn description(&self) -> &'static str {
        "Corrugated steel intermodal containers stacked in mismatched liveries."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let cw = 6.0_f32;
    let ch = 2.6_f32;
    let cd = 2.5_f32;
    let cast = tank_steel([0.16, 0.16, 0.17]);

    // One container = a corrugated box with cast corner blocks, a locking-rod
    // door end on +X, and roof ribs. Returned as a rigid subtree so a yawed
    // copy keeps its detail aligned.
    let container = move |pos: [f32; 3], rot: Fp4, color: [f32; 3]| -> Generator {
        let mut c = prim(
            solid(cuboid_tapered([cw, ch, cd], 0.0, cladding(color))),
            pos,
            rot,
        );
        // Eight cast corner blocks (the ISO signature).
        for sx in [-1.0_f32, 1.0] {
            for sy in [-1.0_f32, 1.0] {
                for sz in [-1.0_f32, 1.0] {
                    c.children.push(prim(
                        solid(cuboid_tapered([0.34, 0.34, 0.34], 0.0, cast.clone())),
                        [
                            sx * (cw * 0.5 - 0.15),
                            sy * (ch * 0.5 - 0.15),
                            sz * (cd * 0.5 - 0.15),
                        ],
                        id_quat(),
                    ));
                }
            }
        }
        // Door end on +X: two recessed leaves, four vertical locking rods, and
        // a pair of handles.
        let xe = cw * 0.5 + 0.04;
        let door = [color[0] * 0.85, color[1] * 0.85, color[2] * 0.85];
        for dz in [-0.6_f32, 0.6] {
            c.children.push(prim(
                cuboid_tapered([0.05, ch - 0.4, cd * 0.44], 0.0, tank_steel(door)),
                [xe, 0.0, dz],
                id_quat(),
            ));
        }
        for dz in [-0.92_f32, -0.32, 0.32, 0.92] {
            c.children.push(prim(
                solid(cylinder_tapered(
                    0.05,
                    ch - 0.5,
                    6,
                    0.0,
                    tank_steel([0.32, 0.32, 0.34]),
                )),
                [xe + 0.06, 0.0, dz],
                id_quat(),
            ));
        }
        for dz in [-0.32_f32, 0.32] {
            c.children.push(prim(
                cuboid_tapered([0.1, 0.28, 0.06], 0.0, tank_steel([0.32, 0.32, 0.34])),
                [xe + 0.12, 0.1, dz],
                id_quat(),
            ));
        }
        // Raised roof ribs.
        for rx in [-1.7_f32, 0.0, 1.7] {
            c.children.push(prim(
                cuboid_tapered([0.12, 0.08, cd - 0.4], 0.0, cladding(door)),
                [rx, ch * 0.5 + 0.02, 0.0],
                id_quat(),
            ));
        }
        c
    };

    // Bottom row (the first container is the root) + an offset top one.
    let mut prims = vec![container([0.0, ch * 0.5, -1.35], id_quat(), CONTAINER_RED)];
    prims.push(container([0.0, ch * 0.5, 1.35], id_quat(), CONTAINER_BLUE));
    prims.push(container([0.6, ch * 1.5, -0.6], id_quat(), CONTAINER_GREEN));
    // A lone container set apart, yawed so its door end faces the -Z front.
    prims.push(container(
        [-3.6, ch * 0.5, 3.4],
        quat_y(FRAC_PI_2),
        CONTAINER_RUST,
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ShippingContainers.build(""), "shipping_containers");
    }
}
