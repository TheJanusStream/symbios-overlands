//! Cog scrap — a Steampunk *poor* prop. A heap of rusted gears, bent rods and
//! scrap iron. The cast-offs of the soot-yard.
//!
//! One gear leans on its edge with a [`quat_x`] of π/2; a bent rod lies
//! across the heap.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::iron;

/// Heavy rust of the scrap pile.
const RUST: [f32; 3] = [0.45, 0.28, 0.16];
const DARK_IRON: [f32; 3] = [0.24, 0.22, 0.20];

pub struct CogScrap;

impl CatalogueEntry for CogScrap {
    fn slug(&self) -> &'static str {
        "cog_scrap"
    }
    fn name(&self) -> &'static str {
        "Cog Scrap"
    }
    fn description(&self) -> &'static str {
        "A heap of rusted gears, bent rods and scrap iron."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Rusted gear lying flat — the root.
        prim(
            solid(cylinder_tapered(0.6, 0.16, 14, 0.0, iron(RUST))),
            [0.0, 0.08, 0.0],
            id_quat(),
        ),
    ];

    // A smaller gear piled on top.
    prims.push(prim(
        solid(cylinder_tapered(0.4, 0.14, 12, 0.0, iron(DARK_IRON))),
        [0.3, 0.25, 0.15],
        id_quat(),
    ));
    // A gear leaning on its edge.
    prims.push(prim(
        solid(cylinder_tapered(0.5, 0.15, 14, 0.0, iron(RUST))),
        [-0.5, 0.45, -0.2],
        quat_x(FRAC_PI_2),
    ));
    // A bent rod lying across the heap.
    prims.push(prim(
        solid(cylinder_tapered(0.06, 1.6, 6, 0.0, iron(DARK_IRON))),
        [0.2, 0.4, -0.3],
        quat_x(FRAC_PI_2),
    ));
    // A scrap plate.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.1, 0.5], 0.0, iron(RUST))),
        [0.5, 0.12, -0.4],
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
        assert_sanitize_stable(&CogScrap.build(""), "cog_scrap");
    }
}
