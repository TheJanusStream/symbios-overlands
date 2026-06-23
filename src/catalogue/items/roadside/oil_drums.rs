//! Oil drums — a Roadside *poor* prop. A clutch of rusted 55-gallon barrels,
//! two standing and one toppled on its side. The leaking clutter of the
//! busted shoulder.
//!
//! The toppled drum is a cylinder tipped on its side (a single
//! [`quat_x`] of π/2 lays the Y axis along Z).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{RUST_BROWN, STEEL_GREY, enamel, steel};

pub struct OilDrums;

impl CatalogueEntry for OilDrums {
    fn slug(&self) -> &'static str {
        "oil_drums"
    }
    fn name(&self) -> &'static str {
        "Oil Drums"
    }
    fn description(&self) -> &'static str {
        "A clutch of rusted barrels, two standing and one toppled on its side."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A standing rusted drum with two rolling-hoop ribs, a lid and two bung caps,
/// returned for the assemble list at `pos`.
fn standing_drum(pos: [f32; 3]) -> Generator {
    let mut drum = prim(
        solid(cylinder_tapered(0.32, 0.9, 12, 0.0, steel(RUST_BROWN))),
        pos,
        id_quat(),
    );
    for ring_y in [-0.22_f32, 0.22] {
        drum.children.push(prim(
            torus(0.04, 0.32, steel(STEEL_GREY)),
            [0.0, ring_y, 0.0],
            id_quat(),
        ));
    }
    // Galvanised lid + two bung caps on top.
    drum.children.push(prim(
        solid(cylinder_tapered(0.31, 0.05, 12, 0.0, steel(STEEL_GREY))),
        [0.0, 0.46, 0.0],
        id_quat(),
    ));
    for bx in [-0.15_f32, 0.15] {
        drum.children.push(prim(
            solid(cylinder_tapered(0.05, 0.06, 8, 0.0, steel(STEEL_GREY))),
            [bx, 0.51, 0.0],
            id_quat(),
        ));
    }
    drum
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // First standing drum — the root.
        standing_drum([0.0, 0.45, 0.0]),
        standing_drum([0.72, 0.45, 0.18]),
    ];

    // A third drum toppled on its side along Z, with rimmed end caps.
    let mut toppled = prim(
        solid(cylinder_tapered(0.32, 0.9, 12, 0.0, steel(RUST_BROWN))),
        [-0.6, 0.32, -0.35],
        quat_x(FRAC_PI_2),
    );
    for end_y in [-0.46_f32, 0.46] {
        toppled.children.push(prim(
            torus(0.04, 0.3, steel(STEEL_GREY)),
            [0.0, end_y, 0.0],
            id_quat(),
        ));
    }
    prims.push(toppled);

    // A dark oily spill puddle pooling under the toppled drum.
    prims.push(prim(
        solid(cylinder_tapered(
            0.55,
            0.02,
            16,
            0.0,
            enamel([0.05, 0.05, 0.06]),
        )),
        [-0.75, 0.012, -0.6],
        id_quat(),
    ));

    // A stray dented jerry can, with a cap.
    prims.push(prim(
        solid(cuboid_tapered([0.24, 0.34, 0.16], 0.0, steel(RUST_BROWN))),
        [0.55, 0.17, -0.55],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.05, 0.07, 8, 0.0, steel(STEEL_GREY))),
        [0.55, 0.37, -0.55],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&OilDrums.build(""), "oil_drums");
    }
}
