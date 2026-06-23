//! Flagpole — a Civic/Campus prop. A tall steel pole on a concrete base
//! flying a flag, with a gilt truck ball at the top. Scatter clutter for the
//! quad and the forecourts.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, FLAG_RED, STEEL_GREY, concrete, painted, steel};

pub struct Flagpole;

impl CatalogueEntry for Flagpole {
    fn slug(&self) -> &'static str {
        "flagpole"
    }
    fn name(&self) -> &'static str {
        "Flagpole"
    }
    fn description(&self) -> &'static str {
        "Tall steel pole on a concrete base flying a flag, with a gilt truck ball."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
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
                [0.6, 0.3, 0.6],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];
    // Cast foot collar where the pole meets the base.
    prims.push(prim(
        solid(torus(0.05, 0.18, steel(STEEL_GREY))),
        [0.0, 0.32, 0.0],
        id_quat(),
    ));
    // Steel pole.
    prims.push(prim(
        solid(cylinder_tapered(0.08, 6.0, 8, 0.1, steel(STEEL_GREY))),
        [0.0, 3.3, 0.0],
        id_quat(),
    ));
    // Gilt truck ball.
    prims.push(prim(
        solid(sphere(0.14, 3, painted([0.80, 0.66, 0.24]))),
        [0.0, 6.4, 0.0],
        id_quat(),
    ));
    // Halyard rope running the pole, with a tie-off cleat near the foot.
    prims.push(prim(
        solid(cylinder_tapered(
            0.012,
            5.7,
            6,
            0.0,
            painted([0.85, 0.82, 0.72]),
        )),
        [0.11, 3.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.04, 0.04, 0.22], 0.0, steel(STEEL_GREY))),
        [0.13, 1.1, 0.0],
        id_quat(),
    ));
    // A flag flying from the upper pole — five panels rippling on the breeze,
    // each cranked a touch in yaw and stepped in Z so the cloth waves.
    for i in 0..5 {
        let zo = if i % 2 == 0 { 0.07 } else { -0.07 };
        let yaw = if i % 2 == 0 { 0.18 } else { -0.18 };
        prims.push(prim(
            cuboid_tapered([0.42, 1.0, 0.03], 0.0, painted(FLAG_RED)),
            [0.27 + i as f32 * 0.38, 5.5, zo],
            quat_y(yaw),
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
        assert_sanitize_stable(&Flagpole.build(""), "flagpole");
    }
}
