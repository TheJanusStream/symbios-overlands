//! Dead pylon — an Alien-Monolithic *poor* secondary. A snapped pylon: a short
//! dead-stone stub on its base and the broken upper length fallen across the
//! ground, all light gone. The dormant kit of the lightless site.
//!
//! The fallen length lies tipped with a [`quat_x`].

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEAD_STONE, stone};

pub struct DeadPylon;

impl CatalogueEntry for DeadPylon {
    fn slug(&self) -> &'static str {
        "dead_pylon"
    }
    fn name(&self) -> &'static str {
        "Dead Pylon"
    }
    fn description(&self) -> &'static str {
        "Snapped pylon: a dead-stone stub on its base, the broken length fallen across the ground."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MONOLITH_POOR
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

fn build_tree() -> Generator {
    let mut prims = vec![
        // Dead-stone base — the root.
        prim(
            solid(cuboid_tapered([1.6, 0.4, 1.6], 0.0, stone(DEAD_STONE))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Snapped stub still standing.
    prims.push(prim(
        solid(cuboid_tapered([0.8, 2.4, 0.8], 0.2, stone(DEAD_STONE))),
        [0.0, 1.4, 0.0],
        id_quat(),
    ));
    // Broken upper length fallen across the ground.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 6.0, 0.7], 0.4, stone(DEAD_STONE))),
        [0.0, 0.5, 3.6],
        quat_x(1.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DeadPylon.build(""), "dead_pylon");
    }
}
