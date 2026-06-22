//! Trade stall — a Medieval prop. A trestle market stall: an oak board on
//! trestles under a striped wool awning, a back display board hung with
//! strings of onions and a balance scale, and crates, sacks, loaves and a
//! wheel of cheese set out. The everyday commerce of the square, flavoured to
//! the burgh rather than the generic civic stall.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLOTH_CREAM, HERALD_GOLD, HERALD_RED, IRON_DARK, WOOD_DARK, WOOD_OAK, cloth, iron, timber,
};

pub struct TradeStall;

impl CatalogueEntry for TradeStall {
    fn slug(&self) -> &'static str {
        "trade_stall"
    }
    fn name(&self) -> &'static str {
        "Trade Stall"
    }
    fn description(&self) -> &'static str {
        "Trestle market board under a striped awning, hung with wares and set out with goods."
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

    // Awning posts: tall at the back (−Z), shorter at the front (+Z).
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

    // Back display board between the tall posts, hung with strings of onions.
    prims.push(prim(
        solid(cuboid_tapered([2.1, 0.9, 0.06], 0.0, timber(WOOD_DARK))),
        [0.0, 1.85, -0.62],
        id_quat(),
    ));
    for sx in [-0.7_f32, 0.0, 0.7] {
        prims.push(prim(
            cuboid_tapered([0.12, 0.7, 0.12], 0.4, cloth(HERALD_GOLD, WOOD_OAK)),
            [sx, 1.55, -0.55],
            id_quat(),
        ));
    }

    // A balance scale hung from the front of the awning.
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.04, 0.04], 0.0, iron(IRON_DARK))),
        [0.85, 1.55, 0.45],
        id_quat(),
    ));
    for sx in [-0.22_f32, 0.22] {
        prims.push(prim(
            solid(cylinder_tapered(0.12, 0.05, 10, 0.0, iron(IRON_DARK))),
            [0.85 + sx, 1.4, 0.45],
            id_quat(),
        ));
    }

    // Wares on the board: two crates, a sack, a wheel of cheese, loaves.
    for (sx, sz) in [(-0.7_f32, 0.0_f32), (0.7, 0.1)] {
        prims.push(prim(
            solid(cuboid_tapered([0.42, 0.42, 0.42], 0.0, timber(WOOD_OAK))),
            [sx, table_y + 0.27, sz],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([0.34, 0.4, 0.3], 0.35, cloth(CLOTH_CREAM, WOOD_OAK)),
        [-0.2, table_y + 0.26, -0.2],
        id_quat(),
    ));
    // Wheel of cheese on its side.
    let mut cheese = prim(
        solid(cylinder_tapered(
            0.22,
            0.16,
            14,
            0.0,
            cloth(HERALD_GOLD, HERALD_GOLD),
        )),
        [0.15, table_y + 0.14, 0.28],
        quat_x(std::f32::consts::FRAC_PI_2),
    );
    cheese.children.push(prim(
        torus(0.02, 0.22, iron(IRON_DARK)),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    prims.push(cheese);
    // A couple of loaves.
    for sx in [-0.55_f32, -0.3] {
        prims.push(prim(
            cuboid_tapered([0.22, 0.16, 0.14], 0.4, timber(WOOD_OAK)),
            [sx, table_y + 0.12, 0.32],
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
