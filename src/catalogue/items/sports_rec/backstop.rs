//! Backstop — a Sports/Recreation *poor* secondary. A tall chain-link
//! baseball backstop on steel posts with a forward overhang. The rusting
//! edge of the municipal rec ground.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHAIN_GREY, CONCRETE_GREY, STEEL_GREY, chainlink, concrete, steel};

pub struct Backstop;

impl CatalogueEntry for Backstop {
    fn slug(&self) -> &'static str {
        "backstop"
    }
    fn name(&self) -> &'static str {
        "Backstop"
    }
    fn description(&self) -> &'static str {
        "Tall chain-link baseball backstop on steel posts with a forward overhang."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.15_f32;

    let mut prims = vec![
        // Dirt-rimmed concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [6.5, pad_h, 1.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Three steel posts.
    for x in [-3.0_f32, 0.0, 3.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.15, 3.8, 0.15], 0.0, steel(STEEL_GREY))),
            [x, pad_h + 1.9, 0.0],
            id_quat(),
        ));
    }

    // Main chain-link panel.
    prims.push(prim(
        cuboid_tapered([6.2, 3.5, 0.05], 0.0, chainlink(CHAIN_GREY)),
        [0.0, pad_h + 1.85, 0.0],
        id_quat(),
    ));
    // Forward overhang panel tilted in over the plate.
    prims.push(prim(
        cuboid_tapered([6.2, 0.05, 1.4], 0.0, chainlink(CHAIN_GREY)),
        [0.0, pad_h + 3.4, 0.6],
        quat_x(0.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Backstop.build(""), "backstop");
    }
}
