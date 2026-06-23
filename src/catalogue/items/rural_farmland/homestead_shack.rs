//! Homestead shack — the Rural/Farmland *poor* landmark. A weathered board
//! shack with a rusting metal roof, boarded-up window, and a stovepipe
//! trailing thin smoke. The hardscrabble counterpart to the
//! [`barn`](super::barn): same theme, opposite end of the prosperity axis
//! (`Poor`), so a destitute room grows this instead of the red barn.

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_GREY, WOOD_GREY, enamel, fx, metal_roof, stone, weathered};

pub struct HomesteadShack;

impl CatalogueEntry for HomesteadShack {
    fn slug(&self) -> &'static str {
        "homestead_shack"
    }
    fn name(&self) -> &'static str {
        "Homestead Shack"
    }
    fn description(&self) -> &'static str {
        "Weathered board shack with a rusting roof and a smoking stovepipe."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 7.0_f32;
    let w = 6.0_f32;
    let foot_h = 0.4;
    let wall_h = 3.2;
    let wall_top = foot_h + wall_h;
    let front = w * 0.5;

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.5, foot_h, w + 0.5],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Weathered board walls.
        prim(
            solid(cuboid_tapered([l, wall_h, w], 0.0, weathered(WOOD_GREY))),
            [0.0, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Hero face toward the render front (−Z); boards stand proud toward −Z.
    let f = -front;

    // Crooked door.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.85, 1.9, 0.15],
            0.0,
            weathered([0.4, 0.38, 0.34]),
        )),
        [1.4, foot_h + 0.95, f],
        id_quat(),
    ));
    // Boarded-up window.
    prims.push(prim(
        cuboid_tapered([1.4, 1.0, 0.12], 0.0, weathered([0.34, 0.33, 0.3])),
        [-1.6, foot_h + 1.8, f],
        id_quat(),
    ));
    for ty in [-0.3_f32, 0.0, 0.3] {
        prims.push(prim(
            cuboid_tapered([1.5, 0.16, 0.06], 0.0, weathered(WOOD_GREY)),
            [-1.6, foot_h + 1.8 + ty, f - 0.1],
            id_quat(),
        ));
    }

    // Rusting corrugated gable roof (nordic A-frame helper) — sound but
    // weathered, not collapsed.
    let roof_h = 1.9_f32;
    prims.push(gable_roof(
        [l + 1.0, roof_h, w + 1.0],
        [0.0, wall_top + roof_h * 0.5, 0.0],
        metal_roof([0.45, 0.37, 0.32]),
    ));

    // Stovepipe chimney with a rain cap.
    let pipe_x = -1.8;
    prims.push(prim(
        solid(cylinder_tapered(
            0.12,
            1.6,
            8,
            0.0,
            enamel([0.4, 0.26, 0.16]),
        )),
        [pipe_x, wall_top + 0.9, -1.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.32, 0.08, 0.32],
            0.0,
            enamel([0.35, 0.22, 0.14]),
        )),
        [pipe_x, wall_top + 1.72, -1.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: thin stovepipe smoke.
    root.children.push(fx::chimney_smoke(
        [pipe_x, wall_top + 1.8, -1.0],
        0x70F0_5E22,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HomesteadShack.build(""), "homestead_shack");
    }
}
