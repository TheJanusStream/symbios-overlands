//! Scrap shanty — the Cyberpunk *poor* landmark. A precarious tower of
//! mismatched shipping containers stacked askew, patched with tin lean-tos
//! and lit by a single failing neon sign. The undercity counterpart to the
//! glossy [`neon_megatower`](super::neon_megatower): same theme, opposite
//! end of the prosperity axis (`Poor`), so a destitute cyberpunk room grows
//! this instead of the megastructure.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONTAINER_BLUE, CONTAINER_RUST, DARK_METAL, NEON_MAGENTA, RUST_BROWN, metal, rust};

pub struct ScrapShanty;

impl CatalogueEntry for ScrapShanty {
    fn slug(&self) -> &'static str {
        "scrap_shanty"
    }
    fn name(&self) -> &'static str {
        "Scrap Shanty"
    }
    fn description(&self) -> &'static str {
        "Tower of mismatched shipping containers stacked askew under a failing neon sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // A leaning stack of containers, each offset and tilted off the last.
    let container = |w: f32, h: f32, d: f32, color: [f32; 3]| {
        solid(cuboid_tapered([w, h, d], 0.0, rust(color)))
    };
    let ch = 2.5; // container height

    assemble(vec![
        // Ground container (root).
        prim(
            container(4.2, ch, 2.5, CONTAINER_RUST),
            [0.0, ch * 0.5, 0.0],
            id_quat(),
        ),
        // Second tier, shoved back and tilted.
        prim(
            container(3.8, ch, 2.4, CONTAINER_BLUE),
            [0.4, ch * 1.5, -0.25],
            quat_x(0.06),
        ),
        // Third tier, leaning the other way.
        prim(
            container(3.4, ch, 2.2, RUST_BROWN),
            [-0.35, ch * 2.5, 0.2],
            quat_x(-0.07),
        ),
        // A patched shack cab on top.
        prim(
            solid(cuboid_tapered([2.6, 2.0, 2.0], 0.08, metal(DARK_METAL))),
            [0.2, ch * 3.0 + 1.0, 0.1],
            quat_x(0.05),
        ),
        // Slanted tin lean-to off the second tier.
        prim(
            solid(cuboid_tapered([4.6, 0.1, 3.2], 0.0, rust([0.5, 0.5, 0.52]))),
            [0.4, ch * 2.0 + 0.1, 1.4],
            quat_x(0.4),
        ),
        // Failing vertical neon sign down the front (dim, flickery feel).
        prim(
            cuboid_tapered([0.18, ch * 2.2, 0.18], 0.0, glow(NEON_MAGENTA, 3.0)),
            [2.2, ch * 1.6, 0.0],
            id_quat(),
        ),
        // Antenna mast + dim beacon.
        prim(
            solid(cylinder_tapered(0.06, 2.4, 6, 0.0, metal(DARK_METAL))),
            [0.2, ch * 3.0 + 3.2, 0.1],
            id_quat(),
        ),
        prim(
            sphere(0.18, 3, glow(NEON_MAGENTA, 4.0)),
            [0.2, ch * 3.0 + 4.4, 0.1],
            id_quat(),
        ),
    ])
}
