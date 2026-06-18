//! Dead tree — a Gothic-Horror prop. A bare, gnarled, leafless tree, its
//! branches clawing at the fog. Scatter clutter haunting the necropolis.
//!
//! Branches tilt with a [`quat_x`].

use crate::catalogue::items::util::{assemble, cylinder_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEADWOOD, wood};

pub struct DeadTree;

impl CatalogueEntry for DeadTree {
    fn slug(&self) -> &'static str {
        "dead_tree"
    }
    fn name(&self) -> &'static str {
        "Dead Tree"
    }
    fn description(&self) -> &'static str {
        "Bare, gnarled, leafless tree, its branches clawing at the fog."
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
            clearance: 1.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Trunk — the root, tapering up.
        prim(
            solid(cylinder_tapered(0.32, 3.4, 8, 0.45, wood(DEADWOOD))),
            [0.0, 1.7, 0.0],
            id_quat(),
        ),
    ];

    // Gnarled branches reaching up at various heights and angles.
    for (y, len, tilt, z) in [
        (2.2_f32, 1.6_f32, 0.7_f32, 0.3_f32),
        (2.6, 1.4, -0.7, -0.3),
        (3.0, 1.2, 0.5, -0.2),
        (3.3, 1.0, -0.4, 0.25),
    ] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, len, 6, 0.6, wood(DEADWOOD))),
            [0.0, y + len * 0.25, z],
            quat_x(tilt),
        ));
    }

    // A few clawing twigs near the crown.
    for (tilt, z) in [(1.1_f32, 0.3_f32), (-1.1, -0.3)] {
        prims.push(prim(
            solid(cylinder_tapered(0.05, 0.8, 5, 0.7, wood(DEADWOOD))),
            [0.0, 3.5, z],
            quat_x(tilt),
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
        assert_sanitize_stable(&DeadTree.build(""), "dead_tree");
    }
}
