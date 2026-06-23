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
    // Plank frame proud of the -Z front face, so it reads as a glazed case.
    let fr = -0.07_f32;
    for (sz, c) in [
        ([2.0_f32, 0.14, 0.08], [0.0_f32, 2.02]),
        ([2.0, 0.14, 0.08], [0.0, 0.78]),
    ] {
        prims.push(prim(
            solid(cuboid_tapered(sz, 0.0, plank(PLANK_WOOD))),
            [c[0], c[1], fr],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, 1.4, 0.08], 0.0, plank(PLANK_WOOD))),
            [sx * 0.95, 1.4, fr],
            id_quat(),
        ));
    }
    // Brass nameplate header band.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.6, 0.22, 0.05],
            0.0,
            painted([0.66, 0.54, 0.26]),
        )),
        [0.0, 1.86, fr - 0.02],
        id_quat(),
    ));
    // Varied flyers pinned proud of the front, no two coplanar with the board.
    let flyers = [
        (-0.52_f32, 1.5, [0.93, 0.93, 0.89]),
        (0.0, 1.58, [0.92, 0.86, 0.42]),
        (0.52, 1.42, [0.88, 0.62, 0.66]),
        (-0.3, 1.12, [0.9, 0.9, 0.86]),
        (0.42, 1.08, [0.56, 0.72, 0.84]),
    ];
    for (px, py, color) in flyers {
        prims.push(prim(
            cuboid_tapered([0.34, 0.42, 0.03], 0.0, painted(color)),
            [px, py, fr - 0.02],
            id_quat(),
        ));
    }

    // Small gabled plank roof, proud of the case.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 0.3, 0.7], 0.6, plank(PLANK_WOOD))),
        [0.0, 2.22, fr * 0.5],
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
