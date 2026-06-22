//! Cob roundhouse — the Solarpunk *poor* landmark. A hand-built round cob
//! house under a conical living roof, with a timber door and a little window.
//! The grassroots counterpart to the [`biodome`](super::biodome): same green
//! ethic, opposite end of the prosperity axis (`Poor`), so a destitute
//! solarpunk room grows the makeshift commune instead of the eco-quarter.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the cob wall.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    COB_EARTH, CONCRETE_PALE, GLASS_CLEAN, MOSS_GREEN, TIMBER_WARM, concrete, foliage, glass,
    timber,
};

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

    // Low stone footing ring the cob is raised on (keeps the earth wall off
    // the wet ground).
    prims.push(prim(
        solid(cylinder_tapered(
            2.74,
            0.4,
            16,
            0.0,
            concrete(CONCRETE_PALE),
        )),
        [0.0, 0.2, 0.0],
        id_quat(),
    ));
    // Eave ring beam where the overhanging roof springs from the wall top.
    prims.push(prim(
        solid(torus(0.12, 2.95, timber(TIMBER_WARM))),
        [0.0, wall_h + 0.04, 0.0],
        id_quat(),
    ));
    // Conical living-turf roof, oversailing the wall as a sheltering eave.
    prims.push(prim(
        solid(cone(3.3, 2.2, 16, foliage(MOSS_GREEN))),
        [0.0, wall_h + 1.1, 0.0],
        id_quat(),
    ));
    // Smoke vent / finial at the apex — a little timber stack and cap.
    prims.push(prim(
        solid(cylinder_tapered(0.18, 0.55, 8, 0.0, timber(TIMBER_WARM))),
        [0.0, wall_h + 2.35, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(0.3, 0.26, 8, foliage(MOSS_GREEN))),
        [0.0, wall_h + 2.75, 0.0],
        id_quat(),
    ));

    // Timber door on the −Z hero front, framed.
    prims.push(prim(
        solid(cuboid_tapered([0.92, 1.8, 0.22], 0.0, timber(TIMBER_WARM))),
        [0.0, 0.9, -2.45],
        id_quat(),
    ));
    // A little glazed window beside the door.
    prims.push(prim(
        cuboid_tapered([0.6, 0.6, 0.15], 0.0, glass(GLASS_CLEAN, 0.6)),
        [1.15, 1.6, -2.18],
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
