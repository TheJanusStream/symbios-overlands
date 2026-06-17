//! Planter — a marble box of neatly-clipped greenery and a few blooms. A
//! prosperity-Rich scatter prop: ornamental landscaping signals upkeep and
//! disposable means in any setting.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{FOLIAGE_GREEN, MARBLE, foliage, marble};

pub struct Planter;

impl CatalogueEntry for Planter {
    fn slug(&self) -> &'static str {
        "planter"
    }
    fn name(&self) -> &'static str {
        "Planter"
    }
    fn description(&self) -> &'static str {
        "Marble box of clipped greenery dotted with blooms."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.1,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let box_h = 0.5;

    let crown_y = box_h + 0.42;
    let mut prims = vec![
        // Marble box + capping rim.
        prim(
            solid(cuboid_tapered([1.0, box_h, 1.0], 0.0, marble(MARBLE))),
            [0.0, box_h * 0.5, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [1.12, 0.1, 1.12],
                0.0,
                marble([0.8, 0.79, 0.76]),
            )),
            [0.0, box_h, 0.0],
            id_quat(),
        ),
        // Soil.
        prim(
            cuboid_tapered([0.86, 0.1, 0.86], 0.0, foliage([0.2, 0.13, 0.08])),
            [0.0, box_h + 0.02, 0.0],
            id_quat(),
        ),
    ];

    // Clipped hedge: overlapping green spheres.
    for (dx, dz, r) in [
        (0.0_f32, 0.0_f32, 0.42_f32),
        (0.28, 0.18, 0.3),
        (-0.26, -0.2, 0.3),
    ] {
        prims.push(prim(
            sphere(r, 3, foliage(FOLIAGE_GREEN)),
            [dx, crown_y, dz],
            id_quat(),
        ));
    }

    // A few blooms.
    let blooms = [
        (0.2_f32, 0.25_f32, [0.86_f32, 0.3_f32, 0.32_f32]),
        (-0.18, -0.1, [0.9, 0.78, 0.3]),
        (0.05, -0.28, [0.7, 0.45, 0.85]),
    ];
    for (dx, dz, color) in blooms {
        prims.push(prim(
            sphere(0.1, 3, foliage(color)),
            [dx, crown_y + 0.32, dz],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
