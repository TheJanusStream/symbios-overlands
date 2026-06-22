//! Handcart — a Medieval prop. A two-wheel oak cart with iron-shod spoked
//! wheels, parked with its shafts propped level on a stick, loaded with grain
//! sacks, a small ale cask and a wicker basket: the workaday transport of a
//! market town.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_3};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid, torus,
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
        "Two-wheeled oak handcart with iron-shod spoked wheels, grain sacks and an ale cask."
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

/// An iron-shod spoked wheel at `center`, axle lying along Z. The hub is the
/// subtree root (rotated so its axis runs along Z); the wooden rim, iron
/// tyre and six spokes are children in the wheel's local frame.
fn wheel(center: [f32; 3]) -> Generator {
    let mut w = prim(
        solid(cylinder_tapered(0.13, 0.2, 8, 0.0, iron(IRON_DARK))),
        center,
        quat_x(FRAC_PI_2),
    );
    // Wooden rim + proud iron tyre (both ring the wheel's local Y = its axle).
    w.children.push(prim(
        torus(0.07, 0.48, timber(WOOD_DARK)),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w.children.push(prim(
        torus(0.04, 0.52, iron(IRON_DARK)),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    // Six spokes = three diameter bars in the wheel plane (local XZ).
    for k in 0..3 {
        w.children.push(prim(
            solid(cuboid_tapered([0.92, 0.06, 0.06], 0.0, timber(WOOD_OAK))),
            [0.0, 0.0, 0.0],
            quat_y(k as f32 * FRAC_PI_3),
        ));
    }
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

    // Axle and two spoked wheels.
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

    // A load: two grain sacks, a small ale cask and a wicker basket.
    for (sx, sz) in [(-0.55_f32, 0.22_f32), (0.45, -0.18)] {
        prims.push(prim(
            cuboid_tapered([0.55, 0.55, 0.5], 0.35, cloth(CLOTH_CREAM, WOOD_OAK)),
            [sx, bed_y + 0.35, sz],
            id_quat(),
        ));
    }
    // Ale cask lying on its side across the bed (axis along Z).
    let mut cask = prim(
        solid(cylinder_tapered(0.3, 0.7, 12, -0.12, timber(WOOD_DARK))),
        [-0.1, bed_y + 0.4, 0.28],
        quat_x(FRAC_PI_2),
    );
    for dy in [0.2_f32, -0.2] {
        cask.children.push(prim(
            torus(0.03, 0.31, iron(IRON_DARK)),
            [0.0, dy, 0.0],
            id_quat(),
        ));
    }
    prims.push(cask);
    // Wicker basket.
    prims.push(prim(
        solid(cylinder_tapered(0.22, 0.34, 10, -0.08, timber(WOOD_OAK))),
        [0.55, bed_y + 0.37, 0.3],
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
        assert_sanitize_stable(&Handcart.build(""), "handcart");
    }
}
