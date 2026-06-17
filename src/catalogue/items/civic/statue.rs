//! Statue — a bronze figure on a marble plinth. A prosperity-Rich scatter
//! prop: commemorative statuary signals an established, well-off settlement
//! in any setting.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{BRONZE, MARBLE, bronze, marble, quat_z};

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
    let cap_y = 0.95;
    let body_h = 1.2;
    let body_y = cap_y + body_h * 0.5 + 0.06;
    let head_y = cap_y + body_h + 0.2;
    super::assemble(vec![
        // Stepped plinth.
        prim(
            solid(cuboid_tapered(
                [1.0, 0.25, 1.0],
                0.0,
                marble([0.82, 0.81, 0.78]),
            )),
            [0.0, 0.125, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.8, 0.7, 0.8], 0.05, marble(MARBLE))),
            [0.0, 0.6, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [0.95, 0.12, 0.95],
                0.0,
                marble([0.82, 0.81, 0.78]),
            )),
            [0.0, cap_y, 0.0],
            id_quat(),
        ),
        // Bronze figure: tapered body, a head, and one raised arm.
        prim(
            solid(cylinder_tapered(0.26, body_h, 12, 0.35, bronze(BRONZE))),
            [0.0, body_y, 0.0],
            id_quat(),
        ),
        prim(
            sphere(0.17, 3, bronze(BRONZE)),
            [0.0, head_y, 0.0],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(0.07, 0.7, 8, 0.0, bronze(BRONZE))),
            [0.28, body_y + 0.45, 0.0],
            quat_z(-0.9),
        ),
    ])
}
