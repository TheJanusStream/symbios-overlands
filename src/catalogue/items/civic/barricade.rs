//! Barricade — crossed timber beams behind a lashed-on plank. An
//! escalation-Conflict scatter prop: a hasty road-block reads the same in a
//! medieval siege or a modern riot.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, quat_x, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{CANVAS_RED, TIN, WOOD, WOOD_GREY, cloth, corrugated, quat_z, wood};

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
    use std::f32::consts::FRAC_PI_2;
    let beam = || solid(cuboid_tapered([0.12, 1.6, 0.12], 0.0, wood(WOOD)));
    // Rope lashing wrapped round an X-crossing — a thin dark fibre ring in
    // the beams' (X-Y) plane.
    let lash = |x: f32| {
        prim(
            torus(0.035, 0.14, wood([0.12, 0.10, 0.07])),
            [x, 0.8, 0.0],
            quat_x(FRAC_PI_2),
        )
    };

    // The top rail is the flat root (id_quat); the leaning sawhorse beams,
    // braces and rag hang off it as children — so the assemble root is never
    // a tilted piece (which would skew the whole block).
    super::assemble(vec![
        // Top rail lashed across the trestles.
        prim(
            solid(cuboid_tapered([2.2, 0.18, 0.16], 0.0, wood(WOOD_GREY))),
            [0.0, 0.98, 0.0],
            id_quat(),
        ),
        // Two X-crossed sawhorse trestles, one at each end.
        prim(beam(), [-0.85, 0.8, 0.0], quat_z(0.55)),
        prim(beam(), [-0.85, 0.8, 0.0], quat_z(-0.55)),
        prim(beam(), [0.85, 0.8, 0.0], quat_z(0.55)),
        prim(beam(), [0.85, 0.8, 0.0], quat_z(-0.55)),
        lash(-0.85),
        lash(0.85),
        // Lower lashing plank.
        prim(
            solid(cuboid_tapered([2.0, 0.16, 0.14], 0.0, wood(WOOD))),
            [0.0, 0.46, 0.05],
            id_quat(),
        ),
        // A long plank nailed diagonally for rigidity.
        prim(
            solid(cuboid_tapered([0.14, 2.1, 0.1], 0.0, wood(WOOD_GREY))),
            [0.0, 0.72, -0.07],
            quat_z(0.62),
        ),
        // A salvaged corrugated sheet wired across the front (-Z) face.
        prim(
            cuboid_tapered([0.85, 0.7, 0.04], 0.0, corrugated(TIN)),
            [-0.55, 0.6, -0.12],
            quat_z(0.08),
        ),
        // A torn warning rag tied to the rail.
        prim(
            cuboid_tapered([0.32, 0.44, 0.03], 0.0, cloth(CANVAS_RED)),
            [0.55, 0.66, -0.12],
            quat_z(0.2),
        ),
    ])
}
