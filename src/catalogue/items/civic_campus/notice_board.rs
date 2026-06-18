//! Notice board — a Civic/Campus prop. A green pin-board panel under a small
//! gabled roof on two posts, papered with notices. Scatter clutter along the
//! quad paths.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{NOTICE_GREEN, PLANK_WOOD, STEEL_GREY, painted, plank, steel};

pub struct NoticeBoard;

impl CatalogueEntry for NoticeBoard {
    fn slug(&self) -> &'static str {
        "notice_board"
    }
    fn name(&self) -> &'static str {
        "Notice Board"
    }
    fn description(&self) -> &'static str {
        "Green pin-board under a small gabled roof, papered with notices."
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
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Left post — the root.
        prim(
            solid(cuboid_tapered([0.1, 2.0, 0.1], 0.0, steel(STEEL_GREY))),
            [-0.9, 1.0, 0.0],
            id_quat(),
        ),
    ];
    // Right post.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 2.0, 0.1], 0.0, steel(STEEL_GREY))),
        [0.9, 1.0, 0.0],
        id_quat(),
    ));

    // Green pin-board panel.
    prims.push(prim(
        solid(cuboid_tapered([1.9, 1.2, 0.1], 0.0, painted(NOTICE_GREEN))),
        [0.0, 1.4, 0.0],
        id_quat(),
    ));
    // A few white notices pinned up.
    for (px, py) in [(-0.5_f32, 1.6), (0.3, 1.5), (0.5, 1.2)] {
        prims.push(prim(
            cuboid_tapered([0.35, 0.45, 0.12], 0.0, painted([0.92, 0.92, 0.88])),
            [px, py, 0.0],
            id_quat(),
        ));
    }

    // Small gabled plank roof.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 0.3, 0.6], 0.6, plank(PLANK_WOOD))),
        [0.0, 2.2, 0.0],
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
        assert_sanitize_stable(&NoticeBoard.build(""), "notice_board");
    }
}
