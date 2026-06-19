//! Colonnade — an AncientClassical secondary. A stoa: a row of fluted
//! marble columns on a stepped sandstone stylobate carrying an architrave
//! and cornice. The open civic portico of a classical agora.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, SANDSTONE_GOLD, marble, sandstone};

pub struct Colonnade;

impl CatalogueEntry for Colonnade {
    fn slug(&self) -> &'static str {
        "colonnade"
    }
    fn name(&self) -> &'static str {
        "Colonnade"
    }
    fn description(&self) -> &'static str {
        "Row of fluted marble columns on a stepped stylobate under an architrave."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_BAND
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
    let l = 9.0_f32;
    let shaft_h = 3.2;

    // Three-step stylobate — the root is the bottom step.
    let mut prims = vec![
        prim(
            solid(cuboid_tapered(
                [l + 0.6, 0.3, 3.0],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [l + 0.2, 0.3, 2.6],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [0.0, 0.45, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [l - 0.2, 0.3, 2.3],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [0.0, 0.75, 0.0],
            id_quat(),
        ),
    ];
    let syl_top = 0.9;

    // Five fluted columns: base drum, tapered shaft, square capital.
    for col in 0..5 {
        let x = -3.6 + col as f32 * 1.8;
        prims.push(prim(
            solid(cylinder_tapered(0.42, 0.3, 16, 0.0, marble(MARBLE_WHITE))),
            [x, syl_top + 0.15, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(
                0.34,
                shaft_h,
                16,
                0.12,
                marble(MARBLE_WHITE),
            )),
            [x, syl_top + 0.3 + shaft_h * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.7, 0.35, 0.7], 0.0, marble(MARBLE_WHITE))),
            [x, syl_top + 0.3 + shaft_h + 0.175, 0.0],
            id_quat(),
        ));
    }

    // Architrave beam + cornice over the capitals.
    let entab_y = syl_top + 0.3 + shaft_h + 0.35;
    prims.push(prim(
        solid(cuboid_tapered(
            [l, 0.55, 0.95],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [0.0, entab_y + 0.27, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.4, 0.3, 1.2],
            0.0,
            marble(MARBLE_WHITE),
        )),
        [0.0, entab_y + 0.7, 0.0],
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
        assert_sanitize_stable(&Colonnade.build(""), "colonnade");
    }
}
