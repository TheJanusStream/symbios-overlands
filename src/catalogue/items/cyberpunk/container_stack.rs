//! Container stack — a Cyberpunk *poor* secondary. Two weathered shipping
//! containers stacked askew with a dim neon strip; makeshift undercity
//! housing/storage ringing the scrap shanty.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONTAINER_BLUE, CONTAINER_RUST, NEON_CYAN, NEON_LIME, corrugated};

pub struct ContainerStack;

impl CatalogueEntry for ContainerStack {
    fn slug(&self) -> &'static str {
        "container_stack"
    }
    fn name(&self) -> &'static str {
        "Container Stack"
    }
    fn description(&self) -> &'static str {
        "Two weathered shipping containers stacked askew with a dim neon strip."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ch = 2.5;
    assemble(vec![
        // Lower container (root).
        prim(
            solid(cuboid_tapered(
                [3.6, ch, 2.3],
                0.0,
                corrugated(CONTAINER_BLUE),
            )),
            [0.0, ch * 0.5, 0.0],
            id_quat(),
        ),
        // Upper container, shifted and tilted.
        prim(
            solid(cuboid_tapered(
                [3.3, ch, 2.2],
                0.0,
                corrugated(CONTAINER_RUST),
            )),
            [0.35, ch * 1.5, 0.15],
            quat_x(0.05),
        ),
        // Dim neon strip down the side.
        prim(
            cuboid_tapered([0.15, ch * 1.6, 0.15], 0.0, glow(NEON_LIME, 3.0)),
            [1.9, ch * 0.9, 0.0],
            id_quat(),
        ),
        // A small lit porthole on the lower container — a sign the undercity
        // housing is occupied. Proud of the face so it can't z-fight the
        // body plane.
        prim(
            cuboid_tapered([0.5, 0.5, 0.06], 0.0, glow(NEON_CYAN, 2.0)),
            [-0.6, ch * 0.55, 1.2],
            id_quat(),
        ),
    ])
}
