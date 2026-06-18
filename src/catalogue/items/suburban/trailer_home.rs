//! Trailer home — the Suburban *poor* landmark. A single-wide mobile home on
//! cinder-block supports with a shallow metal roof, a window AC unit, and a
//! little entry step. The trailer-lot counterpart to the
//! [`community_center`](super::community_center): same theme, opposite end of
//! the prosperity axis (`Poor`), so a destitute room grows this instead of
//! the civic hall.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, SIDING_BLUE, TRAILER_WHITE, enamel, glass, render, siding};

pub struct TrailerHome;

impl CatalogueEntry for TrailerHome {
    fn slug(&self) -> &'static str {
        "trailer_home"
    }
    fn name(&self) -> &'static str {
        "Trailer Home"
    }
    fn description(&self) -> &'static str {
        "Single-wide mobile home on cinder blocks with a metal roof and AC unit."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 11.0_f32;
    let d = 4.0_f32;
    let floor_y = 0.7_f32;
    let body_h = 2.6_f32;
    let front = d * 0.5;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, 0.3, d + 0.6],
                0.0,
                render([0.5, 0.5, 0.51]),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Cinder-block supports.
    for sx in [-1.0_f32, -0.33, 0.33, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.5, floor_y - 0.3, 0.5],
                    0.0,
                    render([0.45, 0.45, 0.46]),
                )),
                [sx * l * 0.45, 0.3 + (floor_y - 0.3) * 0.5, sz * d * 0.35],
                id_quat(),
            ));
        }
    }

    // Body.
    prims.push(prim(
        solid(cuboid_tapered([l, body_h, d], 0.0, siding(TRAILER_WHITE))),
        [0.0, floor_y + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // Accent stripe.
    prims.push(prim(
        cuboid_tapered([l + 0.05, 0.3, d + 0.05], 0.0, enamel(SIDING_BLUE)),
        [0.0, floor_y + body_h * 0.6, 0.0],
        id_quat(),
    ));
    // Shallow metal roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.5, 0.4, d + 0.5],
            0.1,
            enamel([0.72, 0.72, 0.70]),
        )),
        [0.0, floor_y + body_h + 0.2, 0.0],
        id_quat(),
    ));

    // Sliding windows along the front.
    for c in 0..3 {
        let x = -l * 0.3 + c as f32 * (l * 0.3);
        prims.push(prim(
            cuboid_tapered([1.5, 0.9, 0.15], 0.0, glass(GLASS_TINT, 0.0)),
            [x, floor_y + 1.5, front],
            id_quat(),
        ));
    }
    // Door and step.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.85, 1.9, 0.15],
            0.0,
            enamel([0.7, 0.68, 0.62]),
        )),
        [l * 0.35, floor_y + 0.95, front],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [1.1, floor_y, 0.7],
            0.0,
            render([0.5, 0.5, 0.5]),
        )),
        [l * 0.35, floor_y * 0.5, front + 0.5],
        id_quat(),
    ));
    // Window AC unit.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.8, 0.6, 0.5],
            0.0,
            enamel([0.78, 0.78, 0.78]),
        )),
        [-l * 0.3, floor_y + 1.3, front + 0.3],
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
        assert_sanitize_stable(&TrailerHome.build(""), "trailer_home");
    }
}
