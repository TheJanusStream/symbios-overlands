//! Lecture hall — a Civic/Campus secondary. A modern concrete auditorium
//! with a full-height glass curtain wall, a cantilevered entrance canopy and
//! a clerestory band of glazing. The teaching block of the campus.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, GLASS_TINT, concrete, glass};

pub struct LectureHall;

impl CatalogueEntry for LectureHall {
    fn slug(&self) -> &'static str {
        "lecture_hall"
    }
    fn name(&self) -> &'static str {
        "Lecture Hall"
    }
    fn description(&self) -> &'static str {
        "Modern concrete auditorium with a glass curtain wall and entrance canopy."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.4_f32;
    let body_h = 4.5_f32;
    let body_top = slab_h + body_h;

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [12.0, slab_h, 9.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Concrete body.
    prims.push(prim(
        solid(cuboid_tapered(
            [10.0, body_h, 7.0],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, slab_h + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // Full-height glass curtain wall on the front.
    prims.push(prim(
        cuboid_tapered([9.0, 3.6, 0.2], 0.0, glass(GLASS_TINT, 1.3)),
        [0.0, slab_h + 2.0, 3.55],
        id_quat(),
    ));
    // Cantilevered entrance canopy over the doors.
    prims.push(prim(
        solid(cuboid_tapered(
            [6.0, 0.3, 2.6],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, slab_h + 3.2, 4.6],
        id_quat(),
    ));
    // Roof cap + clerestory band of glazing.
    prims.push(prim(
        solid(cuboid_tapered(
            [10.4, 0.4, 7.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, body_top + 0.2, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([9.4, 0.7, 0.2], 0.0, glass(GLASS_TINT, 1.1)),
        [0.0, body_top - 0.5, 3.5],
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
        assert_sanitize_stable(&LectureHall.build(""), "lecture_hall");
    }
}
