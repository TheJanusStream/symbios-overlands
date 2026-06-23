//! Gas station — the Roadside landmark and the kit's lit hero. A flat
//! steel-columned canopy lit from beneath shelters a pump island beside a
//! brick convenience store, and a tall pylon price sign glows out over the
//! highway. ~20 m of forecourt, so it anchors the strip and reads as the
//! filling station from across the home region. Its canopy underside, pump
//! faces and pylon sign are the trim escalation's ruin pass snuffs to a
//! dark, abandoned lot.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the forecourt pad.

use crate::catalogue::items::modern_city::curtain_wall;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, foundation_block, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ASPHALT_DARK, BRICK_TAN, CANOPY_LIT, CHROME_BRIGHT, CONCRETE_GREY, CORRUGATED_GREY,
    ENAMEL_CREAM, ENAMEL_RED, GLASS_TINT, NEON_RED, SIGN_AMBER, STEEL_GREY, asphalt, brick, chrome,
    concrete, corrugated, enamel, fx, glass, sign_board, steel,
};

pub struct GasStation;

impl CatalogueEntry for GasStation {
    fn slug(&self) -> &'static str {
        "gas_station"
    }
    fn name(&self) -> &'static str {
        "Gas Station"
    }
    fn description(&self) -> &'static str {
        "Lit pump-island canopy beside a brick store, with a glowing pylon price sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 16.0,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_top = 0.2_f32;

    let mut prims = vec![
        // Asphalt forecourt pad — the root.
        prim(
            solid(cuboid_tapered(
                [20.0, pad_top, 16.0],
                0.0,
                asphalt(ASPHALT_DARK),
            )),
            [0.0, pad_top * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(8.4, 6.4, [-5.0, 4.0], 1.5));

    // --- Brick convenience store at the back (+Z), its glazed storefront and
    // lit name band turned to face the −Z camera front across the forecourt. ---
    let store_c = [-5.0_f32, 4.0];
    let store_front = store_c[1] - 3.0; // store's −Z (camera-facing) wall plane
    prims.push(prim(
        solid(cuboid_tapered([8.0, 3.4, 6.0], 0.0, brick(BRICK_TAN))),
        [store_c[0], pad_top + 1.7, store_c[1]],
        id_quat(),
    ));
    // Parapet coping, proud of the wall on every side.
    prims.push(prim(
        solid(cuboid_tapered(
            [8.4, 0.4, 6.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [store_c[0], pad_top + 3.6, store_c[1]],
        id_quat(),
    ));
    // Mullioned glazed storefront on the −Z face.
    for g in curtain_wall(
        [store_c[0], pad_top + 1.7, store_front - 0.2],
        [6.4, 2.4],
        (4, 1),
        -0.22,
        glass(GLASS_TINT, 1.5),
        steel(STEEL_GREY),
    ) {
        prims.push(g);
    }
    // Brick bulkhead under the glazing.
    prims.push(prim(
        solid(cuboid_tapered([6.6, 0.5, 0.4], 0.0, brick(BRICK_TAN))),
        [store_c[0], pad_top + 0.25, store_front - 0.12],
        id_quat(),
    ));
    // Glazed door at one end of the storefront.
    prims.push(prim(
        cuboid_tapered([1.0, 2.0, 0.12], 0.0, glass(GLASS_TINT, 1.7)),
        [store_c[0] - 2.5, pad_top + 1.0, store_front - 0.3],
        id_quat(),
    ));
    // Lit name band high on the wall, just under the parapet.
    for g in sign_board(
        [store_c[0], pad_top + 3.0, store_front - 0.5],
        [4.2, 0.7],
        (3, 1),
        SIGN_AMBER,
        2.2,
        -1.0,
    ) {
        prims.push(g);
    }

    // --- Pump-island canopy: four steel columns, a corrugated deck, a chrome
    // fascia, a dark soffit with recessed flood cells (not one blown-out slab)
    // and a lit brand strip on the −Z fascia. ---
    let ci = [4.0_f32, 0.0]; // canopy centre (X, Z)
    let col_y = pad_top + 2.5;
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.45, 5.0, 0.45], 0.0, steel(STEEL_GREY))),
                [ci[0] + sx * 3.5, col_y, ci[1] + sz * 2.6],
                id_quat(),
            ));
        }
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [10.0, 0.6, 8.0],
            0.0,
            corrugated(CORRUGATED_GREY),
        )),
        [ci[0], pad_top + 5.3, ci[1]],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([10.4, 0.5, 8.4], 0.0, chrome(CHROME_BRIGHT))),
        [ci[0], pad_top + 4.8, ci[1]],
        id_quat(),
    ));
    // Red brand stripe, proud of the chrome fascia.
    prims.push(prim(
        cuboid_tapered([10.6, 0.22, 8.6], 0.0, enamel(ENAMEL_RED)),
        [ci[0], pad_top + 4.9, ci[1]],
        id_quat(),
    ));
    // Dark soffit + a grid of recessed flood cells underneath it.
    prims.push(prim(
        solid(cuboid_tapered(
            [9.4, 0.16, 7.4],
            0.0,
            enamel([0.12, 0.12, 0.13]),
        )),
        [ci[0], pad_top + 4.55, ci[1]],
        id_quat(),
    ));
    for gx in [-3.0_f32, 0.0, 3.0] {
        for gz in [-2.2_f32, 2.2] {
            prims.push(prim(
                cuboid_tapered([2.2, 0.1, 1.6], 0.0, glow(CANOPY_LIT, 2.2)),
                [ci[0] + gx, pad_top + 4.46, ci[1] + gz],
                id_quat(),
            ));
        }
    }
    // Lit brand strip on the −Z fascia, facing the camera.
    for g in sign_board(
        [ci[0], pad_top + 4.85, ci[1] - 4.35],
        [4.4, 0.5],
        (4, 1),
        SIGN_AMBER,
        2.0,
        -1.0,
    ) {
        prims.push(g);
    }

    // --- Pump island: a concrete curb, two enamel pumps with red toppers,
    // segmented amber price faces (−Z) and chrome nozzles. ---
    prims.push(prim(
        solid(cuboid_tapered(
            [6.0, 0.3, 2.0],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [ci[0], pad_top + 0.15, ci[1]],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        let px = ci[0] + sx * 1.7;
        prims.push(prim(
            solid(cuboid_tapered([0.8, 1.5, 0.6], 0.0, enamel(ENAMEL_CREAM))),
            [px, pad_top + 0.3 + 0.75, ci[1]],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.9, 0.25, 0.7], 0.0, enamel(ENAMEL_RED))),
            [px, pad_top + 0.3 + 1.6, ci[1]],
            id_quat(),
        ));
        for g in sign_board(
            [px, pad_top + 1.45, ci[1] - 0.33],
            [0.52, 0.42],
            (1, 2),
            SIGN_AMBER,
            2.0,
            -1.0,
        ) {
            prims.push(g);
        }
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, 0.5, 0.12],
                0.0,
                chrome(CHROME_BRIGHT),
            )),
            [px + 0.46, pad_top + 1.0, ci[1] - 0.2],
            id_quat(),
        ));
    }

    // --- Pylon price sign at the −Z front corner — the beacon. ---
    let pylon = [8.0_f32, -6.0];
    prims.push(prim(
        solid(cuboid_tapered([0.5, 7.0, 0.5], 0.0, steel(STEEL_GREY))),
        [pylon[0], pad_top + 3.5, pylon[1]],
        id_quat(),
    ));
    // Cream backing board.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 3.6, 0.3], 0.0, enamel(ENAMEL_CREAM))),
        [pylon[0], pad_top + 6.0, pylon[1]],
        id_quat(),
    ));
    // Lit red brand strip on top, facing −Z.
    for g in sign_board(
        [pylon[0], pad_top + 7.0, pylon[1] - 0.2],
        [2.6, 0.9],
        (2, 1),
        NEON_RED,
        2.4,
        -1.0,
    ) {
        prims.push(g);
    }
    // Segmented amber price block below, facing −Z; the buzzing neon hum.
    let mut price = sign_board(
        [pylon[0], pad_top + 5.4, pylon[1] - 0.2],
        [2.6, 1.8],
        (3, 2),
        SIGN_AMBER,
        2.2,
        -1.0,
    );
    price[1].audio = fx::neon_buzz();
    prims.extend(price);

    let mut root = assemble(prims);
    // Signature life: a distant highway drone, dust off the lot.
    root.audio = fx::highway_drone();
    root.children
        .push(fx::road_dust([2.0, pad_top + 0.3, -7.0], 0x0D05_7A1E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GasStation.build(""), "gas_station");
    }

    #[test]
    fn has_lit_canopy_and_sign() {
        assert!(crate::catalogue::items::util::has_emissive(
            &GasStation.build("")
        ));
    }
}
