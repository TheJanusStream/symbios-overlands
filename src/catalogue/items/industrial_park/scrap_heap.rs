//! Scrap heap — an Industrial-Park *poor* prop. A tangle of cast-off steel:
//! rusted I-beams, a bundle of rebar, a crushed drum, a coil of cable and an
//! old tyre, torn sheet metal, piled at the yard's edge.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, helix, id_quat, prim, quat_x, quat_y, quat_z,
    solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp4, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{RUST_BROWN, concrete, rust, tank_steel};

/// Rusted iron.
const IRON_RUST: [f32; 3] = [0.40, 0.26, 0.15];
/// Faded drum.
const DRUM: [f32; 3] = [0.30, 0.36, 0.30];

pub struct ScrapHeap;

impl CatalogueEntry for ScrapHeap {
    fn slug(&self) -> &'static str {
        "scrap_heap"
    }
    fn name(&self) -> &'static str {
        "Scrap Heap"
    }
    fn description(&self) -> &'static str {
        "Tangle of rusted I-beams, rebar, a drum, a cable coil, and torn metal."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_POOR
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

/// A rusted I-beam — bottom flange, web, top flange — as a rigid subtree so it
/// tumbles as one piece at `rot`.
fn i_beam(pos: [f32; 3], rot: Fp4, len: f32, color: [f32; 3]) -> Generator {
    let mut b = prim(
        solid(cuboid_tapered([len, 0.34, 0.08], 0.0, rust(color))),
        pos,
        rot,
    );
    for sy in [-1.0_f32, 1.0] {
        b.children.push(prim(
            solid(cuboid_tapered([len, 0.07, 0.34], 0.0, rust(color))),
            [0.0, sy * 0.17, 0.0],
            id_quat(),
        ));
    }
    b
}

fn build_tree() -> Generator {
    // Scuffed, oil-stained dirt patch under the pile — the flat id_quat root
    // (the heap used to root on a yawed I-beam, which spun the whole tangle
    // into its frame).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [3.4, 0.12, 2.8],
            0.0,
            concrete([0.2, 0.19, 0.17]),
        )),
        [0.0, 0.06, 0.0],
        id_quat(),
    )];

    // Two I-beams crossing the pile and a third propped up on them.
    prims.push(i_beam([0.0, 0.3, 0.0], quat_y(0.2), 2.6, IRON_RUST));
    prims.push(i_beam([0.2, 0.55, 0.1], quat_y(-0.7), 2.2, IRON_RUST));
    prims.push(i_beam(
        [-0.4, 0.7, -0.5],
        quat_z(0.35),
        1.8,
        [0.44, 0.3, 0.16],
    ));

    // A bundle of rebar.
    for (i, dz) in [-0.06_f32, 0.0, 0.06].iter().enumerate() {
        prims.push(prim(
            solid(cylinder_tapered(0.04, 2.4, 6, 0.0, rust([0.44, 0.3, 0.16]))),
            [-0.9, 0.25 + i as f32 * 0.07, 0.8 + dz],
            quat_x(FRAC_PI_2),
        ));
    }

    // A crushed drum on its side.
    prims.push(prim(
        solid(cylinder_tapered(0.34, 1.0, 12, 0.12, tank_steel(DRUM))),
        [1.2, 0.4, -0.6],
        quat_x(FRAC_PI_2),
    ));

    // A loose coil of cable and an old tyre.
    prims.push(prim(
        helix(0.32, 0.05, 0.12, 3.0, 12, rust([0.22, 0.2, 0.18])),
        [-1.3, 0.3, -0.7],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.16, 0.42, tank_steel([0.12, 0.12, 0.13])),
        [1.4, 0.32, 0.9],
        quat_x(1.3),
    ));

    // Torn sheet metal leaning on the pile.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 0.05, 1.0], 0.0, rust(RUST_BROWN))),
        [-0.3, 0.6, -0.5],
        quat_x(0.6),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ScrapHeap.build(""), "scrap_heap");
    }
}
