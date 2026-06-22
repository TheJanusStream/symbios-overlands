//! Sod shelter — a Nordic *poor* secondary. A crude lean-to: a low turf back
//! wall and a sloping sod roof propped on two bowed poles, open toward the
//! shore (-Z) with a little stacked firewood and a barrel kept dry beneath.
//! The kind of windbreak a croft throws up beside the
//! [`turf_house`](super::turf_house).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_COLD, TURF_GREEN, WOOD_DARK, WOOD_WARM, rough_stone, timber, turf};

pub struct SodShelter;

impl CatalogueEntry for SodShelter {
    fn slug(&self) -> &'static str {
        "sod_shelter"
    }
    fn name(&self) -> &'static str {
        "Sod Shelter"
    }
    fn description(&self) -> &'static str {
        "Crude turf-roofed lean-to on bowed poles, open to the weather."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_POOR
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
                rough_stone(STONE_COLD),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Low turf back wall (+Z, away from the shore).
        prim(
            solid(cuboid_tapered([4.0, 1.9, 0.8], 0.08, turf(TURF_GREEN))),
            [0.0, foot_h + 0.95, 1.1],
            id_quat(),
        ),
    ];

    // Two bowed front poles (-Z) with a tie-beam across.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, 1.1, 7, 0.0, timber(WOOD_DARK))),
            [sx * 1.7, foot_h + 0.55, -1.2],
            quat_x(-0.12),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.14, 0.14], 0.0, timber(WOOD_DARK))),
        [0.0, front_top, -1.2],
        id_quat(),
    ));
    // Two rafter poles running back-to-front under the roof.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.07, 2.8, 6, 0.0, timber(WOOD_WARM))),
            [sx * 1.4, (back_top + front_top) * 0.5 + 0.05, -0.1],
            quat_x(std::f32::consts::FRAC_PI_2 - 0.3),
        ));
    }

    // Sloping turf roof from the high back wall (+Z) down to the front poles
    // (-Z) so the open side faces the camera.
    let mid_y = (back_top + front_top) * 0.5;
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.4, 3.4], 0.05, turf(TURF_GREEN))),
        [0.0, mid_y + 0.2, 0.0],
        quat_x(-0.3),
    ));

    // Stored under the lean-to: a stack of firewood and a barrel kept dry.
    for r in 0..2 {
        for c in 0..3 {
            let tone = if (r + c) % 2 == 0 {
                WOOD_WARM
            } else {
                WOOD_DARK
            };
            prims.push(prim(
                solid(cylinder_tapered(0.12, 0.7, 8, 0.0, timber(tone))),
                [
                    -1.4 + c as f32 * 0.28,
                    foot_h + 0.13 + r as f32 * 0.24,
                    -0.5,
                ],
                quat_x(std::f32::consts::FRAC_PI_2),
            ));
        }
    }
    prims.push(prim(
        solid(cylinder_tapered(0.32, 0.8, 10, 0.12, timber(WOOD_WARM))),
        [1.3, foot_h + 0.4, -0.4],
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
        assert_sanitize_stable(&SodShelter.build(""), "sod_shelter");
    }
}
