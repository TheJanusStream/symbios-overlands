//! Tea house — a Feudal-Japan secondary. A small raised timber pavilion
//! with shoji-paper walls, an open front veranda, and a hip tile roof, set
//! beside a stone water basin (tsukubai) fed by a bamboo spout. The basin
//! trickles and a thread of incense rises — the quiet of the tea garden.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BAMBOO_TAN, PAPER_CREAM, STONE_GREY, TILE_SLATE, TIMBER_BROWN, TIMBER_DARK, WATER_BLUE, fx,
    paper, roof_tile, rough_stone, stone, timber, water,
};

pub struct TeaHouse;

impl CatalogueEntry for TeaHouse {
    fn slug(&self) -> &'static str {
        "tea_house"
    }
    fn name(&self) -> &'static str {
        "Tea House"
    }
    fn description(&self) -> &'static str {
        "Raised timber pavilion with shoji walls beside a trickling stone basin."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_BAND
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
    let plat_top = 1.0;
    let post_h = 3.0;
    let eave = plat_top + post_h;
    let corners = [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered([6.5, 0.3, 5.5], 0.0, stone(STONE_GREY))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Stilts and raised veranda platform.
    for (sx, sz) in corners {
        prims.push(prim(
            solid(cuboid_tapered([0.35, 0.6, 0.35], 0.0, timber(TIMBER_BROWN))),
            [sx * 2.6, 0.45, sz * 2.1],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([6.0, 0.4, 5.0], 0.0, timber(TIMBER_BROWN))),
        [0.0, plat_top - 0.2, 0.0],
        id_quat(),
    ));

    // Corner posts.
    for (sx, sz) in corners {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.25, post_h, 0.25],
                0.0,
                timber(TIMBER_BROWN),
            )),
            [sx * 2.7, plat_top + post_h * 0.5, sz * 2.2],
            id_quat(),
        ));
    }

    // Shoji-paper walls: back (+Z) and two sides; the front (−Z hero face)
    // left open to the veranda.
    let wall_h = post_h - 0.4;
    let wall_cy = plat_top + wall_h * 0.5;
    prims.push(prim(
        solid(cuboid_tapered([5.4, wall_h, 0.1], 0.0, paper(PAPER_CREAM))),
        [0.0, wall_cy, 2.2],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, wall_h, 4.4], 0.0, paper(PAPER_CREAM))),
            [sx * 2.7, wall_cy, 0.0],
            id_quat(),
        ));
    }

    // Shoji lattice (kumiko): dark mullions proud of each paper panel — four
    // uprights and two rails per wall.
    let zf = 2.26;
    for i in 0..4 {
        let x = -2.0 + i as f32 * (4.0 / 3.0);
        prims.push(prim(
            cuboid_tapered([0.06, wall_h, 0.03], 0.0, timber(TIMBER_DARK)),
            [x, wall_cy, zf],
            id_quat(),
        ));
    }
    for sy in [-0.75_f32, 0.75] {
        prims.push(prim(
            cuboid_tapered([5.2, 0.06, 0.03], 0.0, timber(TIMBER_DARK)),
            [0.0, wall_cy + sy, zf],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        let xf = sx * 2.76;
        for i in 0..4 {
            let z = -1.6 + i as f32 * (3.2 / 3.0);
            prims.push(prim(
                cuboid_tapered([0.03, wall_h, 0.06], 0.0, timber(TIMBER_DARK)),
                [xf, wall_cy, z],
                id_quat(),
            ));
        }
        for sy in [-0.75_f32, 0.75] {
            prims.push(prim(
                cuboid_tapered([0.03, 0.06, 4.2], 0.0, timber(TIMBER_DARK)),
                [xf, wall_cy + sy, 0.0],
                id_quat(),
            ));
        }
    }

    // Hip tile roof: deep eave board, a tapered cap, and upswept corners.
    prims.push(prim(
        solid(cuboid_tapered([7.8, 0.18, 6.8], 0.0, roof_tile(TILE_SLATE))),
        [0.0, eave + 0.1, 0.0],
        id_quat(),
    ));
    let cap_h = 1.4;
    prims.push(prim(
        solid(cuboid_tapered(
            [7.4, cap_h, 6.4],
            0.5,
            roof_tile(TILE_SLATE),
        )),
        [0.0, eave + 0.18 + cap_h * 0.5, 0.0],
        id_quat(),
    ));
    let (ehx, ehz) = (3.9_f32, 3.4_f32);
    for (sx, sz) in corners {
        let theta = (-sx).atan2(-sz);
        prims.push(prim(
            wedge([1.4, 0.55, 1.4], roof_tile(TILE_SLATE)),
            [sx * (ehx - 0.5), eave + 0.18, sz * (ehz - 0.5)],
            quat_y(theta),
        ));
    }

    // Tsukubai water basin on the front (−Z), with a bamboo spout.
    let basin_x = 3.6;
    let basin_z = -1.6;
    prims.push(prim(
        solid(cylinder_tapered(
            0.45,
            0.7,
            12,
            0.1,
            rough_stone(STONE_GREY),
        )),
        [basin_x, 0.35, basin_z],
        id_quat(),
    ));
    let mut basin_water = prim(
        cuboid_tapered([0.6, 0.06, 0.6], 0.0, water(WATER_BLUE)),
        [basin_x, 0.7, basin_z],
        id_quat(),
    );
    basin_water.audio = fx::water_basin();
    prims.push(basin_water);
    // Bamboo spout leaning over the basin.
    prims.push(prim(
        solid(cylinder_tapered(0.05, 1.0, 6, 0.0, timber(BAMBOO_TAN))),
        [basin_x + 0.4, 0.95, basin_z],
        quat_x(0.5),
    ));

    let mut root = assemble(prims);
    // Signature life: a thread of incense rising by the basin.
    root.children
        .push(fx::incense_wisp([basin_x - 0.3, 0.8, basin_z], 0x7EA0_CE11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TeaHouse.build(""), "tea_house");
    }
}
