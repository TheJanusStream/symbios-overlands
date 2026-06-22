//! Boot hill — a Wild-West *poor* secondary. A dusty rise of leaning wooden
//! grave crosses behind a broken rail. The bust town's lonely cemetery.
//!
//! The crosses lean with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, prim_scaled, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DUST_TAN, WOOD_RAW, canvas, clapboard, iron};

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

/// A low elongated mound of turned earth — a grave, its long axis along Z.
fn grave_mound(x: f32, z: f32) -> Generator {
    prim_scaled(
        solid(sphere(0.5, 4, canvas([0.5, 0.42, 0.3]))),
        [x, 0.34, z],
        id_quat(),
        [0.9, 0.5, 1.7],
    )
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

    // Grave mounds, each with a leaning cross at its head.
    prims.push(grave_mound(-1.2, 0.6));
    prims.push(grave_mound(0.3, -0.4));
    prims.push(grave_mound(1.1, 0.9));
    prims.push(cross(-1.2, -0.2, 0.16));
    prims.push(cross(0.3, -1.2, -0.2));
    prims.push(cross(1.1, 0.1, 0.12));

    // A weathered rounded headstone leaning at the back.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.7, 0.12],
            0.0,
            canvas([0.62, 0.6, 0.55]),
        )),
        [-0.5, 0.5, 1.2],
        quat_x(-0.1),
    ));
    prims.push(prim(
        solid(cylinder_tapered(
            0.25,
            0.12,
            12,
            0.0,
            canvas([0.62, 0.6, 0.55]),
        )),
        [-0.5, 0.83, 1.18],
        quat_x(-0.1 + std::f32::consts::FRAC_PI_2),
    ));

    // A bare dead tree at the edge of the plot.
    prims.push(prim(
        solid(cylinder_tapered(
            0.13,
            2.2,
            6,
            0.45,
            clapboard([0.3, 0.24, 0.16]),
        )),
        [-2.1, 1.1, -1.3],
        quat_x(0.18),
    ));
    for (bx, tilt) in [(0.6_f32, 0.9_f32), (-0.5, -0.8)] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.06,
                0.9,
                5,
                0.3,
                clapboard([0.3, 0.24, 0.16]),
            )),
            [-2.1 + bx * 0.3, 1.9, -1.2],
            quat_x(tilt),
        ));
    }

    // A shovel left stuck in the dirt.
    prims.push(prim(
        solid(cylinder_tapered(0.04, 1.1, 6, 0.0, clapboard(WOOD_RAW))),
        [1.6, 0.6, -0.6],
        quat_x(0.4),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.32, 0.42, 0.05],
            0.0,
            iron([0.44, 0.44, 0.47]),
        )),
        [1.6, 0.12, -0.34],
        quat_x(0.4),
    ));

    // A broken rail at the foot of the rise.
    prims.push(prim(
        solid(cuboid_tapered([3.4, 0.1, 0.1], 0.0, clapboard(WOOD_RAW))),
        [-0.3, 0.62, 2.2],
        quat_x(0.12),
    ));
    for (sx, h) in [(-1.5_f32, 0.9_f32), (1.5, 0.6)] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, h, 0.14], 0.0, clapboard(WOOD_RAW))),
            [sx, h * 0.5, 2.2],
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
