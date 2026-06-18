//! Parking garage — a Modern-City secondary. An open concrete deck
//! structure: stacked slabs on a column grid with spandrel rails and a
//! stair/elevator core, a few cars parked on the ground level. The blunt
//! infrastructure between the glass towers.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_y, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CAR_BODY, CONCRETE_GREY, STEEL_GREY, concrete, enamel, steel};

pub struct ParkingGarage;

impl CatalogueEntry for ParkingGarage {
    fn slug(&self) -> &'static str {
        "parking_garage"
    }
    fn name(&self) -> &'static str {
        "Parking Garage"
    }
    fn description(&self) -> &'static str {
        "Open concrete deck structure on a column grid with parked cars."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 16.0_f32;
    let d = 12.0_f32;
    let decks = 4;
    let deck_gap = 3.0_f32;
    let total_h = decks as f32 * deck_gap;

    let mut prims = vec![
        // Ground slab — the root.
        prim(
            solid(cuboid_tapered(
                [w + 1.0, 0.5, d + 1.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
    ];

    // Column grid.
    for sx in [-1.0_f32, 0.0, 1.0] {
        for sz in [-1.0_f32, 0.0, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.6, total_h, 0.6],
                    0.0,
                    concrete(CONCRETE_GREY),
                )),
                [sx * w * 0.4, 0.5 + total_h * 0.5, sz * d * 0.4],
                id_quat(),
            ));
        }
    }

    // Stacked decks with spandrel rails.
    for k in 1..=decks {
        let y = 0.5 + k as f32 * deck_gap;
        prims.push(prim(
            solid(cuboid_tapered([w, 0.4, d], 0.0, concrete(CONCRETE_GREY))),
            [0.0, y, 0.0],
            id_quat(),
        ));
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([w, 0.8, 0.25], 0.0, steel(STEEL_GREY)),
                [0.0, y - deck_gap * 0.5, sz * d * 0.5],
                id_quat(),
            ));
        }
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([0.25, 0.8, d], 0.0, steel(STEEL_GREY)),
                [sx * w * 0.5, y - deck_gap * 0.5, 0.0],
                id_quat(),
            ));
        }
    }

    // Stair/elevator core at one corner.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, total_h + 1.0, 3.0],
            0.0,
            concrete([0.5, 0.5, 0.52]),
        )),
        [-w * 0.5 + 1.5, 0.5 + (total_h + 1.0) * 0.5, -d * 0.5 + 1.5],
        id_quat(),
    ));

    // A few cars parked on the ground level.
    let car = |w: f32, h: f32, l: f32| solid(cuboid_tapered([w, h, l], 0.1, enamel(CAR_BODY)));
    for (x, z, yaw) in [
        (-3.0_f32, 3.5_f32, 0.0_f32),
        (2.0, 3.5, 0.1),
        (5.5, -2.0, 1.57),
    ] {
        prims.push(prim(car(2.0, 1.4, 4.2), [x, 1.2, z], quat_y(yaw)));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ParkingGarage.build(""), "parking_garage");
    }
}
