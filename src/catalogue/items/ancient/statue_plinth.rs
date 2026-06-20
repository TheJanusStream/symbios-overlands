//! Statue plinth — an AncientClassical prop. A marble pedestal carrying a
//! weathered draped figure with a lost head: the civic statuary of a
//! classical square.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, SANDSTONE_WEATHERED, marble, sandstone};

pub struct StatuePlinth;

impl CatalogueEntry for StatuePlinth {
    fn slug(&self) -> &'static str {
        "statue_plinth"
    }
    fn name(&self) -> &'static str {
        "Statue Plinth"
    }
    fn description(&self) -> &'static str {
        "Marble pedestal carrying a weathered draped figure."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Pedestal base — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [1.3, 0.3, 1.3],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];
    // Plinth die.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.2, 0.9], 0.05, marble(MARBLE_WHITE))),
        [0.0, 0.9, 0.0],
        id_quat(),
    ));
    // Cap.
    prims.push(prim(
        solid(cuboid_tapered([1.1, 0.2, 1.1], 0.0, marble(MARBLE_WHITE))),
        [0.0, 1.6, 0.0],
        id_quat(),
    ));

    // Draped figure: legs, robed torso (tapered), and a shoulder stub where
    // the head has broken away.
    prims.push(prim(
        solid(cylinder_tapered(0.28, 1.0, 12, 0.15, marble(MARBLE_WHITE))),
        [0.0, 2.2, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.34, 0.9, 12, 0.25, marble(MARBLE_WHITE))),
        [0.0, 3.0, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(0.18, 2, marble(MARBLE_WHITE))),
        [0.0, 3.5, 0.0],
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
        assert_sanitize_stable(&StatuePlinth.build(""), "statue_plinth");
    }
}
