//! Veggie planter — a Solarpunk prop. A raised timber bed of crops with a
//! climbing-bean trellis. Scatter clutter greening the eco-quarter.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CROP_GREEN, LEAF_GREEN, SOIL_DARK, TIMBER_WARM, crop_tufts, foliage, timber};

pub struct VeggiePlanter;

impl CatalogueEntry for VeggiePlanter {
    fn slug(&self) -> &'static str {
        "veggie_planter"
    }
    fn name(&self) -> &'static str {
        "Veggie Planter"
    }
    fn description(&self) -> &'static str {
        "Raised timber bed of crops with a climbing-bean trellis."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Timber bed — the root.
        prim(
            solid(cuboid_tapered([1.8, 0.5, 0.9], 0.0, timber(TIMBER_WARM))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
    ];

    // Soil cap + rows of leafy crops mounded in the bed.
    prims.push(prim(
        solid(cuboid_tapered([1.66, 0.16, 0.76], 0.0, foliage(SOIL_DARK))),
        [0.0, 0.55, 0.0],
        id_quat(),
    ));
    prims.extend(crop_tufts(
        [0.0, 0.62, 0.0],
        [1.5, 0.6],
        5,
        2,
        0.5,
        foliage(CROP_GREEN),
    ));

    // Lattice trellis at the back: two posts, a top rail and cross slats.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.06, 1.4, 0.06], 0.0, timber(TIMBER_WARM))),
            [sx * 0.8, 1.0, -0.35],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([1.7, 0.06, 0.06], 0.0, timber(TIMBER_WARM))),
        [0.0, 1.62, -0.35],
        id_quat(),
    ));
    for y in [1.05_f32, 1.32] {
        prims.push(prim(
            solid(cuboid_tapered([1.6, 0.04, 0.04], 0.0, timber(TIMBER_WARM))),
            [0.0, y, -0.35],
            id_quat(),
        ));
    }
    // Climbing-bean foliage growing up the trellis.
    prims.extend(crop_tufts(
        [0.0, 1.35, -0.34],
        [1.5, 0.0],
        6,
        1,
        0.42,
        foliage(LEAF_GREEN),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&VeggiePlanter.build(""), "veggie_planter");
    }
}
