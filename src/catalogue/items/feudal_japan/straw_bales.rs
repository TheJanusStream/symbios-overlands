//! Straw bales — a Feudal-Japan *poor* prop. A stack of bound rice-straw
//! bales (tawara) drying after the harvest, with a couple of cylindrical
//! rolls tossed alongside. The everyday clutter of the farmstead.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{THATCH_STRAW, thatch};

pub struct StrawBales;

impl CatalogueEntry for StrawBales {
    fn slug(&self) -> &'static str {
        "straw_bales"
    }
    fn name(&self) -> &'static str {
        "Straw Bales"
    }
    fn description(&self) -> &'static str {
        "Stack of bound rice-straw bales with cylindrical rolls alongside."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let straw = || thatch(THATCH_STRAW);

    // Bottom-row bale — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([0.9, 0.5, 0.6], 0.05, straw())),
        [-0.5, 0.25, 0.0],
        id_quat(),
    )];
    // Rest of the bottom row.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 0.5, 0.6], 0.05, straw())),
        [0.5, 0.25, 0.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.9, 0.5, 0.6], 0.05, straw())),
        [0.0, 0.25, 0.7],
        id_quat(),
    ));
    // Top bale, set back.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 0.5, 0.6], 0.05, straw())),
        [0.0, 0.75, 0.2],
        id_quat(),
    ));

    // A couple of cylindrical rolls tipped on their sides nearby.
    prims.push(prim(
        solid(cylinder_tapered(0.3, 0.9, 10, 0.0, straw())),
        [1.2, 0.3, -0.7],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.28, 0.85, 10, 0.0, straw())),
        [-1.3, 0.28, 0.6],
        quat_x(FRAC_PI_2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&StrawBales.build(""), "straw_bales");
    }
}
