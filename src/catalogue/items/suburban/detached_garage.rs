//! Detached garage — a Suburban secondary. A standalone sided garage with a
//! roll-up door, a shingle roof, and a side window: the workshop and storage
//! at the back of the lot.

use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    GLASS_TINT, HEDGE_GREEN, ROOF_GREY, SIDING_SAGE, enamel, glass, render, shingle, siding,
};

pub struct DetachedGarage;

impl CatalogueEntry for DetachedGarage {
    fn slug(&self) -> &'static str {
        "detached_garage"
    }
    fn name(&self) -> &'static str {
        "Detached Garage"
    }
    fn description(&self) -> &'static str {
        "Standalone sided garage with a roll-up door under a shingle roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
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
    let w = 6.5_f32;
    let d = 6.5_f32;
    let base_h = 0.4;
    let body_h = 3.0;
    // Hero face (roll-up door + man-door) on the -Z front.
    let front = -d * 0.5;

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [w + 0.6, base_h, d + 0.6],
                0.0,
                render([0.55, 0.55, 0.56]),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Sided body.
        prim(
            solid(cuboid_tapered([w, body_h, d], 0.0, siding(SIDING_SAGE))),
            [0.0, base_h + body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Sectional roll-up door on the front: white surround, panel, panel lines.
    let door_w = w - 2.6;
    let door_h = body_h - 0.5;
    let door_x = -0.7;
    prims.push(prim(
        solid(cuboid_tapered(
            [door_w + 0.4, door_h + 0.3, 0.16],
            0.0,
            enamel([0.86, 0.86, 0.84]),
        )),
        [door_x, base_h + (door_h + 0.3) * 0.5, front - 0.05],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([door_w, door_h, 0.2], 0.0, enamel([0.80, 0.80, 0.78])),
        [door_x, base_h + door_h * 0.5, front - 0.12],
        id_quat(),
    ));
    for k in 1..4 {
        let y = base_h + door_h * (k as f32 / 4.0);
        prims.push(prim(
            cuboid_tapered([door_w, 0.05, 0.06], 0.0, enamel([0.58, 0.58, 0.56])),
            [door_x, y, front - 0.2],
            id_quat(),
        ));
    }

    // A man-door beside the roll-up.
    let man_x = w * 0.5 - 0.7;
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, 2.1, 0.16],
            0.0,
            enamel([0.52, 0.46, 0.36]),
        )),
        [man_x, base_h + 1.05, front - 0.08],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.5, 0.12], 0.0, glass(GLASS_TINT, 0.0)),
        [man_x, base_h + 1.6, front - 0.16],
        id_quat(),
    ));

    // Side window.
    prims.push(prim(
        cuboid_tapered([0.2, 1.0, 1.2], 0.0, glass(GLASS_TINT, 0.0)),
        [w * 0.5, base_h + 1.8, 1.0],
        id_quat(),
    ));

    // Front-gable shingle roof: the triangle faces the door.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [w + 1.0, 1.9, d + 1.0],
            [0.94, 0.0],
            shingle(ROOF_GREY),
        )),
        [0.0, base_h + body_h + 0.95, 0.0],
        id_quat(),
    ));

    // A clipped shrub at the back corner.
    prims.extend(crop_tufts(
        [-w * 0.5 + 0.6, base_h, d * 0.5 - 0.6],
        [1.2, 1.0],
        2,
        2,
        0.8,
        foliage(HEDGE_GREEN),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DetachedGarage.build(""), "detached_garage");
    }
}
