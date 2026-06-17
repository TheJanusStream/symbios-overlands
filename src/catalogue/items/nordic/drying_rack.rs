//! Drying rack — a Nordic prop. A timber frame of poles strung with split
//! fish curing in the wind and a hung strip of homespun cloth: the everyday
//! work of a coastal steading.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{SHIELD_CREAM, WOOD_DARK, WOOD_WARM, cloth, timber};

/// Cured-fish silver.
const FISH_GREY: [f32; 3] = [0.62, 0.64, 0.66];

pub struct DryingRack;

impl CatalogueEntry for DryingRack {
    fn slug(&self) -> &'static str {
        "drying_rack"
    }
    fn name(&self) -> &'static str {
        "Drying Rack"
    }
    fn description(&self) -> &'static str {
        "Timber pole frame strung with curing fish and hung cloth."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let top_y = 2.4;

    let mut prims = vec![
        // Ground sill — the root.
        prim(
            solid(cuboid_tapered([4.2, 0.18, 0.3], 0.0, timber(WOOD_DARK))),
            [0.0, 0.09, 0.0],
            id_quat(),
        ),
    ];
    // End posts, slightly splayed.
    for (sx, lean) in [(-1.0_f32, -0.05_f32), (1.0, 0.05)] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, top_y, 8, 0.05, timber(WOOD_WARM))),
            [sx * 2.0, top_y * 0.5, 0.0],
            quat_x(lean),
        ));
    }
    // Two horizontal stringing rails.
    for y in [top_y, top_y - 0.8] {
        prims.push(prim(
            solid(cuboid_tapered([4.0, 0.09, 0.09], 0.0, timber(WOOD_DARK))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Split fish hung in a row off the top rail.
    for i in 0..7 {
        let x = -1.7 + i as f32 * 0.57;
        prims.push(prim(
            cuboid_tapered([0.14, 0.55, 0.05], 0.3, timber(FISH_GREY)),
            [x, top_y - 0.35, 0.0],
            id_quat(),
        ));
    }
    // Fish off the lower rail, offset.
    for i in 0..6 {
        let x = -1.4 + i as f32 * 0.57;
        prims.push(prim(
            cuboid_tapered([0.13, 0.5, 0.05], 0.3, timber(FISH_GREY)),
            [x, top_y - 1.1, 0.12],
            id_quat(),
        ));
    }
    // A hung strip of homespun cloth at one end.
    prims.push(prim(
        cuboid_tapered([0.6, 1.0, 0.05], 0.0, cloth(SHIELD_CREAM, WOOD_WARM)),
        [1.5, top_y - 0.55, -0.1],
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
        assert_sanitize_stable(&DryingRack.build(""), "drying_rack");
    }
}
