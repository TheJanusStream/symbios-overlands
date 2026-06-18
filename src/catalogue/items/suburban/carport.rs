//! Carport — a Suburban *poor* secondary. A cheap open metal carport on four
//! posts sheltering a tired old car, pitched beside the
//! [`trailer_home`](super::trailer_home).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, enamel, glass, render};

/// Faded car paint.
const OLD_CAR: [f32; 3] = [0.42, 0.40, 0.34];

pub struct Carport;

impl CatalogueEntry for Carport {
    fn slug(&self) -> &'static str {
        "carport"
    }
    fn name(&self) -> &'static str {
        "Carport"
    }
    fn description(&self) -> &'static str {
        "Open metal carport on four posts sheltering a tired old car."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 6.0_f32;
    let d = 6.0_f32;
    let post_h = 2.8;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [w + 0.5, 0.3, d + 0.5],
                0.0,
                render([0.5, 0.5, 0.51]),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Four posts.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.12,
                post_h,
                8,
                0.0,
                enamel([0.7, 0.7, 0.72]),
            )),
            [sx * w * 0.45, 0.3 + post_h * 0.5, sz * d * 0.45],
            id_quat(),
        ));
    }
    // Shallow peaked metal roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.8, 0.7, d + 0.8],
            0.25,
            enamel([0.74, 0.74, 0.72]),
        )),
        [0.0, 0.3 + post_h + 0.35, 0.0],
        id_quat(),
    ));

    // A tired old car under it.
    prims.push(prim(
        solid(cuboid_tapered([1.9, 1.0, 4.2], 0.08, enamel(OLD_CAR))),
        [0.0, 0.8, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.7, 0.7, 2.4], 0.2, enamel(OLD_CAR))),
        [-0.2, 1.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.6, 0.5, 2.42], 0.2, glass(GLASS_TINT, 0.0)),
        [-0.2, 1.5, 0.0],
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
        assert_sanitize_stable(&Carport.build(""), "carport");
    }
}
