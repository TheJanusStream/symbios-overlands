//! Shanty — a makeshift lean-to of mismatched boards under a slanted tin
//! roof. A prosperity-Poor scatter prop: it reads as improvised housing in
//! any setting, from a medieval slum to a cyberpunk undercity.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{TIN, WOOD, WOOD_GREY, rust_metal, wood};

pub struct Shanty;

impl CatalogueEntry for Shanty {
    fn slug(&self) -> &'static str {
        "shanty"
    }
    fn name(&self) -> &'static str {
        "Shanty"
    }
    fn description(&self) -> &'static str {
        "Makeshift board lean-to under a slanted tin roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Poor)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.7,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 1.8;
    let d = 1.4;
    let h = 1.5;

    let post_h = h + 0.5;
    super::assemble(vec![
        // Board body — back and side walls as one solid block (the open
        // front faces away once placed).
        prim(
            solid(cuboid_tapered([w, h, d], 0.0, wood(WOOD))),
            [0.0, h * 0.5, 0.0],
            id_quat(),
        ),
        // Mismatched grey-board patch on one side.
        prim(
            cuboid_tapered([0.05, 0.8, 0.7], 0.0, wood(WOOD_GREY)),
            [w * 0.5 + 0.02, h * 0.5, 0.1],
            id_quat(),
        ),
        // Slanted, overhanging tin roof (higher at the front).
        prim(
            solid(cuboid_tapered(
                [w + 0.4, 0.08, d + 0.6],
                0.0,
                rust_metal(TIN),
            )),
            [0.0, h + 0.3, 0.05],
            quat_x(0.32),
        ),
        // A leaning support post propping the high front edge.
        prim(
            solid(cuboid_tapered([0.12, post_h, 0.12], 0.0, wood(WOOD_GREY))),
            [w * 0.5 - 0.1, post_h * 0.5, d * 0.5 + 0.2],
            quat_x(0.12),
        ),
    ])
}
