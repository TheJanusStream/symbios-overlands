//! Hitching post — a Wild-West prop. A timber hitching rail beside a water
//! trough. Scatter clutter along the boardwalk.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_RAW, clapboard};

pub struct HitchingPost;

impl CatalogueEntry for HitchingPost {
    fn slug(&self) -> &'static str {
        "hitching_post"
    }
    fn name(&self) -> &'static str {
        "Hitching Post"
    }
    fn description(&self) -> &'static str {
        "Timber hitching rail beside a water trough."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Top rail — the root.
        prim(
            solid(cuboid_tapered([2.6, 0.14, 0.14], 0.0, clapboard(WOOD_RAW))),
            [0.0, 1.0, 0.0],
            id_quat(),
        ),
    ];
    // Two posts.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.16, 1.2, 0.16], 0.0, clapboard(WOOD_RAW))),
            [sx * 1.1, 0.6, 0.0],
            id_quat(),
        ));
    }
    // A plank water trough alongside.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 0.5, 0.7], 0.0, clapboard(WOOD_RAW))),
        [0.0, 0.25, 0.9],
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
        assert_sanitize_stable(&HitchingPost.build(""), "hitching_post");
    }
}
