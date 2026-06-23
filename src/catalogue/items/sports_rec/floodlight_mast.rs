//! Floodlight mast — a Sports/Recreation prop. A tall steel tower carrying a
//! lit bank of lamps. Scatter clutter around the pitches; its lamp bank is
//! emissive trim the ruin pass can darken.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, STEEL_GREY, concrete, steel};

pub struct FloodlightMast;

impl CatalogueEntry for FloodlightMast {
    fn slug(&self) -> &'static str {
        "floodlight_mast"
    }
    fn name(&self) -> &'static str {
        "Floodlight Mast"
    }
    fn description(&self) -> &'static str {
        "Tall steel tower carrying a lit bank of lamps."
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
            clearance: 1.2,
            min_spawn_dist: 22.0,
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
                [0.9, 0.3, 0.9],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        // Steel mast.
        prim(
            solid(cylinder_tapered(0.25, 9.0, 8, 0.12, steel(STEEL_GREY))),
            [0.0, 4.8, 0.0],
            id_quat(),
        ),
    ];

    // Two collar rings banding the mast.
    for y in [3.5_f32, 7.0] {
        prims.push(prim(
            solid(torus(0.06, 0.3, steel(STEEL_GREY))),
            [0.0, y, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }
    // Back support strut bracing the lamp head out toward the front.
    prims.push(prim(
        solid(cuboid_tapered([0.14, 0.14, 1.2], 0.0, steel(STEEL_GREY))),
        [0.0, 9.2, -0.25],
        quat_x(0.5),
    ));
    // Gridded lamp bank facing the −Z render front — emissive (the ruin pass
    // can darken it). The grid of cells reads as a lamp array.
    for g in super::lamp_bank([0.0, 9.6, -0.5], 2.4, 1.1, 4, 2, -1.0) {
        prims.push(g);
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FloodlightMast.build(""), "floodlight_mast");
    }
}
