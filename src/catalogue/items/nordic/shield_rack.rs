//! Shield rack — a Nordic prop. A timber rail between two posts hung with
//! painted round shields and a couple of leaning spears: the wall of arms
//! outside a warrior's door.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    IRON_DARK, SHIELD_BLUE, SHIELD_GOLD, SHIELD_RED, WOOD_DARK, WOOD_WARM, iron, round_shield,
    timber,
};

pub struct ShieldRack;

impl CatalogueEntry for ShieldRack {
    fn slug(&self) -> &'static str {
        "shield_rack"
    }
    fn name(&self) -> &'static str {
        "Shield Rack"
    }
    fn description(&self) -> &'static str {
        "Timber rail hung with painted round shields and leaning spears."
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
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let rail_y = 2.1;

    let mut prims = vec![
        // Ground sill — the root.
        prim(
            solid(cuboid_tapered([3.2, 0.2, 0.4], 0.0, timber(WOOD_DARK))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];
    // Two posts.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.12, rail_y, 8, 0.05, timber(WOOD_WARM))),
            [sx * 1.4, rail_y * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Top rail the shields hang from.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.18, 0.18], 0.0, timber(WOOD_DARK))),
        [0.0, rail_y, 0.0],
        id_quat(),
    ));

    // Three shields facing front.
    let palette = [SHIELD_RED, SHIELD_GOLD, SHIELD_BLUE];
    for (i, face) in palette.iter().enumerate() {
        let x = -0.95 + i as f32 * 0.95;
        prims.push(round_shield(
            [x, rail_y - 0.7, 0.18],
            quat_x(FRAC_PI_2),
            *face,
            IRON_DARK,
        ));
    }

    // A pair of spears leaning against the rail.
    for (sx, lean) in [(-1.0_f32, 0.12_f32), (1.0, -0.1)] {
        prims.push(prim(
            solid(cylinder_tapered(0.04, 2.6, 6, 0.0, timber(WOOD_WARM))),
            [sx * 1.5, 1.3, 0.25],
            quat_x(lean),
        ));
        prims.push(prim(
            cone(0.06, 0.35, 6, iron(IRON_DARK)),
            [sx * 1.5, 2.65, 0.25 + lean * 1.3],
            quat_x(lean),
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
        assert_sanitize_stable(&ShieldRack.build(""), "shield_rack");
    }
}
