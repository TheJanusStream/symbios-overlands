//! Boot hill — a Wild-West *poor* secondary. A dusty rise of leaning wooden
//! grave crosses behind a broken rail. The bust town's lonely cemetery.
//!
//! The crosses lean with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DUST_TAN, WOOD_RAW, canvas, clapboard};

pub struct BootHill;

impl CatalogueEntry for BootHill {
    fn slug(&self) -> &'static str {
        "boot_hill"
    }
    fn name(&self) -> &'static str {
        "Boot Hill"
    }
    fn description(&self) -> &'static str {
        "Dusty rise of leaning wooden grave crosses behind a broken rail."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One leaning grave cross (post + arm) for the assemble list.
fn cross(x: f32, z: f32, tilt: f32) -> Generator {
    let mut post = prim(
        solid(cuboid_tapered([0.12, 1.2, 0.12], 0.0, clapboard(WOOD_RAW))),
        [x, 0.6, z],
        quat_x(tilt),
    );
    post.children.push(prim(
        solid(cuboid_tapered([0.5, 0.12, 0.12], 0.0, clapboard(WOOD_RAW))),
        [0.0, 0.25, 0.0],
        id_quat(),
    ));
    post
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Dusty rise — the root.
        prim(
            solid(cylinder_tapered(2.6, 0.3, 16, 0.3, canvas(DUST_TAN))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // A scatter of leaning crosses.
    prims.push(cross(-1.2, 0.4, 0.16));
    prims.push(cross(0.3, -0.6, -0.2));
    prims.push(cross(1.1, 0.7, 0.12));
    prims.push(cross(-0.4, 1.2, -0.1));

    // A broken rail at the foot.
    prims.push(prim(
        solid(cuboid_tapered([3.4, 0.1, 0.1], 0.0, clapboard(WOOD_RAW))),
        [0.0, 0.7, 2.2],
        quat_x(0.1),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, 0.9, 0.14], 0.0, clapboard(WOOD_RAW))),
            [sx * 1.5, 0.55, 2.2],
            id_quat(),
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
        assert_sanitize_stable(&BootHill.build(""), "boot_hill");
    }
}
