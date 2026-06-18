//! Goalpost — a Sports/Recreation prop. A white goal frame with a chain-link
//! net slung behind it. Scatter clutter across the training pitches.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHAIN_GREY, LINE_WHITE, chainlink, enamel};

pub struct Goalpost;

impl CatalogueEntry for Goalpost {
    fn slug(&self) -> &'static str {
        "goalpost"
    }
    fn name(&self) -> &'static str {
        "Goalpost"
    }
    fn description(&self) -> &'static str {
        "White goal frame with a chain-link net slung behind."
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
            clearance: 4.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let h = 2.5_f32;
    let half_w = 3.5_f32;

    let mut prims = vec![
        // Crossbar — the root.
        prim(
            solid(cuboid_tapered(
                [half_w * 2.0 + 0.2, 0.12, 0.12],
                0.0,
                enamel(LINE_WHITE),
            )),
            [0.0, h, 0.0],
            id_quat(),
        ),
    ];

    // Two uprights.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, h, 0.12], 0.0, enamel(LINE_WHITE))),
            [sx * half_w, h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Chain-link net sloping back from the crossbar.
    prims.push(prim(
        cuboid_tapered([half_w * 2.0, 2.4, 0.05], 0.0, chainlink(CHAIN_GREY)),
        [0.0, h * 0.5 + 0.2, -0.9],
        quat_x(0.4),
    ));
    // Net floor behind the goal line.
    prims.push(prim(
        cuboid_tapered([half_w * 2.0, 0.05, 1.4], 0.0, chainlink(CHAIN_GREY)),
        [0.0, 0.1, -0.9],
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
        assert_sanitize_stable(&Goalpost.build(""), "goalpost");
    }
}
