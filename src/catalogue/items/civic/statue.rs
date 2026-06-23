//! Statue — a bronze figure on a marble plinth. A prosperity-Rich scatter
//! prop: commemorative statuary signals an established, well-off settlement
//! in any setting.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{BRONZE, MARBLE, bronze, figure_parts, marble};

pub struct Statue;

impl CatalogueEntry for Statue {
    fn slug(&self) -> &'static str {
        "statue"
    }
    fn name(&self) -> &'static str {
        "Statue"
    }
    fn description(&self) -> &'static str {
        "Bronze figure raised on a stepped marble plinth."
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
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Stepped plinth — base, die and cornice cap, each course overlapping
    // the one below so no two horizontal faces sit flush (coplanar z-fight).
    let die_top = 0.94;
    let cap_top = die_top + 0.10;

    let mut prims = vec![
        // Base step.
        prim(
            solid(cuboid_tapered(
                [1.0, 0.25, 1.0],
                0.0,
                marble([0.82, 0.81, 0.78]),
            )),
            [0.0, 0.125, 0.0],
            id_quat(),
        ),
        // Main die, very slightly battered.
        prim(
            solid(cuboid_tapered([0.78, 0.72, 0.78], 0.04, marble(MARBLE))),
            [0.0, 0.59, 0.0],
            id_quat(),
        ),
        // Cornice cap, oversailing the die.
        prim(
            solid(cuboid_tapered(
                [0.96, 0.14, 0.96],
                0.0,
                marble([0.82, 0.81, 0.78]),
            )),
            [0.0, die_top + 0.03, 0.0],
            id_quat(),
        ),
        // Bronze dedication plate, proud of the die's front (-Z) face.
        prim(
            cuboid_tapered([0.52, 0.3, 0.04], 0.0, bronze([0.40, 0.30, 0.16])),
            [0.0, 0.6, -0.43],
            id_quat(),
        ),
    ];

    // The commemorative figure, gaze and raised arm toward the front.
    prims.extend(figure_parts(cap_top - 0.02, -1.0, BRONZE));

    super::assemble(prims)
}
