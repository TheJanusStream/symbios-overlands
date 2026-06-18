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

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, foundation_block, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ASPHALT_DARK, BRICK_TAN, CANOPY_LIT, CHROME_BRIGHT, CONCRETE_GREY, CORRUGATED_GREY,
    ENAMEL_CREAM, ENAMEL_RED, GLASS_TINT, PRICE_AMBER, STEEL_GREY, asphalt, brick, chrome,
    concrete, corrugated, enamel, fx, glass, steel,
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
    prims.push(foundation_block(8.4, 6.4, [-5.0, -4.0], 1.5));

    // Brick convenience store at the back-left, with a lit storefront.
    prims.push(prim(
        solid(cuboid_tapered([8.0, 3.2, 6.0], 0.0, brick(BRICK_TAN))),
        [-5.0, pad_top + 1.6, -4.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [8.4, 0.4, 6.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [-5.0, pad_top + 3.4, -4.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([6.0, 2.0, 0.15], 0.0, glass(GLASS_TINT, 1.4)),
        [-5.0, pad_top + 1.2, -1.05],
        id_quat(),
    ));

    // Pump-island canopy: four steel columns, a corrugated deck, a chrome
    // fascia and a lit underside.
    let ci = [4.0_f32, 2.0]; // canopy centre (X, Z)
    let col_y = pad_top + 2.5;
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.4, 5.0, 0.4], 0.0, steel(STEEL_GREY))),
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
        solid(cuboid_tapered(
            [10.3, 0.45, 8.3],
            0.0,
            chrome(CHROME_BRIGHT),
        )),
        [ci[0], pad_top + 4.85, ci[1]],
        id_quat(),
    ));
    // Red brand stripe on the fascia.
    prims.push(prim(
        cuboid_tapered([10.4, 0.18, 8.4], 0.0, enamel(ENAMEL_RED)),
        [ci[0], pad_top + 4.95, ci[1]],
        id_quat(),
    ));
    // Lit canopy underside — floods the pumps. Emissive.
    prims.push(prim(
        cuboid_tapered([9.6, 0.12, 7.6], 0.0, glow(CANOPY_LIT, 2.5)),
        [ci[0], pad_top + 4.6, ci[1]],
        id_quat(),
    ));

    // Pump island: a concrete curb with two enamel pumps and lit faces.
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
            solid(cuboid_tapered([0.8, 1.4, 0.6], 0.0, enamel(ENAMEL_CREAM))),
            [px, pad_top + 0.3 + 0.7, ci[1]],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.5, 0.4, 0.62], 0.0, glow(PRICE_AMBER, 2.0)),
            [px, pad_top + 0.3 + 1.0, ci[1]],
            id_quat(),
        ));
    }

    // Pylon price sign at the front corner — the beacon. Emissive.
    let pylon = [8.5_f32, 6.5];
    prims.push(prim(
        solid(cuboid_tapered([0.4, 7.0, 0.4], 0.0, steel(STEEL_GREY))),
        [pylon[0], pad_top + 3.5, pylon[1]],
        id_quat(),
    ));
    let mut sign = prim(
        cuboid_tapered([2.6, 2.0, 0.4], 0.0, glow(PRICE_AMBER, 4.0)),
        [pylon[0], pad_top + 7.0, pylon[1]],
        id_quat(),
    );
    sign.audio = fx::neon_buzz();
    prims.push(sign);

    let mut root = assemble(prims);
    // Signature life: a distant highway drone, dust off the lot.
    root.audio = fx::highway_drone();
    root.children
        .push(fx::road_dust([2.0, pad_top + 0.3, 7.0], 0x0D05_7A1E));
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
        assert!(super::super::has_emissive(&GasStation.build("")));
    }
}
