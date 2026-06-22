//! Barrel stack — a Medieval prop. A cluster of iron-hooped oak barrels of
//! ale and salt: three standing and one nestled on top, a tapped cask lying
//! on a timber cradle with a spigot and a waiting tankard, and a packing
//! crate — the stores of a tavern or market.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, WOOD_DARK, WOOD_OAK, iron, timber};

pub struct BarrelStack;

impl CatalogueEntry for BarrelStack {
    fn slug(&self) -> &'static str {
        "barrel_stack"
    }
    fn name(&self) -> &'static str {
        "Barrel Stack"
    }
    fn description(&self) -> &'static str {
        "Iron-hooped oak barrels with a tapped cask on a cradle, a spigot, a tankard and a crate."
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
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One standing oak barrel at `center`: a bellied staved drum with two iron
/// hoops, returned as a [`Generator`] for the assemble list.
fn barrel(center: [f32; 3], tone: [f32; 3]) -> Generator {
    let h = 1.0;
    let mut b = prim(
        solid(cylinder_tapered(0.4, h, 14, -0.12, timber(tone))),
        center,
        id_quat(),
    );
    for dy in [h * 0.3, -h * 0.3] {
        b.children.push(prim(
            torus(0.035, 0.41, iron(IRON_DARK)),
            [0.0, dy, 0.0],
            id_quat(),
        ));
    }
    b
}

fn build_tree() -> Generator {
    let ground_y = 0.5;
    // Three barrels standing in a tight triangle, one nestled on top.
    let mut prims = vec![barrel([0.0, ground_y, -0.5], WOOD_OAK)];
    prims.push(barrel([0.45, ground_y, 0.3], WOOD_DARK));
    prims.push(barrel([-0.45, ground_y, 0.3], WOOD_OAK));
    prims.push(barrel([0.0, ground_y + 1.0, -0.05], WOOD_DARK));

    // ── Tapped cask lying on a low timber cradle, off to the side ──
    let cradle_x = 1.7;
    // Two A-frame cradle saddles.
    for sz in [-0.45_f32, 0.45] {
        prims.push(prim(
            solid(cuboid_tapered([0.5, 0.34, 0.12], 0.5, timber(WOOD_DARK))),
            [cradle_x, 0.17, sz],
            id_quat(),
        ));
    }
    // The cask, lying on its side (axis along Z), bellied with hoops.
    let mut cask = prim(
        solid(cylinder_tapered(0.42, 1.1, 14, -0.12, timber(WOOD_OAK))),
        [cradle_x, 0.5, 0.0],
        quat_z(FRAC_PI_2),
    );
    for dy in [0.32_f32, -0.32] {
        cask.children.push(prim(
            torus(0.04, 0.43, iron(IRON_DARK)),
            [0.0, dy, 0.0],
            id_quat(),
        ));
    }
    prims.push(cask);
    // Iron spigot/tap on the −X-facing belly, near the bottom.
    prims.push(prim(
        solid(cone(0.06, 0.22, 8, iron(IRON_DARK))),
        [cradle_x - 0.5, 0.34, 0.0],
        quat_z(FRAC_PI_2),
    ));
    // A waiting tankard under the spigot.
    let mut tankard = prim(
        solid(cylinder_tapered(0.1, 0.2, 10, 0.0, iron(IRON_DARK))),
        [cradle_x - 0.62, 0.1, 0.0],
        id_quat(),
    );
    tankard.children.push(prim(
        torus(0.02, 0.1, iron(IRON_DARK)),
        [0.06, 0.0, 0.0],
        id_quat(),
    ));
    prims.push(tankard);

    // A packing crate beside the standing barrels.
    prims.push(prim(
        solid(cuboid_tapered([0.55, 0.55, 0.55], 0.0, timber(WOOD_DARK))),
        [-1.1, 0.275, -0.3],
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
        assert_sanitize_stable(&BarrelStack.build(""), "barrel_stack");
    }
}
