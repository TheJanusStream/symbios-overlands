//! Garden bed — a low stone-kerbed planting bed of greenery and blooms. An
//! escalation-Calm scatter prop: cultivated ground signals a settlement
//! tended rather than fought over, in any setting.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid, sphere};
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
    let rim_h = 0.3;
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

    // Leafy clumps.
    for (dx, dz, r) in [
        (-0.4_f32, 0.0_f32, 0.3_f32),
        (0.4, 0.1, 0.28),
        (0.0, -0.15, 0.26),
    ] {
        prims.push(prim(
            sphere(r, 3, foliage(FOLIAGE_GREEN)),
            [dx, rim_h + 0.32, dz],
            id_quat(),
        ));
    }

    // Blooms in varied colours.
    let blooms = [
        (-0.5_f32, 0.15_f32, [0.9_f32, 0.3_f32, 0.3_f32]),
        (0.45, -0.1, [0.95, 0.8, 0.3]),
        (0.1, 0.25, [0.85, 0.85, 0.9]),
        (-0.1, -0.3, [0.7, 0.4, 0.85]),
        (0.55, 0.3, [0.9, 0.5, 0.2]),
    ];
    for (dx, dz, color) in blooms {
        prims.push(prim(
            sphere(0.08, 3, foliage(color)),
            [dx, rim_h + 0.5, dz],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
