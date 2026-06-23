//! Fuel pump — a Roadside prop. A single enamel dispenser with a lit price
//! face, a chrome nozzle in its holster and a low concrete base. Scatter
//! clutter for the forecourt and the lot.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CHROME_BRIGHT, CONCRETE_GREY, ENAMEL_CREAM, ENAMEL_RED, SIGN_AMBER, chrome, concrete, enamel,
    sign_board,
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
    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [0.95, 0.16, 0.75],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.08, 0.0],
            id_quat(),
        ),
        // Enamel body.
        prim(
            solid(cuboid_tapered([0.72, 1.5, 0.52], 0.0, enamel(ENAMEL_CREAM))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
        // Red topper cap.
        prim(
            solid(cuboid_tapered([0.82, 0.22, 0.62], 0.0, enamel(ENAMEL_RED))),
            [0.0, 1.72, 0.0],
            id_quat(),
        ),
    ];

    // Segmented amber price/display face on the −Z front (two stacked digit
    // cells split by a dark gap — a flat lit slab washes white).
    for g in sign_board(
        [0.0, 1.28, -0.28],
        [0.52, 0.5],
        (1, 2),
        SIGN_AMBER,
        2.0,
        -1.0,
    ) {
        prims.push(g);
    }
    // Chrome keypad / card-reader panel below the display.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.42, 0.32, 0.06],
            0.0,
            chrome(CHROME_BRIGHT),
        )),
        [0.0, 0.72, -0.28],
        id_quat(),
    ));

    // Chrome nozzle holstered on the +X side, with a black rubber hose.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.42, 0.1], 0.0, chrome(CHROME_BRIGHT))),
        [0.43, 1.05, 0.1],
        id_quat(),
    ));
    // Hose loop coiled off the holster.
    prims.push(prim(
        solid(torus(0.04, 0.16, enamel([0.08, 0.08, 0.09]))),
        [0.46, 0.7, 0.1],
        id_quat(),
    ));
    // Hose drop down the flank.
    prims.push(prim(
        solid(cylinder_tapered(
            0.04,
            0.5,
            8,
            0.0,
            enamel([0.08, 0.08, 0.09]),
        )),
        [0.46, 0.5, 0.26],
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
        assert_sanitize_stable(&FuelPump.build(""), "fuel_pump");
    }
}
