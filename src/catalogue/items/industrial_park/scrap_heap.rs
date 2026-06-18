//! Scrap heap — an Industrial-Park *poor* prop. A tangle of cast-off steel:
//! rusted I-beams, a bundle of rebar, a leaking drum, and torn sheet metal,
//! piled at the yard's edge.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, prim, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{RUST_BROWN, rust, tank_steel};

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
        "Tangle of rusted I-beams, rebar, a drum, and torn sheet metal."
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

fn build_tree() -> Generator {
    // A long rusted I-beam — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([2.6, 0.3, 0.3], 0.0, rust(IRON_RUST))),
        [0.0, 0.18, 0.0],
        quat_y(0.2),
    )];
    // A second beam crossing it.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 0.28, 0.28], 0.0, rust(IRON_RUST))),
        [0.2, 0.42, 0.1],
        quat_y(-0.7),
    ));

    // A bundle of rebar.
    for (i, dz) in [-0.06_f32, 0.0, 0.06].iter().enumerate() {
        prims.push(prim(
            solid(cylinder_tapered(0.04, 2.4, 6, 0.0, rust([0.44, 0.3, 0.16]))),
            [-0.8, 0.2 + i as f32 * 0.07, 0.7 + dz],
            quat_x(FRAC_PI_2),
        ));
    }

    // A leaking drum on its side.
    prims.push(prim(
        solid(cylinder_tapered(0.34, 1.0, 12, 0.0, tank_steel(DRUM))),
        [1.1, 0.34, -0.6],
        quat_x(FRAC_PI_2),
    ));

    // Torn sheet metal leaning on the pile.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 0.05, 1.0], 0.0, rust(RUST_BROWN))),
        [-0.3, 0.5, -0.5],
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
