//! Garden bed — a low stone-kerbed planting bed of greenery and blooms. An
//! escalation-Calm scatter prop: cultivated ground signals a settlement
//! tended rather than fought over, in any setting.

use crate::catalogue::items::util::{cone, cuboid_tapered, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{FOLIAGE_GREEN, STONE, foliage, stone};

pub struct GardenBed;

impl CatalogueEntry for GardenBed {
    fn slug(&self) -> &'static str {
        "garden_bed"
    }
    fn name(&self) -> &'static str {
        "Garden Bed"
    }
    fn description(&self) -> &'static str {
        "Low stone-kerbed bed of greenery and blooms."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Calm)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.3,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let rim_h = 0.28;
    let mut prims = vec![
        // Stone kerb.
        prim(
            solid(cuboid_tapered([1.5, rim_h, 1.1], 0.0, stone(STONE))),
            [0.0, rim_h * 0.5, 0.0],
            id_quat(),
        ),
        // Soil.
        prim(
            cuboid_tapered([1.3, 0.1, 0.9], 0.0, foliage([0.2, 0.13, 0.08])),
            [0.0, rim_h + 0.02, 0.0],
            id_quat(),
        ),
    ];

    // Larger corner kerb stones, proud above the rim.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.24, 0.4, 0.22],
                0.0,
                stone([0.55, 0.53, 0.48]),
            )),
            [sx * 0.66, 0.2, sz * 0.46],
            id_quat(),
        ));
    }

    // A small flowering shrub in the centre — two stacked clumps.
    prims.push(prim(
        sphere(0.34, 3, foliage([0.16, 0.4, 0.15])),
        [0.0, rim_h + 0.34, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.24, 3, foliage(FOLIAGE_GREEN)),
        [0.0, rim_h + 0.62, 0.0],
        id_quat(),
    ));

    // Naturalistic leafy clumps.
    for (dx, dz, r) in [
        (-0.42_f32, 0.05_f32, 0.28_f32),
        (0.42, 0.12, 0.26),
        (0.18, -0.2, 0.24),
        (-0.2, -0.18, 0.22),
    ] {
        prims.push(prim(
            sphere(r, 3, foliage(FOLIAGE_GREEN)),
            [dx, rim_h + 0.3, dz],
            id_quat(),
        ));
    }

    // Upright grass tufts.
    for (dx, dz) in [(-0.55_f32, -0.2_f32), (0.55, 0.2), (0.0, 0.28)] {
        prims.push(prim(
            cone(0.07, 0.34, 5, foliage([0.3, 0.5, 0.2])),
            [dx, rim_h + 0.3, dz],
            id_quat(),
        ));
    }

    // Wildflowers scattered in varied colours.
    let blooms = [
        (-0.5_f32, 0.18_f32, [0.9_f32, 0.3_f32, 0.3_f32]),
        (0.46, -0.12, [0.95, 0.8, 0.3]),
        (0.12, 0.26, [0.85, 0.85, 0.9]),
        (-0.12, -0.3, [0.7, 0.4, 0.85]),
        (0.56, 0.3, [0.9, 0.5, 0.2]),
        (-0.34, 0.28, [0.95, 0.55, 0.75]),
    ];
    for (dx, dz, color) in blooms {
        prims.push(prim(
            sphere(0.08, 3, foliage(color)),
            [dx, rim_h + 0.48, dz],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
