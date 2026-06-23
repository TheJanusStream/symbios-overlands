//! Goalpost — a Sports/Recreation prop. A white goal frame with a chain-link
//! net slung behind it. Scatter clutter across the training pitches.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHAIN_GREY, LINE_WHITE, chainlink, enamel};

pub struct Goalpost;

impl CatalogueEntry for Goalpost {
    fn slug(&self) -> &'static str {
        "goalpost"
    }
    fn name(&self) -> &'static str {
        "Goalpost"
    }
    fn description(&self) -> &'static str {
        "White goal frame with a chain-link net slung behind."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let h = 2.5_f32;
    let half_w = 3.5_f32;

    let mut prims = vec![
        // Net floor — the flat root. (The round crossbar is a rotated child:
        // a rotated piece must never be the assemble root, or its rotation
        // spins the whole goal sideways.)
        prim(
            cuboid_tapered([half_w * 2.0, 0.05, 0.7], 0.0, chainlink(CHAIN_GREY)),
            [0.0, 0.06, 0.4],
            id_quat(),
        ),
    ];

    // Crossbar — a round white tube along X, above the mouth.
    prims.push(prim(
        solid(cylinder_tapered(
            0.09,
            half_w * 2.0 + 0.18,
            8,
            0.0,
            enamel(LINE_WHITE),
        )),
        [0.0, h, 0.0],
        quat_z(FRAC_PI_2),
    ));

    // Two round uprights on the goal line (the goal mouth opens to −Z).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.09, h, 8, 0.0, enamel(LINE_WHITE))),
            [sx * half_w, h * 0.5, 0.0],
            id_quat(),
        ));
        // Round back stay sloping down to the ground behind each upright.
        prims.push(prim(
            solid(cylinder_tapered(0.06, 1.7, 8, 0.0, enamel(LINE_WHITE))),
            [sx * half_w, h * 0.55, 0.55],
            quat_x(0.62),
        ));
    }

    // Chain-link net: a wide, near-vertical panel just behind the mouth that
    // leans back at the top. Kept shallow so the whole wide goal shape reads
    // (a deep net box foreshortens to a dark slab at the render's angles).
    prims.push(prim(
        cuboid_tapered([half_w * 2.0, h - 0.1, 0.05], 0.0, chainlink(CHAIN_GREY)),
        [0.0, (h - 0.1) * 0.5, 0.5],
        quat_x(0.2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Goalpost.build(""), "goalpost");
    }
}
