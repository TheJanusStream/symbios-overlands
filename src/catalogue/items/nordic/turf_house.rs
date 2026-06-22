//! Turf house — the Nordic *poor* landmark. A low longhouse dug into the
//! cold ground, its thick walls and roof built up from stacked sod over a
//! fieldstone footing, with a single timber-framed door on the shore-facing
//! wall and a smoke hole breathing peat smoke. The croft counterpart to the
//! carved [`mead_hall`](super::mead_hall): same theme, opposite end of the
//! prosperity axis (`Poor`), so a destitute Nordic room grows this instead.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, STONE_COLD, TURF_GREEN, WOOD_DARK, fx, gable_roof, rough_stone, timber, turf,
};

pub struct TurfHouse;

impl CatalogueEntry for TurfHouse {
    fn slug(&self) -> &'static str {
        "turf_house"
    }
    fn name(&self) -> &'static str {
        "Turf House"
    }
    fn description(&self) -> &'static str {
        "Low sod-walled longhouse over a fieldstone footing, smoke seeping from its roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 12.0_f32;
    let w = 6.0_f32;
    let foot_h = 0.4;
    let wall_h = 2.2;
    let wall_top = foot_h + wall_h;
    let roof_h = 2.4; // low, but a real ridge

    let mut prims = vec![
        // Fieldstone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 1.0, foot_h, w + 1.0],
                0.0,
                rough_stone(STONE_COLD),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Thick sod side walls.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([l, wall_h, 1.1], 0.1, turf(TURF_GREEN))),
            [0.0, foot_h + wall_h * 0.5, sz * (w * 0.5 - 0.4)],
            id_quat(),
        ));
    }
    // Sod gable ends, carried up into the roof triangle.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.1, wall_h, w], 0.1, turf(TURF_GREEN))),
            [sx * (l * 0.5 - 0.4), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [1.1, roof_h, w],
                [0.0, 0.94],
                turf(TURF_GREEN),
            )),
            [sx * (l * 0.5 - 0.4), wall_top + roof_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Timber-framed door on the shore-facing (-Z) long wall, with a dim peat
    // ember glow within.
    let zf = -(w * 0.5 - 0.05);
    prims.push(prim(
        solid(cuboid_tapered([1.6, 1.9, 0.3], 0.0, timber(WOOD_DARK))),
        [0.0, foot_h + 0.95, zf - 0.05],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.0, 1.3, 0.18], 0.0, glow(FIRE_ORANGE, 1.6)),
        [0.0, foot_h + 0.8, zf - 0.18],
        id_quat(),
    ));

    // Low turf gable roof.
    prims.push(gable_roof(
        [l + 1.4, roof_h, w + 1.4],
        [0.0, wall_top + roof_h * 0.5, 0.0],
        turf(TURF_GREEN),
    ));
    // Sod ridge cap.
    prims.push(prim(
        solid(cuboid_tapered([l + 1.0, 0.35, 0.7], 0.2, turf(TURF_GREEN))),
        [0.0, wall_top + roof_h - 0.1, 0.0],
        id_quat(),
    ));

    // Timber smoke-hole curb near the ridge, off to one end.
    let hole_x = 3.2;
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.5, 1.0], 0.2, timber(WOOD_DARK))),
        [hole_x, wall_top + roof_h - 0.2, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: peat smoke seeping from the roof hole.
    root.children.push(fx::hearth_smoke(
        [hole_x, wall_top + roof_h + 0.3, 0.0],
        0x70F0_DA11,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TurfHouse.build(""), "turf_house");
    }
}
