//! Scrap boiler — a Steampunk *poor* secondary. A rusted-through boiler tank
//! leaning on a makeshift cradle, rigged with patched copper pipes and a bent
//! chimney. The improvised still of the soot-yard.
//!
//! The tank is a cylinder tipped on its side with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{COPPER_ORANGE, IRON_DARK, WOOD_BROWN, copper, iron, plank};

/// Heavy rust of the failing boiler.
const RUST: [f32; 3] = [0.45, 0.28, 0.16];

pub struct ScrapBoiler;

impl CatalogueEntry for ScrapBoiler {
    fn slug(&self) -> &'static str {
        "scrap_boiler"
    }
    fn name(&self) -> &'static str {
        "Scrap Boiler"
    }
    fn description(&self) -> &'static str {
        "Rusted boiler on a makeshift cradle, rigged with patched pipes and a bent chimney."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Rusted boiler tank — the root, laid along Z.
        prim(
            solid(cylinder_tapered(0.85, 2.6, 12, 0.0, iron(RUST))),
            [0.0, 1.1, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Makeshift plank cradle.
    for z in [-0.8_f32, 0.8] {
        prims.push(prim(
            solid(cuboid_tapered([1.6, 0.4, 0.4], 0.0, plank(WOOD_BROWN))),
            [0.0, 0.3, z],
            id_quat(),
        ));
    }

    // Bent iron chimney leaning off the top.
    prims.push(prim(
        solid(cylinder_tapered(0.2, 2.4, 8, 0.1, iron(IRON_DARK))),
        [0.3, 2.4, -0.8],
        quat_x(0.2),
    ));

    // Patched copper pipe rigged across the tank.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 2.2, 8, 0.0, copper(COPPER_ORANGE))),
        [0.9, 1.4, 0.0],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(torus(0.05, 0.2, copper(COPPER_ORANGE))),
        [0.9, 1.4, 1.1],
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
        assert_sanitize_stable(&ScrapBoiler.build(""), "scrap_boiler");
    }
}
