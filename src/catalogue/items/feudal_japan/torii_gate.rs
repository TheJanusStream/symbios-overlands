//! Torii gate — a Feudal-Japan secondary. The iconic vermilion gateway:
//! two lacquered pillars on stone footings carrying a curved double lintel
//! (kasagi over shimaki) and a tie beam (nuki) pierced through, with a
//! central plaque. Marks the threshold of the sacred ground.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{LACQUER_RED, STONE_GREY, TIMBER_DARK, lacquer, stone, timber};

pub struct ToriiGate;

impl CatalogueEntry for ToriiGate {
    fn slug(&self) -> &'static str {
        "torii_gate"
    }
    fn name(&self) -> &'static str {
        "Torii Gate"
    }
    fn description(&self) -> &'static str {
        "Vermilion gateway of two pillars under a curved double lintel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 2.6_f32; // half the pillar spacing
    let pillar_h = 5.5;
    let nuki_y = pillar_h * 0.72;

    let mut prims = vec![
        // Stone footing strip — the root.
        prim(
            solid(cuboid_tapered(
                [2.0 * span + 1.4, 0.4, 1.2],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Two lacquered pillars on stone bases, tapering up.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.8, 0.5, 0.8], 0.0, stone(STONE_GREY))),
            [sx * span, 0.45, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(
                0.32,
                pillar_h,
                10,
                0.12,
                lacquer(LACQUER_RED),
            )),
            [sx * span, 0.4 + pillar_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Nuki tie beam pierced through the pillars.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * span + 0.8, 0.42, 0.55],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, 0.4 + nuki_y, 0.0],
        id_quat(),
    ));

    // Shimaki beam, then the broad kasagi crown overhanging it.
    let top = 0.4 + pillar_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * span + 1.2, 0.5, 0.7],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, top + 0.25, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * span + 2.0, 0.55, 0.95],
            0.08,
            lacquer([
                LACQUER_RED[0] * 0.85,
                LACQUER_RED[1] * 0.85,
                LACQUER_RED[2] * 0.85,
            ]),
        )),
        [0.0, top + 0.75, 0.0],
        id_quat(),
    ));

    // Central plaque (gakuzuka) between the tie beam and the lintel.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, top - nuki_y - 0.2, 0.2],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, 0.4 + (nuki_y + pillar_h) * 0.5, 0.1],
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
        assert_sanitize_stable(&ToriiGate.build(""), "torii_gate");
    }
}
