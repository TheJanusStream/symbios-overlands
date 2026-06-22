//! Lean-to — a Medieval *poor* secondary. A crude open shelter: a low daub
//! back wall on a fieldstone footing, a thatch roof on bowed poles and
//! exposed rafters sloping down to the open −Z front, and a little store of
//! firewood, a pail and a rough bench tucked under it. The kind of windbreak
//! a cottar throws up beside the [`wattle_hovel`](super::wattle_hovel).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DAUB_CREAM, STONE_GREY, THATCH_STRAW, WOOD_DARK, WOOD_OAK, daub, rough_stone, thatch, timber,
};

pub struct LeanTo;

impl CatalogueEntry for LeanTo {
    fn slug(&self) -> &'static str {
        "lean_to"
    }
    fn name(&self) -> &'static str {
        "Lean-To"
    }
    fn description(&self) -> &'static str {
        "Crude thatch-roofed lean-to on bowed poles over a daub back wall, open to the weather."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let foot_h = 0.3;
    let back_z = 1.1; // back wall at +Z; shelter opens toward −Z (camera)

    let mut prims = vec![
        // Fieldstone footing — the root.
        prim(
            solid(cuboid_tapered(
                [4.2, foot_h, 3.2],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Low daub back wall.
        prim(
            solid(cuboid_tapered([4.0, 1.9, 0.6], 0.06, daub(DAUB_CREAM))),
            [0.0, foot_h + 0.95, back_z],
            id_quat(),
        ),
    ];

    // Two bowed front poles (short, at the open −Z eave) + back uprights.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, 1.2, 7, 0.0, timber(WOOD_DARK))),
            [sx * 1.7, foot_h + 0.6, -1.2],
            quat_x(-0.12),
        ));
    }

    // Sloping thatch roof: high at the back (+Z), low at the open front (−Z).
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.5, 3.5], 0.05, thatch(THATCH_STRAW))),
        [0.0, foot_h + 1.7, 0.0],
        quat_x(-0.34),
    ));
    // Exposed rafters under the thatch, following the slope.
    for sx in [-1.4_f32, 0.0, 1.4] {
        prims.push(prim(
            solid(cuboid_tapered([0.09, 0.09, 3.3], 0.0, timber(WOOD_DARK))),
            [sx, foot_h + 1.5, 0.0],
            quat_x(-0.34),
        ));
    }

    // Store of firewood: a few split logs lying along X, end-grain out.
    for (dy, dz) in [(0.16_f32, 0.55_f32), (0.16, 0.85), (0.42, 0.7)] {
        prims.push(prim(
            solid(cylinder_tapered(0.12, 1.6, 7, 0.0, timber(WOOD_OAK))),
            [0.9, foot_h + dy, dz],
            crate::catalogue::items::util::quat_z(FRAC_PI_2),
        ));
    }
    // A pail.
    prims.push(prim(
        solid(cylinder_tapered(0.18, 0.34, 10, 0.1, timber(WOOD_OAK))),
        [-1.3, foot_h + 0.17, -0.2],
        id_quat(),
    ));
    // A rough plank bench against the back wall.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.1, 0.4], 0.0, timber(WOOD_DARK))),
        [-0.6, foot_h + 0.5, 0.6],
        id_quat(),
    ));
    for sx in [-1.2_f32, 0.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.5, 0.3], 0.0, timber(WOOD_DARK))),
            [-0.6 + sx, foot_h + 0.25, 0.6],
            id_quat(),
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
        assert_sanitize_stable(&LeanTo.build(""), "lean_to");
    }
}
