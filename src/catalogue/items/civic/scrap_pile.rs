//! Scrap pile — a heap of corroded sheet metal, a bald tyre and a dented
//! drum. A prosperity-Poor scatter prop reading as accumulated junk in any
//! setting.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{RUST, SCRAP, TIN, quat_z, rust_metal};

pub struct ScrapPile;

impl CatalogueEntry for ScrapPile {
    fn slug(&self) -> &'static str {
        "scrap_pile"
    }
    fn name(&self) -> &'static str {
        "Scrap Pile"
    }
    fn description(&self) -> &'static str {
        "Heap of rusted sheet metal, a tyre and a dented drum."
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
            clearance: 1.3,
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
        // Base mound of crushed metal.
        prim(
            solid(cuboid_tapered([1.3, 0.5, 1.0], 0.25, rust_metal(SCRAP))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
        // Two leaning sheets.
        prim(
            cuboid_tapered([1.0, 0.06, 0.7], 0.0, rust_metal(TIN)),
            [0.15, 0.6, -0.05],
            quat_z(0.45),
        ),
        prim(
            cuboid_tapered([0.8, 0.06, 0.6], 0.0, rust_metal(RUST)),
            [-0.25, 0.5, 0.2],
            quat_x(0.4),
        ),
        // A bald tyre lying flat (torus turned into the ground plane).
        prim(
            torus(0.12, 0.34, rust_metal([0.07, 0.07, 0.08])),
            [0.42, 0.18, -0.32],
            quat_x(FRAC_PI_2),
        ),
        // A dented oil drum on its side.
        prim(
            solid(cylinder_tapered(0.28, 0.62, 12, 0.0, rust_metal(RUST))),
            [-0.4, 0.32, -0.25],
            quat_z(FRAC_PI_2),
        ),
    ])
}
