//! Hitching post — a Wild-West prop. A round-log hitching rail with iron
//! tie-rings beside a hewn-log water trough. Scatter clutter along the
//! boardwalk.
//!
//! The rail is a [`quat_z`]-rotated log, so it is demoted to a child of an
//! upright post root: [`assemble`] rebases only child *translation*, so a
//! rotated *root* would spin every sibling into its frame.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, WOOD_RAW, canvas, clapboard, iron};

pub struct HitchingPost;

impl CatalogueEntry for HitchingPost {
    fn slug(&self) -> &'static str {
        "hitching_post"
    }
    fn name(&self) -> &'static str {
        "Hitching Post"
    }
    fn description(&self) -> &'static str {
        "Round-log hitching rail with iron tie-rings beside a hewn-log water trough."
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
    let rail_y = 1.1_f32;
    let post_h = rail_y + 0.1;

    let mut prims = vec![
        // Left post — the root. Must be identity-rotation: the rotated rail
        // can only be a child, never the assemble root.
        prim(
            solid(cylinder_tapered(0.1, post_h, 8, 0.0, clapboard(WOOD_RAW))),
            [-1.1, post_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Right post.
    prims.push(prim(
        solid(cylinder_tapered(0.1, post_h, 8, 0.0, clapboard(WOOD_RAW))),
        [1.1, post_h * 0.5, 0.0],
        id_quat(),
    ));
    // Round top rail (a log) running along X — a child, not the root.
    prims.push(prim(
        solid(cylinder_tapered(0.08, 2.6, 8, 0.0, clapboard(WOOD_RAW))),
        [0.0, rail_y, 0.0],
        quat_z(FRAC_PI_2),
    ));
    // Iron tie-rings hanging from the rail (vertical hoops).
    for x in [-0.5_f32, 0.5] {
        prims.push(prim(
            solid(torus(0.03, 0.11, iron(IRON_DARK))),
            [x, rail_y - 0.13, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    // A hewn-log water trough alongside, with a still water surface.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 0.5, 0.7], 0.0, clapboard(WOOD_RAW))),
        [0.0, 0.25, 0.95],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 0.62, 0.7], 0.0, clapboard(WOOD_RAW))),
            [sx * 1.04, 0.31, 0.95],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0, 0.06, 0.55],
            0.0,
            canvas([0.3, 0.4, 0.46]),
        )),
        [0.0, 0.46, 0.95],
        id_quat(),
    ));
    // A tin pail set by the trough.
    prims.push(prim(
        solid(cylinder_tapered(0.16, 0.34, 10, 0.08, iron(IRON_DARK))),
        [1.45, 0.17, 0.95],
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
