//! Scoreboard — a Sports/Recreation prop. A freestanding lit display on two
//! steel posts. Scatter clutter at the pitch ends; its screen is emissive
//! trim the ruin pass can darken.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{SCORE_AMBER, STEEL_GREY, enamel, steel};

pub struct Scoreboard;

impl CatalogueEntry for Scoreboard {
    fn slug(&self) -> &'static str {
        "scoreboard"
    }
    fn name(&self) -> &'static str {
        "Scoreboard"
    }
    fn description(&self) -> &'static str {
        "Freestanding lit scoreboard on two steel posts."
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
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Housing — the root.
        prim(
            solid(cuboid_tapered(
                [4.5, 2.4, 0.4],
                0.0,
                enamel([0.12, 0.12, 0.14]),
            )),
            [0.0, 4.5, 0.0],
            id_quat(),
        ),
    ];

    // Two steel posts.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 4.0, 0.2], 0.0, steel(STEEL_GREY))),
            [sx * 1.8, 2.0, 0.0],
            id_quat(),
        ));
    }

    // Lit screen face — emissive.
    prims.push(prim(
        cuboid_tapered([4.0, 1.9, 0.12], 0.0, glow(SCORE_AMBER, 3.5)),
        [0.0, 4.5, 0.26],
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
        assert_sanitize_stable(&Scoreboard.build(""), "scoreboard");
    }
}
