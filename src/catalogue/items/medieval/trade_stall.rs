//! Trade stall — a Medieval prop. A trestle market stall: an oak board on
//! trestles under a striped wool awning sloping to the customer side, with
//! crates and sacks of wares set out. The everyday commerce of the square,
//! flavoured to the burgh rather than the generic civic stall.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLOTH_CREAM, HERALD_RED, WOOD_DARK, WOOD_OAK, cloth, timber};

pub struct TradeStall;

impl CatalogueEntry for TradeStall {
    fn slug(&self) -> &'static str {
        "trade_stall"
    }
    fn name(&self) -> &'static str {
        "Trade Stall"
    }
    fn description(&self) -> &'static str {
        "Trestle market board under a striped wool awning, set out with crates and sacks."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let table_y = 0.95;

    let mut prims = vec![
        // Trestle board — the root.
        prim(
            solid(cuboid_tapered([2.2, 0.12, 1.0], 0.0, timber(WOOD_OAK))),
            [0.0, table_y, 0.0],
            id_quat(),
        ),
    ];
    // Four table legs.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, table_y, 0.1], 0.0, timber(WOOD_DARK))),
            [sx * 1.0, table_y * 0.5, sz * 0.4],
            id_quat(),
        ));
    }

    // Awning posts: tall at the back (-Z), shorter at the front (+Z).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 2.3, 0.1], 0.0, timber(WOOD_DARK))),
            [sx * 1.05, 1.15, -0.55],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.1, 1.7, 0.1], 0.0, timber(WOOD_DARK))),
            [sx * 1.05, 0.85, 0.55],
            id_quat(),
        ));
    }

    // Striped wool awning sloping down toward the customer side.
    prims.push(prim(
        cuboid_tapered([2.5, 0.07, 1.5], 0.0, cloth(HERALD_RED, CLOTH_CREAM)),
        [0.0, 2.0, 0.0],
        quat_x(0.35),
    ));

    // Wares on the board: two crates and two sacks.
    for (sx, sz) in [(-0.7_f32, 0.0_f32), (0.7, 0.1)] {
        prims.push(prim(
            solid(cuboid_tapered([0.42, 0.42, 0.42], 0.0, timber(WOOD_OAK))),
            [sx, table_y + 0.27, sz],
            id_quat(),
        ));
    }
    for (sx, sz) in [(0.0_f32, -0.2_f32), (0.25, 0.25)] {
        prims.push(prim(
            cuboid_tapered([0.34, 0.4, 0.3], 0.35, cloth(CLOTH_CREAM, WOOD_OAK)),
            [sx, table_y + 0.26, sz],
            id_quat(),
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
        assert_sanitize_stable(&TradeStall.build(""), "trade_stall");
    }
}
