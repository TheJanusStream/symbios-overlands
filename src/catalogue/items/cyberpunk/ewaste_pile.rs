//! E-waste pile — a Cyberpunk *poor* prop. A heap of dead screens, cabling
//! and busted circuitry with one cracked panel still faintly glowing;
//! undercity street clutter.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, RUST_BROWN, metal, rust};

pub struct EwastePile;

impl CatalogueEntry for EwastePile {
    fn slug(&self) -> &'static str {
        "ewaste_pile"
    }
    fn name(&self) -> &'static str {
        "E-Waste Pile"
    }
    fn description(&self) -> &'static str {
        "Heap of dead screens and cabling with one cracked panel still glowing."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_POOR
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
    assemble(vec![
        // Base mound of crushed e-waste (root).
        prim(
            solid(cuboid_tapered([1.2, 0.5, 1.0], 0.25, rust(RUST_BROWN))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
        // Dead screens / chassis tossed on at angles.
        prim(
            solid(cuboid_tapered([0.7, 0.06, 0.5], 0.0, metal(DARK_METAL))),
            [0.1, 0.6, -0.05],
            quat_x(0.5),
        ),
        prim(
            solid(cuboid_tapered([0.6, 0.06, 0.45], 0.0, metal(DARK_METAL))),
            [-0.2, 0.5, 0.15],
            quat_y(0.6),
        ),
        // A cracked panel still faintly lit.
        prim(
            cuboid_tapered([0.5, 0.04, 0.34], 0.0, glow(NEON_CYAN, 2.5)),
            [0.18, 0.66, 0.2],
            quat_x(-0.35),
        ),
    ])
}
