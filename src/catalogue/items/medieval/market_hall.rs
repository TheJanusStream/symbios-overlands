//! Market hall — a Medieval secondary. The classic open-ground market
//! house: a stone-pillared arcade of round arches left open at street level
//! for traders' stalls, a jettied timber-framed upper floor with daub infill
//! where the guild meets, a steep tiled gable roof, and a little open bell
//! lantern on the ridge that calls the market. The covered market that
//! anchors a burgh's square.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, id_quat, prim, quat_x,
    quat_y, solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DAUB_CREAM, IRON_DARK, SLATE_GREY, STONE_GREY, STONE_PALE, WOOD_DARK, WOOD_OAK, daub, iron,
    rough_stone, shingle, stone, timber,
};

pub struct MarketHall;

impl CatalogueEntry for MarketHall {
    fn slug(&self) -> &'static str {
        "market_hall"
    }
    fn name(&self) -> &'static str {
        "Market Hall"
    }
    fn description(&self) -> &'static str {
        "Round-arched stone market arcade under a jettied timber upper floor and a bell lantern."
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
            clearance: 7.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A semicircular stone arch spanning along X in the wall (XY) plane at
/// `z = zf`, springing at `y` with radius `r` — the Romanesque arcade head.
fn round_arch_x(cx: f32, y: f32, zf: f32, r: f32, mat: SovereignMaterialSettings) -> Generator {
    prim(
        with_cut(torus(0.16, r, mat), [0.0, 0.5], [0.0, 1.0], 0.0),
        [cx, y, zf],
        quat_x(-FRAC_PI_2),
    )
}

fn build_tree() -> Generator {
    let l = 8.0_f32; // along X; long arcade faces ±Z (camera = −Z)
    let w = 6.0_f32; // along Z
    let foot_h = 0.3;
    let ground_h = 2.6; // open arcade height
    let deck_y = foot_h + ground_h;
    let upper_h = 2.6;

    let pier_xs = [-3.3_f32, -1.1, 1.1, 3.3];
    let pier_zs = [-(w * 0.5 - 0.6), w * 0.5 - 0.6];
    let spring_y = foot_h + 1.4;
    let arch_r = 1.1;

    let mut prims = vec![
        // Cobbled footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, foot_h, w + 0.6],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Open arcade: 4×2 grid of stone piers carrying round arches.
    for &px in &pier_xs {
        for &pz in &pier_zs {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.5, spring_y - foot_h, 0.5],
                    0.0,
                    stone(STONE_PALE),
                )),
                [px, foot_h + (spring_y - foot_h) * 0.5, pz],
                id_quat(),
            ));
        }
    }
    // Round arches between adjacent piers on both long faces.
    for &pz in &pier_zs {
        for cx in [-2.2_f32, 0.0, 2.2] {
            prims.push(round_arch_x(cx, spring_y, pz, arch_r, stone(STONE_PALE)));
        }
    }

    // Timber floor deck spanning the arcade.
    prims.push(prim(
        solid(cuboid_tapered([l, 0.4, w], 0.0, timber(WOOD_OAK))),
        [0.0, deck_y + 0.2, 0.0],
        id_quat(),
    ));

    // Jettied (oversailing) daub-infilled upper storey.
    let upper_y = deck_y + 0.4 + upper_h * 0.5;
    let upper_top = deck_y + 0.4 + upper_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.8, upper_h, w + 0.8],
            0.0,
            daub(DAUB_CREAM),
        )),
        [0.0, upper_y, 0.0],
        id_quat(),
    ));
    // Exposed timber frame on the upper storey: corner posts, rails, studs.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            cuboid_tapered([0.28, upper_h, 0.28], 0.0, timber(WOOD_DARK)),
            [sx * (l * 0.5 + 0.28), upper_y, sz * (w * 0.5 + 0.28)],
            id_quat(),
        ));
    }
    for sz in [-1.0_f32, 1.0] {
        let zf = sz * (w * 0.5 + 0.44);
        // Top and bottom rails.
        for dy in [upper_h * 0.5 - 0.18, -upper_h * 0.5 + 0.18] {
            prims.push(prim(
                cuboid_tapered([l + 0.9, 0.22, 0.1], 0.0, timber(WOOD_DARK)),
                [0.0, upper_y + dy, zf],
                id_quat(),
            ));
        }
        // Vertical studs.
        for sx in [-2.4_f32, -0.8, 0.8, 2.4] {
            prims.push(prim(
                cuboid_tapered([0.13, upper_h - 0.3, 0.09], 0.0, timber(WOOD_DARK)),
                [sx, upper_y, zf],
                id_quat(),
            ));
        }
        // Two shuttered windows per long face.
        for sx in [-1.6_f32, 1.6] {
            prims.push(prim(
                solid(cuboid_tapered([0.9, 0.9, 0.1], 0.0, timber(WOOD_OAK))),
                [sx, upper_y + 0.1, zf - sz * 0.02],
                id_quat(),
            ));
        }
    }

    // Steep tiled gable roof (ridge ‖ X) over the upper storey + gable infill.
    let roof_rise = 2.4;
    prims.push(gable_roof(
        [l + 1.3, roof_rise, w + 1.3],
        [0.0, upper_top + roof_rise * 0.5, 0.0],
        shingle(SLATE_GREY),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [0.3, roof_rise, w + 0.8],
                [0.0, 0.94],
                daub(DAUB_CREAM),
            )),
            [sx * (l * 0.5 + 0.36), upper_top + roof_rise * 0.5, 0.0],
            id_quat(),
        ));
    }
    let ridge_y = upper_top + roof_rise;
    prims.push(prim(
        solid(cuboid_tapered([l + 1.0, 0.2, 0.24], 0.0, timber(WOOD_DARK))),
        [0.0, ridge_y, 0.0],
        id_quat(),
    ));

    // ── Open bell lantern straddling the ridge ──
    let lant_y = ridge_y + 0.1;
    let lant_hw = 0.7;
    // Four corner posts.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 1.2, 0.12], 0.0, timber(WOOD_OAK))),
            [sx * lant_hw, lant_y + 0.6, sz * lant_hw],
            id_quat(),
        ));
    }
    // Pyramidal slate cap.
    prims.push(prim(
        solid(cone(lant_hw * 1.5, 1.0, 4, shingle(SLATE_GREY))),
        [0.0, lant_y + 1.7, 0.0],
        quat_y(FRAC_PI_2 * 0.5),
    ));
    // Bell hung in the open lantern.
    prims.push(prim(
        solid(cylinder_tapered(0.26, 0.4, 12, -0.35, iron(IRON_DARK))),
        [0.0, lant_y + 0.7, 0.0],
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
        assert_sanitize_stable(&MarketHall.build(""), "market_hall");
    }
}
