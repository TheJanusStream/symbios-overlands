//! Scrap pile — a heap of corroded sheet metal, a bald tyre and a dented
//! drum. A prosperity-Poor scatter prop reading as accumulated junk in any
//! setting.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, helix, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{RUST, SCRAP, TIN, WOOD, corrugated, quat_z, rust_metal, wood};

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
        // Base mound of crushed metal (the flat root).
        prim(
            solid(cuboid_tapered([1.3, 0.5, 1.0], 0.25, rust_metal(SCRAP))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
        // Leaning sheets of salvage.
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
        // A buckled corrugated panel.
        prim(
            cuboid_tapered([0.7, 0.05, 0.5], 0.0, corrugated(TIN)),
            [0.38, 0.46, 0.36],
            quat_z(-0.3),
        ),
        // A bald tyre lying flat (torus turned into the ground plane).
        prim(
            torus(0.12, 0.34, rust_metal([0.07, 0.07, 0.08])),
            [0.42, 0.18, -0.32],
            quat_x(FRAC_PI_2),
        ),
        // A wheel rim propped on edge.
        prim(
            torus(0.05, 0.22, rust_metal([0.45, 0.45, 0.47])),
            [-0.5, 0.55, 0.3],
            quat_x(0.35),
        ),
        // A dented oil drum on its side.
        prim(
            solid(cylinder_tapered(0.28, 0.62, 12, 0.0, rust_metal(RUST))),
            [-0.4, 0.32, -0.25],
            quat_z(FRAC_PI_2),
        ),
        // A coiled spring resting on the heap.
        prim(
            helix(0.11, 0.03, 0.07, 3.0, 8, rust_metal([0.32, 0.32, 0.34])),
            [0.12, 0.72, 0.12],
            quat_x(FRAC_PI_2),
        ),
        // A bent length of pipe (two segments meeting at an elbow).
        prim(
            solid(cylinder_tapered(
                0.05,
                0.5,
                8,
                0.0,
                rust_metal([0.5, 0.28, 0.16]),
            )),
            [0.3, 0.68, 0.0],
            quat_z(0.7),
        ),
        prim(
            solid(cylinder_tapered(
                0.05,
                0.4,
                8,
                0.0,
                rust_metal([0.5, 0.28, 0.16]),
            )),
            [0.52, 0.52, 0.0],
            quat_z(-0.25),
        ),
        // A broken wooden crate.
        prim(
            solid(cuboid_tapered([0.4, 0.34, 0.4], 0.0, wood(WOOD))),
            [-0.55, 0.45, -0.12],
            quat_z(0.16),
        ),
        // Rebar rods jutting from the heap.
        prim(
            solid(cylinder_tapered(
                0.02,
                0.72,
                6,
                0.0,
                rust_metal([0.42, 0.24, 0.14]),
            )),
            [0.05, 0.7, -0.22],
            quat_z(0.4),
        ),
        prim(
            solid(cylinder_tapered(
                0.02,
                0.6,
                6,
                0.0,
                rust_metal([0.42, 0.24, 0.14]),
            )),
            [-0.1, 0.7, -0.3],
            quat_x(0.35),
        ),
    ])
}
