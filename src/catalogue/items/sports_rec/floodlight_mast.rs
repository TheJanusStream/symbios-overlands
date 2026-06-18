//! Floodlight mast — a Sports/Recreation prop. A tall steel tower carrying a
//! lit bank of lamps. Scatter clutter around the pitches; its lamp bank is
//! emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, FLOOD_LIT, STEEL_GREY, concrete, steel};

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
    let prims = vec![
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
        // Lamp-bank frame.
        prim(
            solid(cuboid_tapered([2.2, 1.0, 0.5], 0.0, steel(STEEL_GREY))),
            [0.0, 9.5, 0.2],
            id_quat(),
        ),
        // Lit lamp bank — emissive trim.
        prim(
            cuboid_tapered([2.0, 0.8, 0.2], 0.0, glow(FLOOD_LIT, 4.0)),
            [0.0, 9.5, 0.45],
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
        assert_sanitize_stable(&FloodlightMast.build(""), "floodlight_mast");
    }
}
