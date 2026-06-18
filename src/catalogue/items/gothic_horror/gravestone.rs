//! Gravestone — a Gothic-Horror prop. A single weathered headstone leaning
//! over a low grave mound. Scatter clutter strewn through the necropolis.
//!
//! The stone leans with a single [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_MOSS, mossy};

pub struct Gravestone;

impl CatalogueEntry for Gravestone {
    fn slug(&self) -> &'static str {
        "gravestone"
    }
    fn name(&self) -> &'static str {
        "Gravestone"
    }
    fn description(&self) -> &'static str {
        "Weathered headstone leaning over a low grave mound."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Low grave mound — the root.
        prim(
            solid(cylinder_tapered(0.8, 0.25, 12, 0.3, mossy(STONE_MOSS))),
            [0.0, 0.12, 0.4],
            id_quat(),
        ),
        // Leaning headstone.
        prim(
            solid(cuboid_tapered([0.7, 1.0, 0.16], 0.1, mossy(STONE_MOSS))),
            [0.0, 0.6, -0.4],
            quat_x(0.14),
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
        assert_sanitize_stable(&Gravestone.build(""), "gravestone");
    }
}
