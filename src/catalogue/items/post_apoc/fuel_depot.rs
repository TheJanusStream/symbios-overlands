//! Fuel depot — a Post-apocalyptic secondary. A pair of salvaged fuel tanks on
//! saddles behind a scrap fence, a hand pump and a worklight on a pole. The
//! lifeblood store of the holdout; its light is emissive trim the ruin pass
//! can darken.
//!
//! The tanks are cylinders laid on their sides with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_RUST, RUST_BROWN, STEEL_GREY, WORKLIGHT, concrete, fx, rusted, sheet,
};

pub struct FuelDepot;

impl CatalogueEntry for FuelDepot {
    fn slug(&self) -> &'static str {
        "fuel_depot"
    }
    fn name(&self) -> &'static str {
        "Fuel Depot"
    }
    fn description(&self) -> &'static str {
        "Salvaged fuel tanks on saddles behind a scrap fence, a pump and a worklight."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [7.0, 0.3, 5.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Two fuel tanks on saddles, laid along Z.
    for tx in [-1.6_f32, 1.6] {
        prims.push(prim(
            solid(cylinder_tapered(0.9, 3.4, 14, 0.0, rusted(RUST_BROWN))),
            [tx, 1.3, -0.4],
            quat_x(FRAC_PI_2),
        ));
        for tz in [-1.2_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([1.4, 0.5, 0.5], 0.0, rusted(STEEL_GREY))),
                [tx, 0.4, -0.4 + tz],
                id_quat(),
            ));
        }
    }

    // Scrap fence along the front.
    prims.push(prim(
        solid(cuboid_tapered(
            [7.0, 1.8, 0.15],
            0.0,
            sheet(CORRUGATED_RUST),
        )),
        [0.0, 1.05, 2.4],
        id_quat(),
    ));

    // Worklight on a pole — emissive.
    prims.push(prim(
        solid(cylinder_tapered(0.08, 3.2, 6, 0.0, rusted(STEEL_GREY))),
        [3.2, 1.7, -1.6],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.3, 0.4], 0.0, glow(WORKLIGHT, 3.0)),
        [3.2, 3.3, -1.3],
        quat_x(0.4),
    ));

    let mut root = assemble(prims);
    // Signature life: desolate wind over the lot.
    root.audio = fx::desolate_wind();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FuelDepot.build(""), "fuel_depot");
    }
}
