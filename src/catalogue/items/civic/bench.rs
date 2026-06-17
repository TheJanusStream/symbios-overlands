//! Bench — a slatted seat on iron end-frames. An escalation-Calm scatter
//! prop: public seating signals a settled, unthreatened place to linger in
//! any setting.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{WOOD, bronze, wood};

const IRON: [f32; 3] = [0.12, 0.12, 0.13];

pub struct Bench;

impl CatalogueEntry for Bench {
    fn slug(&self) -> &'static str {
        "bench"
    }
    fn name(&self) -> &'static str {
        "Bench"
    }
    fn description(&self) -> &'static str {
        "Slatted wooden seat on cast-iron end-frames."
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
            clearance: 1.1,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    super::assemble(vec![
        // Seat.
        prim(
            solid(cuboid_tapered([1.5, 0.1, 0.45], 0.0, wood(WOOD))),
            [0.0, 0.5, 0.0],
            id_quat(),
        ),
        // Backrest.
        prim(
            solid(cuboid_tapered([1.5, 0.45, 0.08], 0.0, wood(WOOD))),
            [0.0, 0.75, -0.2],
            id_quat(),
        ),
        // Cast-iron end frames.
        prim(
            solid(cuboid_tapered([0.08, 0.5, 0.45], 0.0, bronze(IRON))),
            [-0.65, 0.25, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.08, 0.5, 0.45], 0.0, bronze(IRON))),
            [0.65, 0.25, 0.0],
            id_quat(),
        ),
    ])
}
