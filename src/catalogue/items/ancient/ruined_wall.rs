//! Ruined wall — the AncientClassical *poor* secondary. A crumbling
//! sandstone wall with a broken stepped top, a leaning stub column, and a
//! couple of tumbled blocks at its foot: the remains a poor settlement
//! shelters against.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid, torus,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, SANDSTONE_WEATHERED, marble, sandstone};

pub struct RuinedWall;

impl CatalogueEntry for RuinedWall {
    fn slug(&self) -> &'static str {
        "ruined_wall"
    }
    fn name(&self) -> &'static str {
        "Ruined Wall"
    }
    fn description(&self) -> &'static str {
        "Crumbling sandstone wall with a broken top, a stub column, and tumbled blocks."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.6,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Footing course — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [4.8, 0.3, 1.0],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];

    // Two piers flanking an arched opening: a fairly intact tall pier on the
    // left carrying the springer, and a lower broken pier on the right.
    let left_h = 2.9_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [1.2, left_h, 0.85],
            0.03,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [-1.6, 0.3 + left_h * 0.5, 0.0],
        id_quat(),
    ));
    let right_h = 1.9_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [1.1, right_h, 0.85],
            0.03,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [1.6, 0.3 + right_h * 0.5, 0.0],
        id_quat(),
    ));
    // A crumbled block tumbled on the broken right pier's jagged top.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.7, 0.5, 0.6],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [1.75, 0.3 + right_h + 0.2, -0.1],
        quat_y(0.4),
    ));

    // Broken arch springing from the left pier's impost over the opening and
    // snapping off before it reaches the right pier — `path_cut [0.15,0.5]`
    // keeps the left springer and crown and drops the collapsed right half.
    let spring_y = 0.3 + 1.9;
    prims.push(prim(
        with_cut(
            torus(0.22, 1.1, sandstone(SANDSTONE_WEATHERED)),
            [0.15, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, spring_y, 0.0],
        quat_x(-FRAC_PI_2),
    ));

    // A leaning marble stub column at the left end — relic of a grander past.
    prims.push(prim(
        solid(cylinder_tapered(0.32, 1.6, 14, 0.06, marble(MARBLE_WHITE))),
        [-2.7, 1.1, 0.4],
        quat_x(0.12),
    ));

    // Tumbled blocks + rubble at the foot, askew.
    let rubble = [
        ([0.8_f32, 0.5, 0.7], [2.6_f32, 0.25, 0.6], 0.5_f32),
        ([0.6, 0.5, 0.6], [2.2, 0.25, -0.7], -0.3),
        ([0.5, 0.35, 0.55], [-2.5, 0.18, -0.5], 0.9),
    ];
    for (size, pos, yaw) in rubble {
        prims.push(prim(
            solid(cuboid_tapered(size, 0.0, sandstone(SANDSTONE_WEATHERED))),
            pos,
            quat_y(yaw),
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
        assert_sanitize_stable(&RuinedWall.build(""), "ruined_wall");
    }
}
