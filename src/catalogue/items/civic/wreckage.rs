//! Wreckage — a collapsed wall, a fallen beam and scattered rubble. An
//! escalation-Conflict scatter prop: the aftermath of fighting reads the
//! same in any setting (and the escalation finish scorches it further).

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{WOOD_GREY, quat_z, stone, wood};

const RUBBLE: [f32; 3] = [0.40, 0.38, 0.35];

pub struct Wreckage;

impl CatalogueEntry for Wreckage {
    fn slug(&self) -> &'static str {
        "wreckage"
    }
    fn name(&self) -> &'static str {
        "Wreckage"
    }
    fn description(&self) -> &'static str {
        "Collapsed wall, a fallen beam and scattered rubble."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Conflict)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    use std::f32::consts::FRAC_PI_2;
    super::assemble(vec![
        // Toppling broken wall section.
        prim(
            solid(cuboid_tapered([1.4, 1.3, 0.25], 0.0, stone(RUBBLE))),
            [0.0, 0.65, 0.0],
            quat_z(0.22),
        ),
        // Jagged remnant of a second wall.
        prim(
            solid(cuboid_tapered(
                [0.6, 0.8, 0.22],
                0.3,
                stone([0.36, 0.34, 0.31]),
            )),
            [0.95, 0.4, -0.4],
            quat_z(-0.15),
        ),
        // Fallen roof beam lying across the rubble.
        prim(
            solid(cuboid_tapered([0.16, 1.8, 0.16], 0.0, wood(WOOD_GREY))),
            [-0.2, 0.2, 0.5],
            quat_z(FRAC_PI_2 - 0.2),
        ),
        // Scattered rubble blocks.
        prim(
            solid(cuboid_tapered([0.35, 0.3, 0.3], 0.2, stone(RUBBLE))),
            [-0.7, 0.15, -0.2],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [0.28, 0.24, 0.32],
                0.2,
                stone([0.42, 0.4, 0.36]),
            )),
            [0.5, 0.12, 0.55],
            id_quat(),
        ),
        prim(
            sphere(0.18, 3, stone([0.3, 0.28, 0.26])),
            [-0.3, 0.1, -0.55],
            id_quat(),
        ),
    ])
}
