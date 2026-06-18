//! Water channel — a Solarpunk prop. A timber rill carrying a ribbon of
//! water past a little stone weir. Scatter clutter irrigating the gardens.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_PALE, TIMBER_WARM, WATER_BLUE, concrete, timber, water};

pub struct WaterChannel;

impl CatalogueEntry for WaterChannel {
    fn slug(&self) -> &'static str {
        "water_channel"
    }
    fn name(&self) -> &'static str {
        "Water Channel"
    }
    fn description(&self) -> &'static str {
        "Timber rill carrying a ribbon of water past a little stone weir."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Timber trough — the root, running along Z.
        prim(
            solid(cuboid_tapered([0.8, 0.4, 3.6], 0.0, timber(TIMBER_WARM))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Water ribbon in the trough (non-solid — no collider on water).
    prims.push(prim(
        cuboid_tapered([0.5, 0.1, 3.4], 0.0, water(WATER_BLUE)),
        [0.0, 0.34, 0.0],
        id_quat(),
    ));

    // A little concrete weir across the rill.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, 0.5, 0.2],
            0.0,
            concrete(CONCRETE_PALE),
        )),
        [0.0, 0.25, 0.6],
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
        assert_sanitize_stable(&WaterChannel.build(""), "water_channel");
    }
}
