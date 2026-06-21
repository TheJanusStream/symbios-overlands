//! Dojo — a Feudal-Japan secondary. A long, low training hall: a raised
//! timber floor and plaster walls between a heavy post frame, fronted by
//! sliding shoji panels around an open central entrance, under a broad hip
//! tile roof. The martial counterpart to the contemplative tea house.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_y, solid, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    PAPER_CREAM, PLASTER_WHITE, STONE_GREY, TILE_SLATE, TIMBER_BROWN, TIMBER_DARK, paper, plaster,
    roof_tile, stone, timber,
};

pub struct Dojo;

impl CatalogueEntry for Dojo {
    fn slug(&self) -> &'static str {
        "dojo"
    }
    fn name(&self) -> &'static str {
        "Dojo"
    }
    fn description(&self) -> &'static str {
        "Long timber-framed training hall with shoji front under a hip tile roof."
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
            clearance: 8.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 12.0_f32;
    let w = 8.0_f32;
    let floor_top = 0.8;
    let wall_h = 3.4;
    let wall_top = floor_top + wall_h;
    let corners = [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let front_z = -(w * 0.5 - 0.15);
    let back_z = w * 0.5 - 0.15;

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.8, 0.4, w + 0.8],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Raised timber floor.
        prim(
            solid(cuboid_tapered([l, 0.5, w], 0.0, timber(TIMBER_BROWN))),
            [0.0, floor_top - 0.25, 0.0],
            id_quat(),
        ),
    ];

    // Plaster back wall (+Z) and side walls.
    prims.push(prim(
        solid(cuboid_tapered(
            [l, wall_h, 0.3],
            0.0,
            plaster(PLASTER_WHITE),
        )),
        [0.0, floor_top + wall_h * 0.5, back_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.3, wall_h, w],
                0.0,
                plaster(PLASTER_WHITE),
            )),
            [sx * (l * 0.5 - 0.15), floor_top + wall_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Front post frame (−Z hero face): four posts framing two shoji panels
    // and a central open entrance.
    for x in [-l * 0.5 + 0.3, -1.8, 1.8, l * 0.5 - 0.3] {
        prims.push(prim(
            solid(cuboid_tapered([0.3, wall_h, 0.3], 0.0, timber(TIMBER_DARK))),
            [x, floor_top + wall_h * 0.5, front_z],
            id_quat(),
        ));
    }
    // Shoji panels flanking the entrance, each gridded with a kumiko lattice.
    let sh_h = wall_h - 0.4;
    let sh_cy = floor_top + sh_h * 0.5;
    let zf = front_z - 0.06;
    for (x0, x1) in [(-l * 0.5 + 0.3, -1.8), (1.8, l * 0.5 - 0.3)] {
        let cx = (x0 + x1) * 0.5;
        let pw = (x1 - x0).abs() - 0.3;
        prims.push(prim(
            solid(cuboid_tapered([pw, sh_h, 0.08], 0.0, paper(PAPER_CREAM))),
            [cx, sh_cy, front_z],
            id_quat(),
        ));
        for i in 0..4 {
            let lx = cx - pw * 0.5 + 0.2 + i as f32 * (pw - 0.4) / 3.0;
            prims.push(prim(
                cuboid_tapered([0.05, sh_h, 0.03], 0.0, timber(TIMBER_DARK)),
                [lx, sh_cy, zf],
                id_quat(),
            ));
        }
        for sy in [-0.8_f32, 0.8] {
            prims.push(prim(
                cuboid_tapered([pw, 0.05, 0.03], 0.0, timber(TIMBER_DARK)),
                [cx, sh_cy + sy, zf],
                id_quat(),
            ));
        }
    }
    // Lintel across the front.
    prims.push(prim(
        solid(cuboid_tapered([l, 0.35, 0.3], 0.0, timber(TIMBER_DARK))),
        [0.0, wall_top - 0.2, front_z],
        id_quat(),
    ));

    // Broad hip tile roof: deep eave board, tapered cap, upswept corners.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 2.4, 0.2, w + 2.4],
            0.0,
            roof_tile(TILE_SLATE),
        )),
        [0.0, wall_top + 0.1, 0.0],
        id_quat(),
    ));
    let cap_h = 2.0;
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 2.0, cap_h, w + 2.0],
            0.5,
            roof_tile(TILE_SLATE),
        )),
        [0.0, wall_top + 0.2 + cap_h * 0.5, 0.0],
        id_quat(),
    ));
    let (ehx, ehz) = ((l + 2.4) * 0.5, (w + 2.4) * 0.5);
    for (sx, sz) in corners {
        let theta = (-sx).atan2(-sz);
        prims.push(prim(
            wedge([1.8, 0.6, 1.8], roof_tile(TILE_SLATE)),
            [sx * (ehx - 0.7), wall_top + 0.2, sz * (ehz - 0.7)],
            quat_y(theta),
        ));
    }
    // Ridge beam along the roof crown.
    prims.push(prim(
        solid(cuboid_tapered(
            [l * 0.55, 0.4, 0.7],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, wall_top + cap_h + 0.1, 0.0],
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
        assert_sanitize_stable(&Dojo.build(""), "dojo");
    }
}
