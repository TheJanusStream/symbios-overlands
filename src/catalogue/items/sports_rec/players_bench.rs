//! Players' bench — a Sports/Recreation prop. A covered team dugout: a bench
//! under a corrugated shelter roof on steel posts. Scatter clutter along the
//! touchline.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CORRUGATED_GREY, SEAT_BLUE, STEEL_GREY, corrugated, enamel, steel};

pub struct PlayersBench;

impl CatalogueEntry for PlayersBench {
    fn slug(&self) -> &'static str {
        "players_bench"
    }
    fn name(&self) -> &'static str {
        "Players' Bench"
    }
    fn description(&self) -> &'static str {
        "Covered team dugout: a bench under a corrugated shelter roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // A three-sided dugout: rear and side walls enclose the bench, which faces
    // out through the open −Z front (the render front) under a tilted roof.
    let mut prims = vec![
        // Bench seat — the root.
        prim(
            solid(cuboid_tapered([3.6, 0.18, 0.5], 0.0, enamel(SEAT_BLUE))),
            [0.0, 0.45, 0.1],
            id_quat(),
        ),
    ];

    // Bench back, toward the rear wall.
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.5, 0.1], 0.0, enamel(SEAT_BLUE))),
        [0.0, 0.7, 0.35],
        id_quat(),
    ));
    // Seat legs.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.45, 0.4], 0.0, steel(STEEL_GREY))),
            [sx * 1.6, 0.22, 0.1],
            id_quat(),
        ));
    }

    // Corrugated rear wall closing the back.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.0, 2.0, 0.12],
            0.0,
            corrugated(CORRUGATED_GREY),
        )),
        [0.0, 1.1, 0.72],
        id_quat(),
    ));
    // Two corrugated side cheek walls.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, 2.0, 1.5],
                0.0,
                corrugated(CORRUGATED_GREY),
            )),
            [sx * 1.95, 1.1, 0.05],
            id_quat(),
        ));
    }
    // Front shelter posts at the open −Z mouth.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 2.2, 0.12], 0.0, steel(STEEL_GREY))),
            [sx * 1.9, 1.1, -0.65],
            id_quat(),
        ));
    }
    // Corrugated roof tilted out over the open front.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.2, 0.15, 1.9],
            0.0,
            corrugated(CORRUGATED_GREY),
        )),
        [0.0, 2.3, 0.0],
        quat_x(-0.18),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PlayersBench.build(""), "players_bench");
    }
}
