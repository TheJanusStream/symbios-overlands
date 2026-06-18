//! Fuel pump — a Roadside prop. A single enamel dispenser with a lit price
//! face, a chrome nozzle in its holster and a low concrete base. Scatter
//! clutter for the forecourt and the lot.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CHROME_BRIGHT, CONCRETE_GREY, ENAMEL_CREAM, ENAMEL_RED, PRICE_AMBER, chrome, concrete, enamel,
};

pub struct FuelPump;

impl CatalogueEntry for FuelPump {
    fn slug(&self) -> &'static str {
        "fuel_pump"
    }
    fn name(&self) -> &'static str {
        "Fuel Pump"
    }
    fn description(&self) -> &'static str {
        "Enamel fuel dispenser with a lit price face and a chrome nozzle."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.9,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [0.9, 0.15, 0.7],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.075, 0.0],
            id_quat(),
        ),
        // Enamel body.
        prim(
            solid(cuboid_tapered([0.7, 1.5, 0.5], 0.0, enamel(ENAMEL_CREAM))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
        // Lit price face.
        prim(
            cuboid_tapered([0.5, 0.4, 0.52], 0.0, glow(PRICE_AMBER, 2.0)),
            [0.0, 1.25, 0.0],
            id_quat(),
        ),
        // Coloured topper cap.
        prim(
            solid(cuboid_tapered([0.8, 0.2, 0.6], 0.0, enamel(ENAMEL_RED))),
            [0.0, 1.75, 0.0],
            id_quat(),
        ),
        // Chrome nozzle holstered on the side.
        prim(
            solid(cuboid_tapered(
                [0.12, 0.3, 0.12],
                0.0,
                chrome(CHROME_BRIGHT),
            )),
            [0.45, 1.05, 0.18],
            id_quat(),
        ),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FuelPump.build(""), "fuel_pump");
    }
}
