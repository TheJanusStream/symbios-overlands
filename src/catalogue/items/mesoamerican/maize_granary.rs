//! Maize granary — a Mesoamerican *poor* secondary. A cuezcomatl: a fat
//! round adobe storage jar on a stone foot, capped with a conical thatch
//! lid, where a household keeps its dried maize beside the
//! [`adobe_hut`](super::adobe_hut).

use crate::catalogue::items::util::{assemble, cone, cylinder_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ADOBE_TAN, STONE_GREY, THATCH_STRAW, cobble, painted, thatch};

pub struct MaizeGranary;

impl CatalogueEntry for MaizeGranary {
    fn slug(&self) -> &'static str {
        "maize_granary"
    }
    fn name(&self) -> &'static str {
        "Maize Granary"
    }
    fn description(&self) -> &'static str {
        "Round adobe storage jar on a stone foot under a conical thatch lid."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_POOR
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
    let mut prims = vec![
        // Stone foot — the root.
        prim(
            solid(cylinder_tapered(1.2, 0.4, 12, 0.0, cobble(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Fat adobe body, slightly bellied (taper in toward the rim).
        prim(
            solid(cylinder_tapered(1.35, 2.0, 16, 0.18, painted(ADOBE_TAN))),
            [0.0, 1.4, 0.0],
            id_quat(),
        ),
    ];

    // Adobe rim collar.
    prims.push(prim(
        solid(cylinder_tapered(1.0, 0.3, 14, 0.0, painted(ADOBE_TAN))),
        [0.0, 2.55, 0.0],
        id_quat(),
    ));
    // Conical thatch lid.
    prims.push(prim(
        solid(cone(1.3, 1.3, 14, thatch(THATCH_STRAW))),
        [0.0, 2.9, 0.0],
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
        assert_sanitize_stable(&MaizeGranary.build(""), "maize_granary");
    }
}
