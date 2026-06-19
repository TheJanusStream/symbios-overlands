//! Lean-to — a Medieval *poor* secondary. A crude open shelter: a low
//! daub back wall on a fieldstone footing and a sloping thatch roof propped
//! on two bowed poles, open to the weather. The kind of windbreak a cottar
//! throws up beside the [`wattle_hovel`](super::wattle_hovel).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DAUB_CREAM, STONE_GREY, THATCH_STRAW, WOOD_DARK, daub, rough_stone, thatch, timber};

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
    let back_top = foot_h + 1.9;
    let front_top = foot_h + 1.1;

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
            [0.0, foot_h + 0.95, -1.1],
            id_quat(),
        ),
    ];

    // Two bowed front poles.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, 1.1, 7, 0.0, timber(WOOD_DARK))),
            [sx * 1.7, foot_h + 0.55, 1.2],
            quat_x(0.12),
        ));
    }

    // Sloping thatch roof from the back wall down to the front poles.
    let mid_y = (back_top + front_top) * 0.5;
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.4, 3.4], 0.05, thatch(THATCH_STRAW))),
        [0.0, mid_y + 0.2, 0.0],
        quat_x(0.3),
    ));

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
