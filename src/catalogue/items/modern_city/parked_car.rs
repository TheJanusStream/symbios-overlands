//! Parked car — a Modern-City prop. A generic sedan: a glossy enamel body
//! with a glazed cabin and dark wheels, left at the kerb.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
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

    // Four wheels (dark blocks, read as tyres from the side).
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.35, 0.7, 0.7], 0.0, enamel(TIRE_BLACK))),
            [sx * 1.4, 0.35, sz * 0.95],
            id_quat(),
        ));
    }

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
