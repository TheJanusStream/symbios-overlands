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
    let cord = || thatch([0.40, 0.30, 0.16]);

    // Stacked bound bales — three on the bottom, one set back on top.
    let bales = [
        [-0.5_f32, 0.25, 0.0],
        [0.5, 0.25, 0.05],
        [0.0, 0.25, 0.7],
        [0.0, 0.75, 0.2],
    ];
    let mut prims = Vec::new();
    for [bx, by, bz] in bales {
        prims.push(prim(
            solid(cuboid_tapered([0.9, 0.5, 0.6], 0.05, straw())),
            [bx, by, bz],
            id_quat(),
        ));
        // Two rope binding bands around the girth.
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([0.04, 0.54, 0.64], 0.0, cord()),
                [bx + sx * 0.24, by, bz],
                id_quat(),
            ));
        }
    }

    // A couple of cylindrical rolls tipped on their sides nearby, each bound
    // with a rope band around the middle.
    for ([rx, ry, rz], rr, rl) in [
        ([1.2_f32, 0.3, -0.7], 0.3_f32, 0.9_f32),
        ([-1.3, 0.28, 0.6], 0.28, 0.85),
    ] {
        prims.push(prim(
            solid(cylinder_tapered(rr, rl, 10, 0.0, straw())),
            [rx, ry, rz],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            cylinder_tapered(rr + 0.02, 0.05, 10, 0.0, cord()),
            [rx, ry, rz],
            quat_x(FRAC_PI_2),
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
        assert_sanitize_stable(&StrawBales.build(""), "straw_bales");
    }
}
