//! Column drum — an AncientClassical prop. The ruin of a fallen column: a
//! base and two standing fluted drums, a toppled drum on the ground, and a
//! tumbled capital block. Scattered marble clutter that says "old ruins".

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, SANDSTONE_WEATHERED, marble, sandstone};

pub struct ColumnDrum;

impl CatalogueEntry for ColumnDrum {
    fn slug(&self) -> &'static str {
        "column_drum"
    }
    fn name(&self) -> &'static str {
        "Column Drum"
    }
    fn description(&self) -> &'static str {
        "Ruin of a fallen column: standing drums, a toppled drum, and a tumbled capital."
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
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Base drum — the root.
    let mut prims = vec![prim(
        solid(cylinder_tapered(0.46, 0.4, 16, 0.0, marble(MARBLE_WHITE))),
        [0.0, 0.2, 0.0],
        id_quat(),
    )];
    // Two standing drums.
    prims.push(prim(
        solid(cylinder_tapered(0.4, 0.7, 16, 0.04, marble(MARBLE_WHITE))),
        [0.0, 0.75, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.38, 0.7, 16, 0.04, marble(MARBLE_WHITE))),
        [0.05, 1.45, 0.0],
        id_quat(),
    ));

    // A toppled drum lying on the ground (axis along Z).
    prims.push(prim(
        solid(cylinder_tapered(
            0.4,
            0.9,
            16,
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [1.3, 0.4, 0.3],
        quat_x(FRAC_PI_2),
    ));
    // Tumbled capital block beside it.
    prims.push(prim(
        solid(cuboid_tapered([0.8, 0.5, 0.8], 0.0, marble(MARBLE_WHITE))),
        [1.4, 0.25, -0.7],
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
        assert_sanitize_stable(&ColumnDrum.build(""), "column_drum");
    }
}
