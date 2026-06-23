//! Greenhouse — a Rural/Farmland secondary. A glazed timber-framed glasshouse
//! on a low stone knee-wall, with a glass gable roof and planting benches
//! inside, where seedlings are raised under glass.

use crate::catalogue::items::modern_city::curtain_wall;
use crate::catalogue::items::solarpunk::{CROP_GREEN, crop_tufts, foliage};
use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, STONE_GREY, TRIM_WHITE, WOOD_GREY, clapboard, glass, stone, weathered};

pub struct Greenhouse;

impl CatalogueEntry for Greenhouse {
    fn slug(&self) -> &'static str {
        "greenhouse"
    }
    fn name(&self) -> &'static str {
        "Greenhouse"
    }
    fn description(&self) -> &'static str {
        "Glazed timber-framed glasshouse on a stone knee-wall with planting benches."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 8.0_f32;
    let w = 5.0_f32;
    let base_h = 0.3;
    let knee_h = 0.6;
    let glass_h = 2.0;
    let eave = base_h + knee_h + glass_h;
    let glass_cy = base_h + knee_h + glass_h * 0.5;
    let wall_cy = base_h + (glass_h + knee_h) * 0.5;
    // Faint warm interior, lit from within at golden hour.
    let dusk_glass = || glass(GLASS_TINT, 0.3);

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.4, base_h, w + 0.4],
                0.0,
                stone([0.55, 0.55, 0.56]),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Stone knee-wall.
        prim(
            solid(cuboid_tapered([l, knee_h, w], 0.0, stone(STONE_GREY))),
            [0.0, base_h + knee_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Back (+Z) and side glazing — plain lit panes.
    prims.push(prim(
        cuboid_tapered([l, glass_h, 0.1], 0.0, dusk_glass()),
        [0.0, glass_cy, w * 0.5 - 0.05],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.1, glass_h, w], 0.0, dusk_glass()),
            [sx * (l * 0.5 - 0.05), glass_cy, 0.0],
            id_quat(),
        ));
    }
    // Timber corner posts.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.14, glass_h + knee_h, 0.14],
                0.0,
                clapboard(TRIM_WHITE),
            )),
            [sx * l * 0.5, wall_cy, sz * w * 0.5],
            id_quat(),
        ));
    }

    // Gridded glazed front on the −Z face (the camera face): two mullioned bays
    // flanking a glazed timber door.
    let front_z = -w * 0.5;
    for bx in [-l * 0.28, l * 0.28] {
        prims.extend(curtain_wall(
            [bx, glass_cy, front_z],
            [l * 0.42, glass_h],
            (2, 2),
            -0.08,
            dusk_glass(),
            clapboard(TRIM_WHITE),
        ));
    }
    // Glazed door, set proud toward −Z, with white jambs.
    prims.push(prim(
        cuboid_tapered(
            [1.1, glass_h + knee_h - 0.1, 0.12],
            0.0,
            glass(GLASS_TINT, 0.35),
        ),
        [0.0, wall_cy, front_z - 0.06],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.12, glass_h + knee_h, 0.1], 0.0, clapboard(TRIM_WHITE)),
            [sx * 0.62, wall_cy, front_z - 0.1],
            id_quat(),
        ));
    }

    // Glass gable roof and a ridge beam.
    prims.push(prim(
        solid(cuboid_tapered([l + 0.6, 1.4, w + 0.6], 0.4, dusk_glass())),
        [0.0, eave + 0.7, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.2, 0.15, 0.15],
            0.0,
            clapboard(TRIM_WHITE),
        )),
        [0.0, eave + 1.35, 0.0],
        id_quat(),
    ));

    // Two planting benches with rows of green seedlings raised under the glass.
    for sz in [-1.0_f32, 1.0] {
        let bench_z = sz * 1.2;
        prims.push(prim(
            solid(cuboid_tapered(
                [l - 1.5, 0.12, 1.0],
                0.0,
                weathered(WOOD_GREY),
            )),
            [0.0, base_h + knee_h + 0.5, bench_z],
            id_quat(),
        ));
        prims.extend(crop_tufts(
            [0.0, base_h + knee_h + 0.56, bench_z],
            [l - 2.2, 0.6],
            6,
            2,
            0.32,
            foliage(CROP_GREEN),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Greenhouse.build(""), "greenhouse");
    }
}
