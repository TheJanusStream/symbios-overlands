//! Wattle hovel — the Medieval *poor* landmark. A small crooked cottage of
//! lime-washed wattle-and-daub over a low fieldstone footing, a crude
//! timber-framed gable with an exposed cruck brace, a plank door and a
//! shuttered window in the long wall, and a heavy steep thatch with a smoke
//! hole breathing hearth smoke. The cottar counterpart to the
//! [`medieval_castle`](super::medieval_castle): same theme, opposite end of
//! the prosperity axis (`Poor`), so a destitute Medieval room grows this
//! instead of the keep.

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, id_quat, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DAUB_CREAM, STONE_GREY, THATCH_STRAW, WOOD_DARK, daub, fx, rough_stone, thatch, timber,
};

pub struct WattleHovel;

impl CatalogueEntry for WattleHovel {
    fn slug(&self) -> &'static str {
        "wattle_hovel"
    }
    fn name(&self) -> &'static str {
        "Wattle Hovel"
    }
    fn description(&self) -> &'static str {
        "Crooked wattle-and-daub cottage under a steep heavy thatch, hearth smoke seeping out."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_POOR
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
    let l = 6.0_f32; // along X; long walls face ±Z (camera = −Z)
    let w = 4.0_f32; // along Z
    let foot_h = 0.35;
    let wall_h = 2.2;
    let wall_top = foot_h + wall_h;
    let roof_rise = 2.2;
    let ridge_y = wall_top + roof_rise;

    let mut prims = vec![
        // Low fieldstone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.8, foot_h, w + 0.8],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Daub walls.
        prim(
            solid(cuboid_tapered([l, wall_h, w], 0.05, daub(DAUB_CREAM))),
            [0.0, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Steep heavy thatch (ridge ‖ X, A-frame slopes face ±Z) with a wide
    // overhang — replaces the old flat-topped frustum mound.
    prims.push(gable_roof(
        [l + 1.0, roof_rise, w + 1.3],
        [0.0, wall_top + roof_rise * 0.5, 0.0],
        thatch(THATCH_STRAW),
    ));
    // Triangular daub gable-end infill on the ±X ends (no daylight under the
    // thatch).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [0.3, roof_rise, w],
                [0.0, 0.94],
                daub(DAUB_CREAM),
            )),
            [sx * (l * 0.5 - 0.02), wall_top + roof_rise * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Ridge pole capping the thatch peak — sits proud above the apex so its
    // faces never graze the converging slopes (no coplanar z-fight).
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.6, 0.16, 0.18],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, ridge_y + 0.11, 0.0],
        id_quat(),
    ));

    // Crude exposed timber frame: corner posts and a sagging mid-rail.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            cuboid_tapered([0.22, wall_h, 0.22], 0.0, timber(WOOD_DARK)),
            [
                sx * (l * 0.5 - 0.1),
                foot_h + wall_h * 0.5,
                sz * (w * 0.5 - 0.1),
            ],
            id_quat(),
        ));
    }
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([l, 0.16, 0.09], 0.0, timber(WOOD_DARK)),
            [0.0, foot_h + wall_h * 0.55, sz * (w * 0.5 + 0.02)],
            id_quat(),
        ));
    }
    // Exposed cruck cross-braces on the −Z (camera) wall, a leaning timber X.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([1.7, 0.12, 0.08], 0.0, timber(WOOD_DARK)),
            [s * 1.4, foot_h + wall_h * 0.5, -(w * 0.5 + 0.03)],
            quat_z(s * 0.62),
        ));
    }

    // Plank door in the −Z long wall (camera face).
    prims.push(prim(
        solid(cuboid_tapered([1.0, 1.7, 0.15], 0.0, timber(WOOD_DARK))),
        [-1.4, foot_h + 0.85, -(w * 0.5 + 0.05)],
        id_quat(),
    ));
    // A small shuttered window beside the door.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.6, 0.12], 0.0, timber(WOOD_DARK))),
        [1.5, foot_h + 1.3, -(w * 0.5 + 0.04)],
        id_quat(),
    ));

    // Timber smoke-hole curb on the rear (+Z) thatch slope near the ridge.
    let hole_x = 1.6;
    prims.push(prim(
        solid(cuboid_tapered([0.8, 0.4, 0.8], 0.2, timber(WOOD_DARK))),
        [hole_x, wall_top + roof_rise - 0.5, 0.8],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: hearth smoke seeping from the roof hole.
    root.children.push(fx::hearth_smoke(
        [hole_x, wall_top + roof_rise - 0.1, 0.8],
        0x70F0_DA11,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WattleHovel.build(""), "wattle_hovel");
    }
}
