//! Drying rack — a Nordic prop. A tall timber *hjell* strung with split fish
//! curing in the wind over three rails and a hung strip of homespun cloth:
//! the everyday work of a coastal steading.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid,
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
        "Tall timber hjell strung with rows of curing fish and hung cloth."
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
    let top_y = 3.0;

    let mut prims = vec![
        // Ground sill — the root.
        prim(
            solid(cuboid_tapered([4.6, 0.2, 0.4], 0.0, timber(WOOD_DARK))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];
    // End posts, each pair splayed front-to-back for a stable hjell.
    for sx in [-1.0_f32, 1.0] {
        for (sz, lean) in [(-1.0_f32, -0.1_f32), (1.0, 0.1)] {
            prims.push(prim(
                solid(cylinder_tapered(0.1, top_y, 8, 0.06, timber(WOOD_WARM))),
                [sx * 2.1, top_y * 0.5, sz * 0.35],
                quat_x(lean),
            ));
        }
    }
    // Three stringing rails at descending heights.
    for y in [top_y - 0.1, top_y - 0.85, top_y - 1.6] {
        prims.push(prim(
            solid(cuboid_tapered([4.3, 0.09, 0.09], 0.0, timber(WOOD_DARK))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }
    // A pair of angle braces.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([2.0, 0.08, 0.08], 0.0, timber(WOOD_DARK))),
            [sx * 1.0, top_y - 0.8, 0.35],
            quat_z(sx * 0.6),
        ));
    }

    // Split fish hung in rows off each rail — tapered to the tail, alternating
    // offset so the rows read as a dense catch.
    for (row, (ry, n, off)) in [
        (top_y - 0.45, 8, 0.0_f32),
        (top_y - 1.2, 7, 0.28),
        (top_y - 1.95, 6, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        let z = if row % 2 == 0 { 0.06 } else { -0.06 };
        for i in 0..n {
            let x = -1.8 + i as f32 * (3.6 / (n as f32 - 1.0)) + off;
            prims.push(prim(
                cuboid_tapered([0.17, 0.6, 0.05], 0.55, timber(FISH_GREY)),
                [x, ry - 0.3, z],
                id_quat(),
            ));
        }
    }

    // A hung strip of homespun cloth at one end.
    prims.push(prim(
        cuboid_tapered([0.7, 1.1, 0.05], 0.0, cloth(SHIELD_CREAM, WOOD_WARM)),
        [1.9, top_y - 0.75, -0.12],
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
