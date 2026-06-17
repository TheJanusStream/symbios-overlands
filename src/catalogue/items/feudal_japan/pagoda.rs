//! Pagoda — the Feudal-Japan landmark. A five-bay tiered tower: lacquered
//! columns and white plaster bodies under wide flared tile roofs that
//! shrink as they climb, crowned by a golden sōrin finial of stacked rings.
//! Blossom drifts from its eaves and a deep temple bell hums at its base.
//! ~20 m tall, so it anchors the settlement and reads as a temple spire
//! across the home region.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    GOLD, LACQUER_RED, PLASTER_WHITE, STONE_GREY, TILE_SLATE, bronze, fx, lacquer, plaster,
    roof_tile, stone,
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

    // Stacked tiers: (body width, body height, roof flare beyond body).
    let tiers = [
        (6.0_f32, 3.8_f32, 2.6_f32),
        (4.8, 3.4, 2.2),
        (3.6, 3.0, 1.9),
    ];
    let roof_h = 1.1;
    let mut y = plinth_h;
    for (w, h, flare) in tiers {
        // Plaster body.
        prims.push(prim(
            solid(cuboid_tapered([w, h, w], 0.0, plaster(PLASTER_WHITE))),
            [0.0, y + h * 0.5, 0.0],
            id_quat(),
        ));
        // Lacquered corner columns.
        for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            prims.push(prim(
                solid(cuboid_tapered([0.4, h, 0.4], 0.0, lacquer(LACQUER_RED))),
                [sx * (w * 0.5 - 0.2), y + h * 0.5, sz * (w * 0.5 - 0.2)],
                id_quat(),
            ));
        }
        // Deep-eave shadow rim, then the flared tile roof above it.
        prims.push(prim(
            solid(cuboid_tapered(
                [w + flare + 0.5, 0.2, w + flare + 0.5],
                0.0,
                roof_tile(TILE_SLATE),
            )),
            [0.0, y + h + 0.1, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [w + flare, roof_h, w + flare],
                0.55,
                roof_tile(TILE_SLATE),
            )),
            [0.0, y + h + 0.2 + roof_h * 0.5, 0.0],
            id_quat(),
        ));
        y += h + roof_h + 0.2;
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
        assert!(super::super::has_emissive(&Pagoda.build("")));
    }
}
