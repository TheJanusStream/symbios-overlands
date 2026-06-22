//! Compost heap — a Solarpunk *poor* prop. A timber pallet bin heaped with
//! rotting compost and green scraps. The humble nutrient cycle of the
//! grassroots commune.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CROP_GREEN, SOIL_DARK, TIMBER_WARM, crop_tufts, foliage, timber};

/// Dark rotting brown of the compost.
const COMPOST: [f32; 3] = [0.30, 0.24, 0.16];

pub struct CompostHeap;

impl CatalogueEntry for CompostHeap {
    fn slug(&self) -> &'static str {
        "compost_heap"
    }
    fn name(&self) -> &'static str {
        "Compost Heap"
    }
    fn description(&self) -> &'static str {
        "Timber pallet bin heaped with rotting compost and green scraps."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_POOR
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
    let step = 0.24_f32; // board pitch (board + gap)
    let mut prims = vec![
        // Earth floor pad — the flat root the bin sits on.
        prim(
            solid(cuboid_tapered([1.5, 0.06, 1.5], 0.0, foliage(SOIL_DARK))),
            [0.0, 0.03, 0.0],
            id_quat(),
        ),
    ];

    // Corner posts of the pallet bin.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.1, 1.0, 0.1], 0.0, timber(TIMBER_WARM))),
                [sx * 0.68, 0.5, sz * 0.68],
                id_quat(),
            ));
        }
    }
    // Slatted walls — horizontal boards with gaps between (a real pallet, not
    // a solid panel): a full back + two sides + a low open front.
    for k in 0..4 {
        let y = 0.16 + k as f32 * step;
        // Back (−Z) and (for the lower two courses) front (+Z).
        prims.push(prim(
            solid(cuboid_tapered([1.36, 0.14, 0.08], 0.0, timber(TIMBER_WARM))),
            [0.0, y, -0.65],
            id_quat(),
        ));
        if k < 2 {
            prims.push(prim(
                solid(cuboid_tapered([1.36, 0.14, 0.08], 0.0, timber(TIMBER_WARM))),
                [0.0, y, 0.65],
                id_quat(),
            ));
        }
        // Two side walls.
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.08, 0.14, 1.36], 0.0, timber(TIMBER_WARM))),
                [sx * 0.65, y, 0.0],
                id_quat(),
            ));
        }
    }

    // Lumpy heaped compost — overlapping brown clumps fill the bin.
    prims.extend(crop_tufts(
        [0.0, 0.18, 0.0],
        [1.05, 1.05],
        4,
        4,
        0.55,
        foliage(COMPOST),
    ));
    // Fresh green scraps tossed on top.
    prims.extend(crop_tufts(
        [0.05, 0.5, 0.0],
        [0.85, 0.85],
        3,
        3,
        0.4,
        foliage(CROP_GREEN),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CompostHeap.build(""), "compost_heap");
    }
}
