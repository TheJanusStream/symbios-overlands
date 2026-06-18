//! Airship dock — a Steampunk secondary. A tapering iron mooring mast with a
//! brass docking ring and a plank gangway, a small dirigible moored alongside
//! — a tapered copper envelope over an iron gondola. The aerial harbour of
//! the works.
//!
//! The envelope is a tapered cylinder tipped on its side (a single
//! [`quat_x`] of π/2 lays the Y axis along Z), narrowing to a nose.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the mast base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, COPPER_ORANGE, IRON_DARK, WOOD_BROWN, brass, copper, fx, iron, plank};

pub struct AirshipDock;

impl CatalogueEntry for AirshipDock {
    fn slug(&self) -> &'static str {
        "airship_dock"
    }
    fn name(&self) -> &'static str {
        "Airship Dock"
    }
    fn description(&self) -> &'static str {
        "Iron mooring mast with a docking ring and a small dirigible moored alongside."
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
            clearance: 8.0,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;
    let mast_h = 10.0_f32;
    let mast_top = base_h + mast_h;

    let mut prims = vec![
        // Iron base — the root.
        prim(
            solid(cuboid_tapered([4.0, base_h, 4.0], 0.0, iron(IRON_DARK))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Tapering lattice mast.
    prims.push(prim(
        solid(cuboid_tapered([2.0, mast_h, 2.0], 0.45, iron(IRON_DARK))),
        [0.0, base_h + mast_h * 0.5, 0.0],
        id_quat(),
    ));
    // Brass docking ring at the top.
    prims.push(prim(
        solid(torus(0.14, 0.9, brass(BRASS))),
        [0.0, mast_top + 0.2, 0.0],
        id_quat(),
    ));
    // Plank gangway reaching out to the gondola.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.2, 1.0], 0.0, plank(WOOD_BROWN))),
        [2.0, mast_top - 2.5, 3.0],
        id_quat(),
    ));

    // Moored dirigible: tapered copper envelope laid along Z.
    let ship_z = 5.2_f32;
    let ship_y = 9.2_f32;
    prims.push(prim(
        solid(cylinder_tapered(1.8, 9.0, 14, 0.45, copper(COPPER_ORANGE))),
        [0.0, ship_y, ship_z],
        quat_x(FRAC_PI_2),
    ));
    // Brass nose cap.
    prims.push(prim(
        solid(torus(0.2, 0.6, brass(BRASS))),
        [0.0, ship_y, ship_z - 4.3],
        quat_x(FRAC_PI_2),
    ));
    // Iron gondola slung beneath.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 0.9, 1.4], 0.1, iron(IRON_DARK))),
        [0.0, ship_y - 1.8, ship_z],
        id_quat(),
    ));
    // Tail fins.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 1.4, 1.4], 0.4, copper(COPPER_ORANGE))),
            [sx * 0.4, ship_y, ship_z + 4.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([1.4, 0.1, 1.4], 0.4, copper(COPPER_ORANGE))),
        [0.0, ship_y + 0.4, ship_z + 4.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: steam venting from the mast head.
    root.children
        .push(fx::steam_vent([0.0, mast_top + 0.4, 0.0], 0x57EA_D0C2));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&AirshipDock.build(""), "airship_dock");
    }
}
