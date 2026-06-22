//! Boathouse — a Nordic secondary. An open-fronted timber shed on a stone
//! slipway, roofed in steep thatch and propped on two front posts, where a
//! crew drags its longship up out of the water for the winter. The mouth
//! faces the shore (the -Z hero face) so it reads as a working naust, not a
//! sealed barn.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_GREY, THATCH_STRAW, WOOD_DARK, WOOD_WARM, gable_roof, stone, thatch, timber};

pub struct Boathouse;

impl CatalogueEntry for Boathouse {
    fn slug(&self) -> &'static str {
        "boathouse"
    }
    fn name(&self) -> &'static str {
        "Boathouse"
    }
    fn description(&self) -> &'static str {
        "Open-fronted timber naust on a stone slipway, thatched and post-propped."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 10.0_f32; // width along the shore (X) — ridge runs this way
    let d = 6.0_f32; // depth inland (Z), open toward -Z (the camera/shore)
    let foot_h = 0.4;
    let wall_h = 4.0;
    let wall_top = foot_h + wall_h;
    let roof_h = 3.0;

    let mut prims = vec![
        // Stone slipway footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 1.0, foot_h, d + 1.0],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Back wall (+Z, away from the shore).
    prims.push(prim(
        solid(cuboid_tapered([l, wall_h, 0.35], 0.0, timber(WOOD_WARM))),
        [0.0, foot_h + wall_h * 0.5, d * 0.5 - 0.18],
        id_quat(),
    ));
    // Side walls, carried up into the gable triangle.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.35, wall_h, d], 0.0, timber(WOOD_DARK))),
            [sx * (l * 0.5 - 0.18), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [0.35, roof_h, d],
                [0.0, 0.94],
                timber(WOOD_DARK),
            )),
            [sx * (l * 0.5 - 0.18), wall_top + roof_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Front posts at the open mouth (-Z).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.24, wall_h, 8, 0.05, timber(WOOD_DARK))),
            [
                sx * (l * 0.5 - 0.3),
                foot_h + wall_h * 0.5,
                -(d * 0.5 - 0.3),
            ],
            id_quat(),
        ));
    }
    // Lintel across the front posts.
    prims.push(prim(
        solid(cuboid_tapered([l, 0.45, 0.4], 0.0, timber(WOOD_DARK))),
        [0.0, wall_top - 0.2, -(d * 0.5 - 0.3)],
        id_quat(),
    ));

    // Steep thatched gable roof, overhanging the open front.
    prims.push(gable_roof(
        [l + 1.4, roof_h, d + 1.6],
        [0.0, wall_top + roof_h * 0.5, -0.2],
        thatch(THATCH_STRAW),
    ));
    // Ridge beam.
    prims.push(prim(
        solid(cuboid_tapered([l + 1.6, 0.3, 0.4], 0.0, timber(WOOD_DARK))),
        [0.0, wall_top + roof_h - 0.1, -0.2],
        id_quat(),
    ));

    // Timber slipway rollers leading out the open front (-Z) toward the
    // water.
    for k in 0..3 {
        let z = -(d * 0.5 + 0.4 + k as f32 * 0.9);
        prims.push(prim(
            solid(cuboid_tapered([l - 1.0, 0.2, 0.32], 0.0, timber(WOOD_WARM))),
            [0.0, foot_h * 0.5 + 0.06, z],
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
        assert_sanitize_stable(&Boathouse.build(""), "boathouse");
    }
}
