//! Chapel — a Medieval secondary. A small parish chapel of dressed stone
//! on a fieldstone footing: a steep slate roof, lancet windows of stained
//! glass down each flank, an iron-banded oak door under the gable, corner
//! buttresses, and a stone cross at the ridge. The quiet civic heart of
//! the burgh.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignStainedGlassConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    IRON_DARK, SLATE_GREY, STONE_GREY, STONE_PALE, WOOD_DARK, iron, rough_stone, shingle, stone,
    timber,
};

pub struct Chapel;

impl CatalogueEntry for Chapel {
    fn slug(&self) -> &'static str {
        "chapel"
    }
    fn name(&self) -> &'static str {
        "Chapel"
    }
    fn description(&self) -> &'static str {
        "Small stone parish chapel with a slate roof, stained-glass lancets, and a cross."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.5,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Coloured leaded glass for the lancets — a non-emissive jewel surface so
/// the daylit chapel glints without the forge's glow.
fn stained() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.30, 0.34, 0.46]),
        roughness: Fp(0.1),
        metallic: Fp(0.2),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::StainedGlass(SovereignStainedGlassConfig {
            cell_count: 8,
            saturation: Fp(0.85),
            grime_level: Fp64(0.15),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let l = 8.0_f32; // nave length (X)
    let w = 4.6_f32; // nave width (Z), door faces +X
    let foot_h = 0.4;
    let wall_h = 4.0;
    let wall_top = foot_h + wall_h;
    let front = l * 0.5;

    let mut prims = vec![
        // Fieldstone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.8, foot_h, w + 0.8],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Dressed-stone nave body.
        prim(
            solid(cuboid_tapered([l, wall_h, w], 0.0, stone(STONE_PALE))),
            [0.0, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
        // Steep slate hip roof.
        prim(
            solid(cuboid_tapered(
                [l + 0.8, 2.8, w + 0.8],
                0.55,
                shingle(SLATE_GREY),
            )),
            [0.0, wall_top + 1.4, 0.0],
            id_quat(),
        ),
    ];

    // Corner buttresses.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.6, wall_h * 0.75, 0.6],
                0.3,
                stone(STONE_GREY),
            )),
            [
                sx * (l * 0.5 - 0.1),
                foot_h + wall_h * 0.375,
                sz * (w * 0.5 + 0.15),
            ],
            id_quat(),
        ));
    }

    // Stained-glass lancets down each flank, two per side.
    for sz in [-1.0_f32, 1.0] {
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([1.0, 2.2, 0.15], 0.0, stained()),
                [sx * 2.0, foot_h + 2.0, sz * (w * 0.5 + 0.04)],
                id_quat(),
            ));
        }
    }
    // Gable window above the door.
    prims.push(prim(
        cuboid_tapered([0.15, 1.0, 1.0], 0.0, stained()),
        [front + 0.04, foot_h + 3.1, 0.0],
        id_quat(),
    ));

    // Iron-banded oak door under the front gable.
    prims.push(prim(
        solid(cuboid_tapered([0.18, 2.4, 1.5], 0.0, timber(WOOD_DARK))),
        [front + 0.05, foot_h + 1.2, 0.0],
        id_quat(),
    ));
    for ty in [0.6_f32, 1.8] {
        prims.push(prim(
            cuboid_tapered([0.22, 0.12, 1.5], 0.0, iron(IRON_DARK)),
            [front + 0.08, foot_h + ty, 0.0],
            id_quat(),
        ));
    }

    // Stone cross over the rear gable peak.
    let cross_x = -l * 0.5 + 0.4;
    let cross_y = wall_top + 2.9;
    prims.push(prim(
        solid(cuboid_tapered([0.18, 1.1, 0.18], 0.0, stone(STONE_PALE))),
        [cross_x, cross_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.18, 0.18, 0.7], 0.0, stone(STONE_PALE))),
        [cross_x, cross_y + 0.2, 0.0],
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
        assert_sanitize_stable(&Chapel.build(""), "chapel");
    }
}
