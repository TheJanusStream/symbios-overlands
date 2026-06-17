//! Stela — a Mesoamerican secondary. A tall carved limestone slab recording
//! a ruler's reign in bands of glyphs, set with a jade mask inlay and paired
//! with a round sacrificial altar stone at its foot.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{JADE_GREEN, LIMESTONE_PALE, STONE_GREY, cobble, jade, limestone};

pub struct Stela;

impl CatalogueEntry for Stela {
    fn slug(&self) -> &'static str {
        "stela"
    }
    fn name(&self) -> &'static str {
        "Stela"
    }
    fn description(&self) -> &'static str {
        "Carved limestone slab in glyph bands with a jade inlay and an altar stone."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Carved stela slab — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [1.4, 4.6, 0.55],
            0.08,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, 2.3, 0.0],
        id_quat(),
    )];

    // Recessed glyph bands down the front face.
    for k in 0..4 {
        prims.push(prim(
            cuboid_tapered([1.0, 0.5, 0.08], 0.0, cobble(STONE_GREY)),
            [0.0, 0.9 + k as f32 * 0.8, 0.3],
            id_quat(),
        ));
    }
    // Jade mask inlay near the top.
    prims.push(prim(
        cuboid_tapered([0.55, 0.7, 0.1], 0.1, jade(JADE_GREEN)),
        [0.0, 3.7, 0.32],
        id_quat(),
    ));

    // Round sacrificial altar stone at the foot.
    prims.push(prim(
        solid(cylinder_tapered(1.0, 0.6, 16, 0.05, limestone(STONE_GREY))),
        [0.0, 0.3, 1.8],
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
        assert_sanitize_stable(&Stela.build(""), "stela");
    }
}
