//! Wattle hovel — the Medieval *poor* landmark. A small crooked cottage of
//! lime-washed wattle-and-daub over a low fieldstone footing, a crude
//! timber frame, a plank door, and a heavy thatched roof with a smoke hole
//! breathing hearth smoke. The cottar counterpart to the
//! [`medieval_castle`](super::medieval_castle): same theme, opposite end of
//! the prosperity axis (`Poor`), so a destitute Medieval room grows this
//! instead of the keep.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
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
        "Crooked wattle-and-daub cottage under a heavy thatch, hearth smoke seeping out."
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
    let l = 6.0_f32; // along X, door faces +X
    let w = 4.0_f32; // along Z
    let foot_h = 0.35;
    let wall_h = 2.2;
    let wall_top = foot_h + wall_h;
    let roof_h = 2.0;

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
            cuboid_tapered([l, 0.18, 0.1], 0.0, timber(WOOD_DARK)),
            [0.0, foot_h + wall_h * 0.55, sz * (w * 0.5 + 0.02)],
            id_quat(),
        ));
    }

    // Plank door in the near gable.
    prims.push(prim(
        solid(cuboid_tapered([0.15, 1.7, 1.0], 0.0, timber(WOOD_DARK))),
        [l * 0.5 + 0.05, foot_h + 0.85, 0.0],
        id_quat(),
    ));

    // Heavy thatched hip roof with a wide overhang.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.4, roof_h, w + 1.4],
            0.4,
            thatch(THATCH_STRAW),
        )),
        [0.0, wall_top + roof_h * 0.5, 0.0],
        id_quat(),
    ));
    // Timber smoke-hole curb near the ridge.
    let hole_x = 1.6;
    prims.push(prim(
        solid(cuboid_tapered([0.9, 0.45, 0.9], 0.2, timber(WOOD_DARK))),
        [hole_x, wall_top + roof_h - 0.1, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: hearth smoke seeping from the roof hole.
    root.children.push(fx::hearth_smoke(
        [hole_x, wall_top + roof_h + 0.3, 0.0],
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
