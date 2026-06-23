//! Parked car — a Modern-City prop. A generic sedan: a glossy enamel body
//! with a glazed cabin and dark wheels, left at the kerb.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CAR_BODY, CAR_GLASS, TIRE_BLACK, enamel, glass};

pub struct ParkedCar;

impl CatalogueEntry for ParkedCar {
    fn slug(&self) -> &'static str {
        "parked_car"
    }
    fn name(&self) -> &'static str {
        "Parked Car"
    }
    fn description(&self) -> &'static str {
        "Generic enamel-bodied sedan with a glazed cabin, left at the kerb."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Lower body — the root.
        prim(
            solid(cuboid_tapered([4.2, 0.7, 1.9], 0.08, enamel(CAR_BODY))),
            [0.0, 0.6, 0.0],
            id_quat(),
        ),
        // Cabin, set back and narrower.
        prim(
            solid(cuboid_tapered([2.3, 0.7, 1.7], 0.18, enamel(CAR_BODY))),
            [-0.2, 1.25, 0.0],
            id_quat(),
        ),
        // Glazed greenhouse.
        prim(
            cuboid_tapered([2.1, 0.55, 1.72], 0.2, glass(CAR_GLASS, 0.0)),
            [-0.2, 1.28, 0.0],
            id_quat(),
        ),
    ];

    // Four round wheels, axles laid across the car.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.4, 0.3, 12, 0.0, enamel(TIRE_BLACK))),
            [sx * 1.4, 0.4, sz * 0.98],
            quat_x(FRAC_PI_2),
        ));
        // Pale hub cap.
        prims.push(prim(
            cylinder_tapered(0.16, 0.32, 10, 0.0, enamel([0.7, 0.71, 0.73])),
            [sx * 1.4, 0.4, sz * 0.99],
            quat_x(FRAC_PI_2),
        ));
    }

    // Front grille and warm headlights.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.08, 0.3, 1.0],
            0.0,
            enamel([0.12, 0.12, 0.13]),
        )),
        [2.12, 0.5, 0.0],
        id_quat(),
    ));
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.1, 0.2, 0.28], 0.0, glow([1.0, 0.92, 0.7], 1.6)),
            [2.13, 0.62, sz * 0.62],
            id_quat(),
        ));
        // Red tail lights.
        prims.push(prim(
            cuboid_tapered([0.09, 0.18, 0.3], 0.0, glow([1.0, 0.12, 0.08], 1.4)),
            [-2.13, 0.66, sz * 0.66],
            id_quat(),
        ));
        // Wing mirrors at the A-pillar.
        prims.push(prim(
            solid(cuboid_tapered([0.18, 0.12, 0.1], 0.0, enamel(CAR_BODY))),
            [0.95, 1.12, sz * 1.02],
            id_quat(),
        ));
    }
    // Number plate at the rear.
    prims.push(prim(
        cuboid_tapered([0.05, 0.18, 0.5], 0.0, enamel([0.85, 0.85, 0.8])),
        [-2.14, 0.4, 0.0],
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
        assert_sanitize_stable(&ParkedCar.build(""), "parked_car");
    }
}
