//! Detached garage — a Suburban secondary. A standalone sided garage with a
//! roll-up door, a shingle roof, and a side window: the workshop and storage
//! at the back of the lot.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, ROOF_GREY, SIDING_SAGE, glass, render, shingle, siding};

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

    // Roll-up door on the front.
    prims.push(prim(
        cuboid_tapered(
            [w - 1.6, body_h - 0.5, 0.2],
            0.0,
            siding([0.78, 0.78, 0.76]),
        ),
        [0.0, base_h + (body_h - 0.5) * 0.5, d * 0.5],
        id_quat(),
    ));
    // Side window.
    prims.push(prim(
        cuboid_tapered([0.2, 1.0, 1.2], 0.0, glass(GLASS_TINT, 0.0)),
        [w * 0.5, base_h + 1.8, -1.0],
        id_quat(),
    ));

    // Shingle hip roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 1.0, 1.6, d + 1.0],
            0.45,
            shingle(ROOF_GREY),
        )),
        [0.0, base_h + body_h + 0.8, 0.0],
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
        assert_sanitize_stable(&DetachedGarage.build(""), "detached_garage");
    }
}
