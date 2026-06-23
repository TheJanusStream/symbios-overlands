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
        // Marble box body.
        prim(
            solid(cuboid_tapered([1.0, box_h, 1.0], 0.0, marble(MARBLE))),
            [0.0, box_h * 0.5, 0.0],
            id_quat(),
        ),
        // Proud base foot.
        prim(
            solid(cuboid_tapered(
                [1.1, 0.08, 1.1],
                0.0,
                marble([0.8, 0.79, 0.76]),
            )),
            [0.0, 0.04, 0.0],
            id_quat(),
        ),
        // Capping rim, oversailing the body.
        prim(
            solid(cuboid_tapered(
                [1.14, 0.1, 1.14],
                0.0,
                marble([0.8, 0.79, 0.76]),
            )),
            [0.0, box_h, 0.0],
            id_quat(),
        ),
        // Carved relief panel proud of the front (-Z) face.
        prim(
            cuboid_tapered([0.62, 0.3, 0.04], 0.0, marble([0.8, 0.79, 0.76])),
            [0.0, 0.26, -0.52],
            id_quat(),
        ),
        // Soil.
        prim(
            cuboid_tapered([0.86, 0.1, 0.86], 0.0, foliage([0.2, 0.13, 0.08])),
            [0.0, box_h + 0.02, 0.0],
            id_quat(),
        ),
    ];

    // Corner pilasters, proud of the body faces.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, box_h, 0.1], 0.0, marble(MARBLE))),
            [sx * 0.48, box_h * 0.5, sz * 0.48],
            id_quat(),
        ));
    }

    // Clipped topiary — a fuller mound of overlapping green spheres.
    for (dx, dz, r) in [
        (0.0_f32, 0.0_f32, 0.44_f32),
        (0.28, 0.2, 0.32),
        (-0.26, -0.2, 0.32),
        (0.24, -0.24, 0.28),
        (-0.22, 0.26, 0.28),
    ] {
        prims.push(prim(
            sphere(r, 3, foliage(FOLIAGE_GREEN)),
            [dx, crown_y, dz],
            id_quat(),
        ));
    }

    // Trailing greenery spilling over the front rim.
    for (dx, dy, dz, r) in [
        (0.2_f32, 0.05_f32, -0.52_f32, 0.18_f32),
        (-0.25, 0.0, -0.5, 0.16),
    ] {
        prims.push(prim(
            sphere(r, 3, foliage([0.22, 0.45, 0.18])),
            [dx, box_h + dy, dz],
            id_quat(),
        ));
    }

    // Blooms in varied colours.
    let blooms = [
        (0.2_f32, 0.25_f32, [0.86_f32, 0.3_f32, 0.32_f32]),
        (-0.18, -0.1, [0.9, 0.78, 0.3]),
        (0.05, -0.28, [0.7, 0.45, 0.85]),
        (0.3, -0.05, [0.92, 0.5, 0.2]),
        (-0.3, 0.15, [0.9, 0.88, 0.85]),
    ];
    for (dx, dz, color) in blooms {
        prims.push(prim(
            sphere(0.1, 3, foliage(color)),
            [dx, crown_y + 0.34, dz],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
