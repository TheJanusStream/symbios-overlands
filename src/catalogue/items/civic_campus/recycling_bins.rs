//! Recycling bins — a Civic/Campus *poor* prop. A row of three colour-coded
//! wheelie bins, lids ajar, on small castors. The overflowing clutter of the
//! underfunded quarter.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::painted;

/// Bin body colours: green, blue, brown.
const BIN_GREEN: [f32; 3] = [0.20, 0.42, 0.26];
const BIN_BLUE: [f32; 3] = [0.18, 0.30, 0.52];
const BIN_BROWN: [f32; 3] = [0.40, 0.28, 0.16];

pub struct RecyclingBins;

impl CatalogueEntry for RecyclingBins {
    fn slug(&self) -> &'static str {
        "recycling_bins"
    }
    fn name(&self) -> &'static str {
        "Recycling Bins"
    }
    fn description(&self) -> &'static str {
        "A row of three colour-coded wheelie bins with lids ajar."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One wheelie bin (body + ajar lid) returned for the assemble list at `x`.
fn bin(x: f32, color: [f32; 3]) -> Generator {
    let mut body = prim(
        solid(cuboid_tapered([0.6, 1.0, 0.6], 0.0, painted(color))),
        [x, 0.55, 0.0],
        id_quat(),
    );
    // Lid tilted ajar at the back.
    body.children.push(prim(
        solid(cuboid_tapered(
            [0.62, 0.1, 0.64],
            0.0,
            painted([0.1, 0.1, 0.1]),
        )),
        [0.0, 0.55, -0.1],
        quat_x(0.4),
    ));
    body
}

fn build_tree() -> Generator {
    let prims = vec![
        bin(-0.7, BIN_GREEN),
        bin(0.0, BIN_BLUE),
        bin(0.7, BIN_BROWN),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RecyclingBins.build(""), "recycling_bins");
    }
}
