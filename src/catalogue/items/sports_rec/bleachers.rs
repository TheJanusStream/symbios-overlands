//! Bleachers — a Sports/Recreation secondary. An open raked stand of
//! aluminium seat rows on a steel frame, with a back rail. The terrace
//! seating that lines the pitch.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the pad.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, SEAT_BLUE, STEEL_GREY, concrete, enamel, steel};

pub struct Bleachers;

impl CatalogueEntry for Bleachers {
    fn slug(&self) -> &'static str {
        "bleachers"
    }
    fn name(&self) -> &'static str {
        "Bleachers"
    }
    fn description(&self) -> &'static str {
        "Open raked stand of aluminium seat rows on a steel frame."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.3_f32;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [10.0, pad_h, 5.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Five seat rows stepping up and back.
    for t in 0..5 {
        let tf = t as f32;
        // Seat plank.
        prims.push(prim(
            solid(cuboid_tapered([9.6, 0.2, 0.5], 0.0, enamel(SEAT_BLUE))),
            [0.0, pad_h + 0.5 + tf * 0.6, -1.6 + tf * 0.8],
            id_quat(),
        ));
        // Footboard below it.
        prims.push(prim(
            solid(cuboid_tapered([9.6, 0.15, 0.5], 0.0, steel(STEEL_GREY))),
            [0.0, pad_h + 0.2 + tf * 0.6, -1.3 + tf * 0.8],
            id_quat(),
        ));
    }

    // Back rail behind the top row.
    prims.push(prim(
        cuboid_tapered([9.8, 0.1, 0.08], 0.0, steel(STEEL_GREY)),
        [0.0, pad_h + 3.6, 1.9],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 1.0, 0.1], 0.0, steel(STEEL_GREY))),
            [sx * 4.6, pad_h + 3.1, 1.9],
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
        assert_sanitize_stable(&Bleachers.build(""), "bleachers");
    }
}
