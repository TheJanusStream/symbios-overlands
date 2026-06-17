//! Idol — a Mesoamerican prop. A squat carved stone deity on a plinth: a
//! blocky seated figure with a gold headdress, a jade collar, and faintly
//! glowing jade eyes that watch the precinct.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GOLD_WARM, JADE_GREEN, STONE_GREY, cobble, gold, jade};

/// Cold jade glow worked into the carved eyes.
const EYE_GLOW: [f32; 3] = [0.35, 0.85, 0.55];

pub struct Idol;

impl CatalogueEntry for Idol {
    fn slug(&self) -> &'static str {
        "idol"
    }
    fn name(&self) -> &'static str {
        "Idol"
    }
    fn description(&self) -> &'static str {
        "Carved stone deity with a gold headdress and glowing jade eyes."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Plinth — the root.
        prim(
            solid(cuboid_tapered([1.3, 0.4, 1.1], 0.0, cobble(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Seated lower body / legs.
        prim(
            solid(cuboid_tapered([1.1, 0.9, 0.9], 0.1, cobble(STONE_GREY))),
            [0.0, 0.85, 0.0],
            id_quat(),
        ),
        // Torso.
        prim(
            solid(cuboid_tapered([0.9, 1.0, 0.7], 0.05, cobble(STONE_GREY))),
            [0.0, 1.8, 0.0],
            id_quat(),
        ),
        // Head.
        prim(
            solid(cuboid_tapered([0.7, 0.7, 0.6], 0.05, cobble(STONE_GREY))),
            [0.0, 2.6, 0.0],
            id_quat(),
        ),
    ];

    // Jade collar.
    prims.push(prim(
        cuboid_tapered([0.95, 0.2, 0.75], 0.0, jade(JADE_GREEN)),
        [0.0, 2.25, 0.0],
        id_quat(),
    ));
    // Gold headdress.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 0.5, 0.7], 0.4, gold(GOLD_WARM))),
        [0.0, 3.1, 0.0],
        id_quat(),
    ));
    // Faintly glowing jade eyes.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.14, 0.1, 0.06], 0.0, glow(EYE_GLOW, 1.6)),
            [sx * 0.16, 2.65, 0.31],
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
        assert_sanitize_stable(&Idol.build(""), "idol");
    }
}
