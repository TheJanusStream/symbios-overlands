//! Handcart — a Medieval prop. A two-wheel oak cart parked with its shafts
//! propped level on a stick, a couple of grain sacks in the bed: the
//! workaday transport of a market town.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLOTH_CREAM, IRON_DARK, WOOD_DARK, WOOD_OAK, cloth, iron, timber};

pub struct Handcart;

impl CatalogueEntry for Handcart {
    fn slug(&self) -> &'static str {
        "handcart"
    }
    fn name(&self) -> &'static str {
        "Handcart"
    }
    fn description(&self) -> &'static str {
        "Two-wheeled oak handcart with iron-shod wheels and a couple of grain sacks."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// An iron-shod spoked wheel at `center`, axle lying along Z. The iron
/// tyre is a child in the wheel's local frame so it follows the tilt.
fn wheel(center: [f32; 3]) -> Generator {
    let mut w = prim(
        solid(cylinder_tapered(0.5, 0.14, 12, 0.0, timber(WOOD_DARK))),
        center,
        quat_x(FRAC_PI_2),
    );
    // Iron tyre around the rim (rings the wheel's local Y = its axle).
    w.children.push(prim(
        torus(0.05, 0.5, iron(IRON_DARK)),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    // Iron hub.
    w.children.push(prim(
        solid(cylinder_tapered(0.12, 0.18, 8, 0.0, iron(IRON_DARK))),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w
}

fn build_tree() -> Generator {
    let bed_y = 0.72;

    let mut prims = vec![
        // Cart bed — the root.
        prim(
            solid(cuboid_tapered([2.0, 0.22, 1.1], 0.0, timber(WOOD_OAK))),
            [0.0, bed_y, 0.0],
            id_quat(),
        ),
    ];
    // Side boards.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([2.0, 0.4, 0.08], 0.0, timber(WOOD_OAK))),
            [0.0, bed_y + 0.3, sz * 0.5],
            id_quat(),
        ));
    }
    // Front and back boards.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.08, 0.4, 1.1], 0.0, timber(WOOD_OAK))),
            [sx * 1.0, bed_y + 0.3, 0.0],
            id_quat(),
        ));
    }

    // Axle and two wheels.
    prims.push(prim(
        solid(cylinder_tapered(0.06, 1.3, 8, 0.0, timber(WOOD_DARK))),
        [0.0, 0.5, 0.0],
        quat_x(FRAC_PI_2),
    ));
    prims.push(wheel([0.0, 0.5, 0.62]));
    prims.push(wheel([0.0, 0.5, -0.62]));

    // Two shafts reaching forward, with a prop stick holding them level.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.5, 0.09, 0.09], 0.0, timber(WOOD_OAK))),
            [1.7, bed_y - 0.05, sz * 0.4],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([0.09, 0.7, 0.09], 0.0, timber(WOOD_DARK))),
        [2.35, bed_y - 0.4, 0.0],
        id_quat(),
    ));

    // A couple of grain sacks in the bed.
    for (sx, sz) in [(-0.5_f32, 0.2_f32), (0.4, -0.2)] {
        prims.push(prim(
            cuboid_tapered([0.55, 0.55, 0.5], 0.35, cloth(CLOTH_CREAM, WOOD_OAK)),
            [sx, bed_y + 0.35, sz],
            id_quat(),
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
        assert_sanitize_stable(&Handcart.build(""), "handcart");
    }
}
