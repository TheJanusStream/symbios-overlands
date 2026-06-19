//! Ruined wall — the AncientClassical *poor* secondary. A crumbling
//! sandstone wall with a broken stepped top, a leaning stub column, and a
//! couple of tumbled blocks at its foot: the remains a poor settlement
//! shelters against.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, solid,
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
            [4.4, 0.3, 0.9],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];

    // A broken wall whose top steps down across its length.
    let segs = [(-1.6_f32, 2.4_f32), (-0.5, 1.7), (0.4, 2.0), (1.5, 1.0)];
    for (x, h) in segs {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.95, h, 0.7],
                0.03,
                sandstone(SANDSTONE_WEATHERED),
            )),
            [x, 0.3 + h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // A leaning stub column at one end.
    prims.push(prim(
        solid(cylinder_tapered(0.32, 1.6, 14, 0.06, marble(MARBLE_WHITE))),
        [-2.2, 1.1, 0.3],
        crate::catalogue::items::util::quat_x(0.12),
    ));

    // A couple of tumbled blocks at the foot, askew.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.8, 0.5, 0.7],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [2.3, 0.25, 0.6],
        quat_y(0.5),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, 0.5, 0.6],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [1.9, 0.25, -0.7],
        quat_y(-0.3),
    ));

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
