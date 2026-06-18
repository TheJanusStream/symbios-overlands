//! Deck chair — a Coastal-Resort prop. A folding timber-framed lounger with
//! a striped canvas seat reclined against a raised back. Scatter clutter for
//! the foreshore and the pool deck.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{AWNING_TEAL, AWNING_WHITE, DECK_WOOD, canvas, plank};

pub struct DeckChair;

impl CatalogueEntry for DeckChair {
    fn slug(&self) -> &'static str {
        "deck_chair"
    }
    fn name(&self) -> &'static str {
        "Deck Chair"
    }
    fn description(&self) -> &'static str {
        "Folding timber lounger with a reclined striped canvas seat."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.9,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Flat canvas seat — the root.
        prim(
            cuboid_tapered([0.62, 0.06, 1.2], 0.0, canvas(AWNING_TEAL, AWNING_WHITE)),
            [0.0, 0.45, 0.0],
            id_quat(),
        ),
    ];

    // Reclined canvas back at the head end (-Z).
    prims.push(prim(
        cuboid_tapered([0.62, 0.06, 0.9], 0.0, canvas(AWNING_TEAL, AWNING_WHITE)),
        [0.0, 0.78, -0.62],
        quat_x(0.7),
    ));

    // Timber armrest rails down both sides.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.06, 0.06, 1.2], 0.0, plank(DECK_WOOD))),
            [sx * 0.34, 0.55, 0.0],
            id_quat(),
        ));
    }

    // Four short legs.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.06, 0.45, 0.06], 0.0, plank(DECK_WOOD))),
                [sx * 0.28, 0.22, sz * 0.5],
                id_quat(),
            ));
        }
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DeckChair.build(""), "deck_chair");
    }
}
