//! Dinghy — a Coastal-Resort prop. A small open planked rowboat hauled up on a
//! patch of sand: a flat sole, flared planked sides with a white boot-top
//! strake, a stern transom and a raked bow, an open interior with two thwart
//! benches, oarlocks and a pair of oars laid along the gunwales.
//!
//! The hull is built from flat planks (a sole + two flared side planks +
//! transoms) rather than a closed cylinder, so the open interior actually reads
//! as an open boat instead of a sealed barrel. Every piece hangs off a flat
//! sand-patch root: a rotated/closed `prims[0]` would both spin the reparented
//! furniture upright and hide the interior (the old bug). The flat root keeps
//! the thwarts horizontal and the trough open.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{AWNING_WHITE, DECK_WOOD, HULL_BLUE, SAND_TAN, enamel, plank, sand};

/// Dark stained interior of the open hull.
const HULL_DARK: [f32; 3] = [0.16, 0.12, 0.10];

pub struct Dinghy;

impl CatalogueEntry for Dinghy {
    fn slug(&self) -> &'static str {
        "dinghy"
    }
    fn name(&self) -> &'static str {
        "Dinghy"
    }
    fn description(&self) -> &'static str {
        "Small painted rowboat with thwart benches and oars, hauled up on the sand."
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
            clearance: 2.2,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let len = 2.8_f32; // hull length along Z (bow at -Z, stern at +Z)
    let mut prims = vec![
        // Beached sand patch — the flat root (keeps the reparented hull pieces
        // and furniture horizontal and the trough open).
        prim(
            solid(cylinder_tapered(1.9, 0.12, 20, 0.0, sand(SAND_TAN))),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
    ];

    // Flat painted sole — the boat's bottom plank.
    prims.push(prim(
        solid(cuboid_tapered([0.56, 0.16, len], 0.0, plank(HULL_BLUE))),
        [0.0, 0.3, 0.0],
        id_quat(),
    ));
    // Dark stained interior floorboard, visible down between the flared sides.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.06, len - 0.1],
            0.0,
            plank(HULL_DARK),
        )),
        [0.0, 0.43, 0.0],
        id_quat(),
    ));

    // Flared planked sides (wider at the gunwale than the sole = a hull, not a
    // barrel) with a white boot-top strake along each.
    for sx in [-1.0_f32, 1.0] {
        let flare = -sx * 0.3;
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.62, len], 0.0, plank(HULL_BLUE))),
            [sx * 0.34, 0.62, 0.0],
            quat_z(flare),
        ));
        prims.push(prim(
            cuboid_tapered([0.07, 0.14, len + 0.05], 0.0, enamel(AWNING_WHITE)),
            [sx * 0.5, 0.82, 0.0],
            quat_z(flare),
        ));
        // Timber gunwale cap riding the flared top edge.
        prims.push(prim(
            solid(cuboid_tapered([0.16, 0.1, len], 0.0, plank(DECK_WOOD))),
            [sx * 0.56, 0.95, 0.0],
            quat_z(flare),
        ));
    }

    // Stern transom closing the +Z end, and a raked bow transom at -Z.
    prims.push(prim(
        solid(cuboid_tapered([0.86, 0.66, 0.1], 0.0, plank(HULL_BLUE))),
        [0.0, 0.6, len * 0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.66, 0.66, 0.1], 0.0, plank(HULL_BLUE))),
        [0.0, 0.66, -(len * 0.5) - 0.05],
        quat_x(0.34),
    ));

    // Two thwart benches across the open hull (horizontal — the root is flat).
    for sz in [-0.7_f32, 0.6] {
        prims.push(prim(
            solid(cuboid_tapered([0.92, 0.08, 0.28], 0.0, plank(DECK_WOOD))),
            [0.0, 0.76, sz],
            id_quat(),
        ));
    }

    // Oarlocks and a pair of oars laid fore-and-aft along the gunwales.
    for sx in [-0.56_f32, 0.56] {
        prims.push(prim(
            solid(cylinder_tapered(0.04, 0.18, 6, 0.0, plank(DECK_WOOD))),
            [sx, 1.02, 0.2],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(0.04, 2.3, 6, 0.0, plank(DECK_WOOD))),
            [sx, 1.04, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Dinghy.build(""), "dinghy");
    }
}
