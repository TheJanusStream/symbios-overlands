//! Scoreboard — a Sports/Recreation prop. A freestanding lit display on two
//! steel posts. Scatter clutter at the pitch ends; its screen is emissive
//! trim the ruin pass can darken.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STEEL_GREY, enamel, fx, steel};

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
        // Dark display housing — the root.
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

    // Two steel posts with a cross-brace.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 4.0, 0.2], 0.0, steel(STEEL_GREY))),
            [sx * 1.8, 2.0, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.16, 0.16], 0.0, steel(STEEL_GREY))),
        [0.0, 2.4, 0.0],
        id_quat(),
    ));
    // Steel cornice cap proud of the housing top (not flush — no z-fight).
    prims.push(prim(
        solid(cuboid_tapered([4.7, 0.2, 0.55], 0.0, steel(STEEL_GREY))),
        [0.0, 5.85, 0.0],
        id_quat(),
    ));

    // Segmented lit display facing the −Z render front — a low idle PA hum
    // sits on the board. Emissive (the ruin pass can snuff it).
    let mut disp = super::score_display(0.0, 4.5, -0.26, 4.0, 1.9);
    disp[0].audio = fx::tannoy_hum();
    prims.extend(disp);

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
