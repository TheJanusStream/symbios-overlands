//! Pagoda — the Feudal-Japan landmark. A five-bay tiered tower: lacquered
//! columns and white plaster bodies under wide flared tile roofs that
//! shrink as they climb, crowned by a golden sōrin finial of stacked rings.
//! Blossom drifts from its eaves and a deep temple bell hums at its base.
//! ~20 m tall, so it anchors the settlement and reads as a temple spire
//! across the home region.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_y, solid, sphere, torus,
    wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    GOLD, LACQUER_RED, PLASTER_WHITE, STONE_GREY, TILE_SLATE, TIMBER_DARK, bronze, fx, lacquer,
    plaster, roof_tile, stone, timber,
};

pub struct Pagoda;

impl CatalogueEntry for Pagoda {
    fn slug(&self) -> &'static str {
        "pagoda"
    }
    fn name(&self) -> &'static str {
        "Pagoda"
    }
    fn description(&self) -> &'static str {
        "Tiered temple tower under flared tile roofs, crowned by a golden finial."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 14.0,
            min_spawn_dist: 55.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plinth_h = 0.7;

    let mut prims = vec![
        // Stone plinth — the root.
        prim(
            solid(cuboid_tapered([9.0, plinth_h, 9.0], 0.0, stone(STONE_GREY))),
            [0.0, plinth_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    let corners = [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

    // Stacked tiers: (body width, body height, roof flare beyond body).
    let tiers = [
        (6.0_f32, 3.6_f32, 2.8_f32),
        (4.8, 3.2, 2.4),
        (3.6, 2.8, 2.0),
    ];
    let mut y = plinth_h;
    for (w, h, flare) in tiers {
        let body_top = y + h;
        // Plaster body.
        prims.push(prim(
            solid(cuboid_tapered([w, h, w], 0.0, plaster(PLASTER_WHITE))),
            [0.0, y + h * 0.5, 0.0],
            id_quat(),
        ));
        // Lacquered corner columns.
        for (sx, sz) in corners {
            prims.push(prim(
                solid(cuboid_tapered([0.4, h, 0.4], 0.0, lacquer(LACQUER_RED))),
                [sx * (w * 0.5 - 0.2), y + h * 0.5, sz * (w * 0.5 - 0.2)],
                id_quat(),
            ));
        }

        // Timber bracket course (tokyō) stepping out under the eaves.
        prims.push(prim(
            solid(cuboid_tapered(
                [w + 0.9, 0.45, w + 0.9],
                0.0,
                timber(TIMBER_DARK),
            )),
            [0.0, body_top + 0.22, 0.0],
            id_quat(),
        ));
        // Deep-eave shadow board — a thin slab at the full flare.
        let eave_w = w + flare;
        prims.push(prim(
            solid(cuboid_tapered(
                [eave_w + 0.4, 0.18, eave_w + 0.4],
                0.0,
                roof_tile(TILE_SLATE),
            )),
            [0.0, body_top + 0.52, 0.0],
            id_quat(),
        ));
        // Flared tile roof cap rising to the ridge.
        let cap_h = 1.0;
        prims.push(prim(
            solid(cuboid_tapered(
                [eave_w, cap_h, eave_w],
                0.62,
                roof_tile(TILE_SLATE),
            )),
            [0.0, body_top + 0.6 + cap_h * 0.5, 0.0],
            id_quat(),
        ));
        // Four upturned flying-eave corners — the swept-roof signature. Each
        // wedge's high tip points out along its corner diagonal (quat_y).
        let eave_half = (eave_w + 0.4) * 0.5;
        for (sx, sz) in corners {
            let theta = (-sx).atan2(-sz);
            prims.push(prim(
                wedge([flare * 0.85, 0.7, flare * 0.85], roof_tile(TILE_SLATE)),
                [
                    sx * (eave_half - flare * 0.25),
                    body_top + 0.55,
                    sz * (eave_half - flare * 0.25),
                ],
                quat_y(theta),
            ));
        }
        y = body_top + 0.6 + cap_h;
    }

    // Golden sōrin finial: a tapered spire threaded through stacked rings,
    // capped with a sacred jewel. The kit's emissive trim.
    prims.push(prim(
        solid(cylinder_tapered(0.14, 3.0, 8, 0.6, glow(GOLD, 2.5))),
        [0.0, y + 1.5, 0.0],
        id_quat(),
    ));
    for k in 0..4 {
        prims.push(prim(
            torus(0.08, 0.5 - k as f32 * 0.08, glow(GOLD, 3.0)),
            [0.0, y + 0.5 + k as f32 * 0.55, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        sphere(0.32, 3, glow(GOLD, 4.0)),
        [0.0, y + 3.1, 0.0],
        id_quat(),
    ));

    // Bronze bell hung in the open lowest bay, the source of the deep ring.
    let bell_y = plinth_h + 2.0;
    prims.push(prim(
        solid(cuboid_tapered([0.18, 0.5, 0.18], 0.0, bronze(GOLD))),
        [3.4, bell_y + 0.7, 0.0],
        id_quat(),
    ));
    let mut bell = prim(
        solid(cylinder_tapered(0.45, 1.0, 12, 0.25, bronze(GOLD))),
        [3.4, bell_y, 0.0],
        id_quat(),
    );
    bell.audio = fx::temple_bell_ring();
    prims.push(bell);

    let eave_y = plinth_h + 4.0;
    let mut root = assemble(prims);
    // Signature life: blossom shed from the lowest eaves above the bell.
    root.children
        .push(fx::falling_petals([0.0, eave_y, 0.0], 0x9A60_DA11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Pagoda.build(""), "pagoda");
    }

    #[test]
    fn has_gold_finial() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Pagoda.build("")
        ));
    }
}
