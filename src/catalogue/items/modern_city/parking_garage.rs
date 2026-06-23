//! Parking garage — a Modern-City secondary. An open concrete deck
//! structure: stacked slabs on a column grid with spandrel rails and a
//! stair/elevator core, a few cars parked on the ground level. The blunt
//! infrastructure between the glass towers.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_y, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CAR_BODY, CAR_GLASS, CONCRETE_GREY, STEEL_GREY, TIRE_BLACK, concrete, enamel, glass, steel,
};

/// A small parked car as a self-contained subtree — an enamel body, a glazed
/// cabin, and four round wheels — yawed as a whole by its root.
fn parked_car(cx: f32, cz: f32, yaw: f32, body: [f32; 3]) -> Generator {
    let mut root = prim(
        solid(cuboid_tapered([3.6, 0.7, 1.7], 0.08, enamel(body))),
        [cx, 0.95, cz],
        quat_y(yaw),
    );
    // Cabin + glazed greenhouse (local frame, set back).
    root.children.push(prim(
        solid(cuboid_tapered([2.0, 0.6, 1.5], 0.2, enamel(body))),
        [-0.2, 0.55, 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid_tapered([1.8, 0.5, 1.52], 0.22, glass(CAR_GLASS, 0.0)),
        [-0.2, 0.58, 0.0],
        id_quat(),
    ));
    // Four round wheels, axles laid across the car.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        root.children.push(prim(
            solid(cylinder_tapered(0.34, 0.3, 12, 0.0, enamel(TIRE_BLACK))),
            [sx * 1.2, -0.6, sz * 0.85],
            quat_x(FRAC_PI_2),
        ));
    }
    // Warm headlights (front, +X) and red tail lights (rear, −X).
    for sz in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.12, 0.18, 0.22], 0.0, glow([1.0, 0.92, 0.7], 1.6)),
            [1.78, 0.0, sz * 0.6],
            id_quat(),
        ));
        root.children.push(prim(
            cuboid_tapered([0.1, 0.16, 0.26], 0.0, glow([1.0, 0.12, 0.08], 1.4)),
            [-1.78, 0.05, sz * 0.6],
            id_quat(),
        ));
    }
    root
}

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

    // A few cars parked on the ground level (yaw aligns them to the bays).
    prims.push(parked_car(-3.0, 3.2, FRAC_PI_2, CAR_BODY));
    prims.push(parked_car(2.0, 3.2, FRAC_PI_2 + 0.08, [0.2, 0.32, 0.5]));
    prims.push(parked_car(5.5, -2.0, 0.0, [0.55, 0.56, 0.58]));

    // A sloped vehicle ramp from the ground up to the first deck.
    prims.push(prim(
        solid(cuboid_tapered(
            [5.0, 0.3, 4.0],
            0.0,
            concrete([0.52, 0.52, 0.54]),
        )),
        [w * 0.5 - 3.0, 0.5 + deck_gap * 0.5, d * 0.5 - 2.0],
        quat_z(0.32),
    ));

    // Entrance signage on the −Z front: a clearance bar and a lit P sign.
    let front_z = -d * 0.5;
    // Yellow height-clearance bar across the entry bay.
    prims.push(prim(
        solid(cuboid_tapered(
            [7.0, 0.3, 0.3],
            0.0,
            enamel([0.9, 0.78, 0.1]),
        )),
        [0.0, 2.4, front_z - 0.2],
        id_quat(),
    ));
    // Lit blue parking sign on a post by the entrance.
    prims.push(prim(
        solid(cuboid_tapered([0.18, 3.0, 0.18], 0.0, steel(STEEL_GREY))),
        [-w * 0.5 + 1.0, 1.5, front_z - 0.5],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.1, 1.1, 0.12], 0.0, glow([0.12, 0.32, 0.78], 2.0)),
        [-w * 0.5 + 1.0, 3.2, front_z - 0.5],
        id_quat(),
    ));
    // White "P" bar inset on the sign.
    prims.push(prim(
        cuboid_tapered([0.22, 0.7, 0.06], 0.0, glow([0.95, 0.96, 1.0], 1.8)),
        [-w * 0.5 + 0.85, 3.2, front_z - 0.58],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.4, 0.22, 0.06], 0.0, glow([0.95, 0.96, 1.0], 1.8)),
        [-w * 0.5 + 0.95, 3.35, front_z - 0.58],
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
        assert_sanitize_stable(&ParkingGarage.build(""), "parking_garage");
    }
}
