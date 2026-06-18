//! Cob roundhouse — the Solarpunk *poor* landmark. A hand-built round cob
//! house under a conical living roof, with a timber door and a little window.
//! The grassroots counterpart to the [`biodome`](super::biodome): same green
//! ethic, opposite end of the prosperity axis (`Poor`), so a destitute
//! solarpunk room grows the makeshift commune instead of the eco-quarter.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the cob wall.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{COB_EARTH, GLASS_CLEAN, MOSS_GREEN, TIMBER_WARM, foliage, glass, timber};

pub struct CobRoundhouse;

impl CatalogueEntry for CobRoundhouse {
    fn slug(&self) -> &'static str {
        "cob_roundhouse"
    }
    fn name(&self) -> &'static str {
        "Cob Roundhouse"
    }
    fn description(&self) -> &'static str {
        "Hand-built round cob house under a conical living roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let wall_h = 2.6_f32;

    let mut prims = vec![
        // Round cob wall — the root.
        prim(
            solid(cylinder_tapered(2.6, wall_h, 16, 0.06, foliage(COB_EARTH))),
            [0.0, wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Conical living roof.
    prims.push(prim(
        solid(cone(3.2, 2.2, 16, foliage(MOSS_GREEN))),
        [0.0, wall_h + 1.1, 0.0],
        id_quat(),
    ));

    // Timber door on the +Z face.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.8, 0.2], 0.0, timber(TIMBER_WARM))),
        [0.0, 0.9, 2.45],
        id_quat(),
    ));
    // A little window.
    prims.push(prim(
        cuboid_tapered([0.6, 0.6, 0.15], 0.0, glass(GLASS_CLEAN, 0.6)),
        [1.3, 1.7, 2.1],
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
        assert_sanitize_stable(&CobRoundhouse.build(""), "cob_roundhouse");
    }
}
