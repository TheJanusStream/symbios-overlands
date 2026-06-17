//! Lantern — a standing lamp post with a warm glowing head. An
//! escalation-Calm scatter prop: maintained street lighting signals a safe,
//! orderly settlement in any setting.

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{BRONZE, LANTERN_WARM, bronze};

pub struct Lantern;

impl CatalogueEntry for Lantern {
    fn slug(&self) -> &'static str {
        "lantern"
    }
    fn name(&self) -> &'static str {
        "Lantern"
    }
    fn description(&self) -> &'static str {
        "Standing lamp post with a warm glowing head."
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
            clearance: 0.9,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 2.6;
    super::assemble(vec![
        // Weighted base.
        prim(
            solid(cylinder_tapered(0.18, 0.2, 12, 0.0, bronze(BRONZE))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
        // Pole.
        prim(
            solid(cylinder_tapered(0.07, pole_h, 10, 0.0, bronze(BRONZE))),
            [0.0, pole_h * 0.5, 0.0],
            id_quat(),
        ),
        // Glowing lantern housing.
        prim(
            cuboid_tapered([0.3, 0.42, 0.3], 0.0, glow(LANTERN_WARM, 5.0)),
            [0.0, pole_h + 0.05, 0.0],
            id_quat(),
        ),
        // Bronze cap.
        prim(
            cone(0.24, 0.22, 4, bronze(BRONZE)),
            [0.0, pole_h + 0.37, 0.0],
            id_quat(),
        ),
    ])
}
