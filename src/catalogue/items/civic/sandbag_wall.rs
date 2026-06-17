//! Sandbag wall — a staggered, stacked-bag emplacement. An
//! escalation-Conflict scatter prop: improvised fortification reads the same
//! across every setting.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{SANDBAG, cloth};

pub struct SandbagWall;

impl CatalogueEntry for SandbagWall {
    fn slug(&self) -> &'static str {
        "sandbag_wall"
    }
    fn name(&self) -> &'static str {
        "Sandbag Wall"
    }
    fn description(&self) -> &'static str {
        "Staggered courses of stacked sandbags."
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
    let bag_w = 0.5;
    let bag_h = 0.24;
    let bag_d = 0.32;
    // Two close sandbag tones so the stack reads as individual bags.
    let tone = |i: usize| {
        if i.is_multiple_of(2) {
            cloth(SANDBAG)
        } else {
            cloth([0.56, 0.49, 0.33])
        }
    };
    let bag = |i: usize| solid(cuboid_tapered([bag_w, bag_h, bag_d], 0.18, tone(i)));

    // Three staggered courses, narrowing toward the top.
    let courses: [&[f32]; 3] = [&[-0.5, 0.0, 0.5], &[-0.25, 0.25], &[0.0]];

    let mut bags = Vec::new();
    let mut i = 0usize;
    for (row, xs) in courses.iter().enumerate() {
        let y = bag_h * (row as f32 + 0.5);
        for &x in *xs {
            bags.push(prim(bag(i), [x, y, 0.0], id_quat()));
            i += 1;
        }
    }

    super::assemble(bags)
}
