//! Barricade — crossed timber beams behind a lashed-on plank. An
//! escalation-Conflict scatter prop: a hasty road-block reads the same in a
//! medieval siege or a modern riot.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{CANVAS_RED, WOOD, WOOD_GREY, cloth, quat_z, wood};

pub struct Barricade;

impl CatalogueEntry for Barricade {
    fn slug(&self) -> &'static str {
        "barricade"
    }
    fn name(&self) -> &'static str {
        "Barricade"
    }
    fn description(&self) -> &'static str {
        "Crossed timber beams behind a lashed-on plank."
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
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let beam = || solid(cuboid_tapered([0.12, 1.5, 0.12], 0.0, wood(WOOD)));

    super::assemble(vec![
        // Two X-crossed sawhorse pairs, one at each end.
        prim(beam(), [-0.8, 0.75, 0.0], quat_z(0.6)),
        prim(beam(), [-0.8, 0.75, 0.0], quat_z(-0.6)),
        prim(beam(), [0.8, 0.75, 0.0], quat_z(0.6)),
        prim(beam(), [0.8, 0.75, 0.0], quat_z(-0.6)),
        // Horizontal plank lashed across the crosses.
        prim(
            solid(cuboid_tapered([2.1, 0.2, 0.14], 0.0, wood(WOOD_GREY))),
            [0.0, 0.85, 0.0],
            id_quat(),
        ),
        // A torn warning rag tied to the plank.
        prim(
            cuboid_tapered([0.3, 0.4, 0.03], 0.0, cloth(CANVAS_RED)),
            [0.5, 0.62, 0.1],
            quat_z(0.2),
        ),
    ])
}
