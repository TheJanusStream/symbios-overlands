//! Grand hotel — the Coastal-Resort landmark and the kit's lit hero. A
//! whitewashed stucco block of three storeys with tiered balconies and a
//! glowing lobby behind a striped entrance awning, crowned by an emissive
//! rooftop sign that reads across the bay at dusk. ~13 m wide, so it anchors
//! the strip and stands in for the seafront hotel from across the home
//! region. Its sign and lit glass are the trim escalation's ruin pass snuffs
//! to a dark, shuttered facade.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the main block.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, GLASS_AQUA, SIGN_GOLD, STEEL_GREY, STUCCO_SAND, STUCCO_WHITE, canvas,
    fx, glass, steel, stucco,
};
use crate::catalogue::items::util::foundation_block;

pub struct GrandHotel;

impl CatalogueEntry for GrandHotel {
    fn slug(&self) -> &'static str {
        "grand_hotel"
    }
    fn name(&self) -> &'static str {
        "Grand Hotel"
    }
    fn description(&self) -> &'static str {
        "Whitewashed seafront hotel with tiered balconies, a lit lobby and a glowing rooftop sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 14.0,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 13.0_f32; // width (X)
    let d = 8.5_f32; // depth (Z)
    let h = 8.0_f32; // body height
    let half_d = d * 0.5;

    let mut prims = vec![
        // Main stucco block — the root, base at ground.
        prim(
            solid(cuboid_tapered([w, h, d], 0.0, stucco(STUCCO_WHITE))),
            [0.0, h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Buried plinth so a terrain-snapped placement shows footing on a slope.
    prims.push(foundation_block(w + 2.0, d + 2.0, [0.0, 0.0], 2.0));

    // Wider ground-floor podium reading as the lobby base.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 2.0, 1.8, d + 1.0],
            0.0,
            stucco(STUCCO_SAND),
        )),
        [0.0, 0.9, 0.0],
        id_quat(),
    ));

    // Corner pilasters proud of the body.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.7, h, 0.7], 0.0, stucco(STUCCO_SAND))),
                [sx * (w * 0.5 - 0.1), h * 0.5, sz * (half_d - 0.1)],
                id_quat(),
            ));
        }
    }

    // Parapet capping the body.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.4, 0.7, d + 0.4],
            0.0,
            stucco(STUCCO_WHITE),
        )),
        [0.0, h + 0.35, 0.0],
        id_quat(),
    ));

    // Tiered front balconies (two upper floors): slab + railing + lit glass.
    for fy in [3.2_f32, 5.4] {
        prims.push(prim(
            solid(cuboid_tapered(
                [w - 1.0, 0.25, 0.9],
                0.0,
                stucco(STUCCO_WHITE),
            )),
            [0.0, fy, half_d + 0.45],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([w - 1.0, 0.55, 0.08], 0.0, steel(STEEL_GREY)),
            [0.0, fy + 0.4, half_d + 0.85],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([w - 1.5, 1.8, 0.15], 0.0, glass(GLASS_AQUA, 1.4)),
            [0.0, fy + 0.2, half_d + 0.05],
            id_quat(),
        ));
    }

    // Lit side windows on the gable faces.
    for sx in [-1.0_f32, 1.0] {
        for fy in [3.2_f32, 5.4] {
            prims.push(prim(
                cuboid_tapered([0.15, 1.6, d - 3.0], 0.0, glass(GLASS_AQUA, 1.2)),
                [sx * (w * 0.5 + 0.05), fy, 0.0],
                id_quat(),
            ));
        }
    }

    // Glowing lobby: a tall lit glass front with a warm glow behind it.
    prims.push(prim(
        cuboid_tapered([9.0, 2.4, 0.2], 0.0, glass(GLASS_AQUA, 1.6)),
        [0.0, 1.5, half_d + 0.02],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([5.0, 1.6, 0.3], 0.0, glow(SIGN_GOLD, 2.0)),
        [0.0, 1.4, half_d - 0.2],
        id_quat(),
    ));

    // Striped entrance awning over the lobby door, on two steel poles.
    prims.push(prim(
        cuboid_tapered([6.0, 0.2, 2.4], 0.0, canvas(AWNING_RED, AWNING_WHITE)),
        [0.0, 3.0, half_d + 1.3],
        quat_x(0.28),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 2.9, 0.12], 0.0, steel(STEEL_GREY))),
            [sx * 2.6, 1.45, half_d + 2.3],
            id_quat(),
        ));
    }

    // Rooftop sign: two steel posts on the parapet carrying an emissive bar.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 1.6, 0.2], 0.0, steel(STEEL_GREY))),
            [sx * 2.6, h + 1.5, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([6.4, 1.1, 0.3], 0.0, glow(SIGN_GOLD, 5.0)),
        [0.0, h + 1.9, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a soft sea breeze breathing over the frontage.
    root.audio = fx::sea_breeze();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GrandHotel.build(""), "grand_hotel");
    }

    #[test]
    fn has_lit_sign() {
        assert!(super::super::has_emissive(&GrandHotel.build("")));
    }
}
