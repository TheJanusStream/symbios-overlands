//! Hydroponics — a Space-Outpost secondary. A glazed barrel-vault grow module
//! over crop racks, lit pink by grow-lights. The food module of the base; its
//! grow-lights are emissive trim the ruin pass can darken.
//!
//! The vault is a glass cylinder tipped on its side with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_CYAN, GROW_PINK, HULL_WHITE, glass, hull, painted};

/// Crop-row green inside the module.
const CROP: [f32; 3] = [0.32, 0.5, 0.24];

pub struct Hydroponics;

impl CatalogueEntry for Hydroponics {
    fn slug(&self) -> &'static str {
        "hydroponics"
    }
    fn name(&self) -> &'static str {
        "Hydroponics"
    }
    fn description(&self) -> &'static str {
        "Glazed barrel-vault grow module over crop racks, lit by pink grow-lights."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.4_f32;
    let axis_y = base_h + 1.6_f32;

    let mut prims = vec![
        // Hull base — the root.
        prim(
            solid(cuboid_tapered([6.0, base_h, 4.0], 0.0, hull(HULL_WHITE))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Glazed barrel vault laid along Z.
    prims.push(prim(
        solid(cylinder_tapered(1.7, 5.4, 16, 0.0, glass(GLASS_CYAN, 1.0))),
        [0.0, axis_y, 0.0],
        quat_x(FRAC_PI_2),
    ));
    // Hull end caps.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([3.4, 3.0, 0.3], 0.0, hull(HULL_WHITE))),
            [0.0, axis_y - 0.1, sz * 2.7],
            id_quat(),
        ));
    }

    // Crop racks + grow-light strips inside (emissive).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.6, 0.8, 4.6], 0.0, painted(CROP))),
            [sx * 0.8, base_h + 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.5, 0.1, 4.6], 0.0, glow(GROW_PINK, 2.5)),
            [sx * 0.8, axis_y + 0.6, 0.0],
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
        assert_sanitize_stable(&Hydroponics.build(""), "hydroponics");
    }
}
