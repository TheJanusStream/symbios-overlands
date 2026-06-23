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

use crate::catalogue::items::modern_city::curtain_wall;
use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, GLASS_AQUA, POOL_AQUA, SIGN_AMBER, SIGN_GOLD, STEEL_GREY,
    STUCCO_SAND, STUCCO_WHITE, canvas, concrete, fx, glass, steel, stucco, water,
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
    // The render front tile looks down the -Z axis, so the glazed seafront
    // facade (lobby, balconies, awning, sign) and the pool terrace all face -Z.
    let front = -half_d;

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

    // Parapet capping the body, with a slim cornice band proud below it.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.4, 0.7, d + 0.4],
            0.0,
            stucco(STUCCO_WHITE),
        )),
        [0.0, h + 0.35, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.6, 0.2, d + 0.6],
            0.0,
            stucco(STUCCO_SAND),
        )),
        [0.0, h - 0.1, 0.0],
        id_quat(),
    ));

    // Glazed lobby curtain wall on the seafront (-Z): a lit aqua glass grid
    // behind proud steel mullions, with a warm interior glow set back inside.
    prims.extend(curtain_wall(
        [0.0, 1.8, front],
        [9.0, 3.0],
        (5, 2),
        -0.28,
        glass(GLASS_AQUA, 1.8),
        steel(STEEL_GREY),
    ));
    prims.push(prim(
        cuboid_tapered([7.4, 2.4, 0.2], 0.0, glow(SIGN_GOLD, 1.4)),
        [0.0, 1.7, front + 0.45],
        id_quat(),
    ));

    // Tiered seafront balconies (two upper floors): slab + railing + lit glass
    // doors gridded by the Window texture, all proud of the -Z facade.
    for fy in [3.6_f32, 5.6] {
        prims.push(prim(
            solid(cuboid_tapered(
                [w - 1.0, 0.25, 0.9],
                0.0,
                stucco(STUCCO_WHITE),
            )),
            [0.0, fy, front - 0.45],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([w - 1.0, 0.55, 0.08], 0.0, steel(STEEL_GREY)),
            [0.0, fy + 0.4, front - 0.85],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([w - 1.5, 1.8, 0.15], 0.0, glass(GLASS_AQUA, 1.5)),
            [0.0, fy + 0.2, front - 0.05],
            id_quat(),
        ));
    }

    // Lit side windows on the gable faces.
    for sx in [-1.0_f32, 1.0] {
        for fy in [3.6_f32, 5.6] {
            prims.push(prim(
                cuboid_tapered([0.15, 1.6, d - 3.0], 0.0, glass(GLASS_AQUA, 1.2)),
                [sx * (w * 0.5 + 0.05), fy, 0.0],
                id_quat(),
            ));
        }
    }

    // Striped entrance awning over the lobby door, on two steel poles, slung
    // out over the -Z front and tilted so its leading edge drops toward shore.
    prims.push(prim(
        cuboid_tapered([6.0, 0.2, 2.4], 0.0, canvas(AWNING_RED, AWNING_WHITE)),
        [0.0, 3.0, front - 1.3],
        quat_x(-0.28),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 2.9, 0.12], 0.0, steel(STEEL_GREY))),
            [sx * 2.6, 1.45, front - 2.3],
            id_quat(),
        ));
    }

    // Rooftop sign: two steel posts on the parapet carrying a deep-saturated
    // amber bar that reads across the bay without blooming to a white blank.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 1.6, 0.2], 0.0, steel(STEEL_GREY))),
            [sx * 2.6, h + 1.5, front + 0.6],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([6.4, 1.1, 0.3], 0.0, glow(SIGN_AMBER, 2.4)),
        [0.0, h + 1.9, front + 0.6],
        id_quat(),
    ));

    // Resort pool terrace on the seafront: a pale stone deck, a glassy
    // turquoise pool ringed by a proud coping, flanked by two parasols.
    let pool_z = front - 6.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [11.0, 0.2, 6.0],
            0.0,
            concrete([0.86, 0.83, 0.76]),
        )),
        [0.0, 0.1, pool_z],
        id_quat(),
    ));
    // Sunk basin shell (darker) under the water so the pool reads as depth.
    prims.push(prim(
        solid(cuboid_tapered(
            [5.2, 0.3, 3.4],
            0.0,
            concrete([0.40, 0.58, 0.62]),
        )),
        [0.0, 0.06, pool_z],
        id_quat(),
    ));
    // Water surface, set just below the coping.
    prims.push(prim(
        cuboid_tapered([4.8, 0.12, 3.0], 0.0, water(POOL_AQUA)),
        [0.0, 0.2, pool_z],
        id_quat(),
    ));
    // Proud coping rim framing the water (raised so nothing is flush).
    for (size, pos) in [
        ([5.4_f32, 0.18, 0.32], [0.0_f32, 0.3, pool_z - 1.7]),
        ([5.4, 0.18, 0.32], [0.0, 0.3, pool_z + 1.7]),
        ([0.32, 0.18, 3.4], [-2.7, 0.3, pool_z]),
        ([0.32, 0.18, 3.4], [2.7, 0.3, pool_z]),
    ] {
        prims.push(prim(
            solid(cuboid_tapered(size, 0.0, stucco(STUCCO_WHITE))),
            pos,
            id_quat(),
        ));
    }
    // Two poolside parasols.
    for sx in [-1.0_f32, 1.0] {
        let px = sx * 4.4;
        prims.push(prim(
            solid(cylinder_tapered(0.05, 2.2, 8, 0.0, steel(STEEL_GREY))),
            [px, 1.1, pool_z + 1.2],
            id_quat(),
        ));
        prims.push(prim(
            cone(1.1, 0.55, 14, canvas(AWNING_RED, AWNING_WHITE)),
            [px, 2.2, pool_z + 1.2],
            id_quat(),
        ));
    }

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
        assert!(crate::catalogue::items::util::has_emissive(
            &GrandHotel.build("")
        ));
    }
}
