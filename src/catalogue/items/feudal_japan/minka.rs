//! Minka — the Feudal-Japan *poor* landmark. A timber-framed farmhouse with
//! plaster-daub walls under a great steep thatched roof, hearth smoke
//! seeping through the ridge. The farmstead counterpart to the lacquered
//! [`pagoda`](super::pagoda): same theme, opposite end of the prosperity
//! axis (`Poor`), so a destitute room grows this instead of the temple.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    PLASTER_WHITE, STONE_GREY, THATCH_STRAW, TIMBER_DARK, fx, plaster, stone, thatch, timber,
};

pub struct Minka;

impl CatalogueEntry for Minka {
    fn slug(&self) -> &'static str {
        "minka"
    }
    fn name(&self) -> &'static str {
        "Minka Farmhouse"
    }
    fn description(&self) -> &'static str {
        "Timber-framed farmhouse with daub walls under a great steep thatch roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_POOR
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
    let l = 10.0_f32;
    let w = 7.0_f32;
    let foot_h = 0.4;
    let wall_h = 2.6;
    let wall_top = foot_h + wall_h;
    let roof_h = 3.6;

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, foot_h, w + 0.6],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Plaster-daub long walls and gable ends.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [l, wall_h, 0.3],
                0.0,
                plaster(PLASTER_WHITE),
            )),
            [0.0, foot_h + wall_h * 0.5, sz * (w * 0.5 - 0.15)],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.3, wall_h, w],
                0.0,
                plaster(PLASTER_WHITE),
            )),
            [sx * (l * 0.5 - 0.15), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Exposed timber corner posts.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.35, wall_h, 0.35],
                0.0,
                timber(TIMBER_DARK),
            )),
            [
                sx * (l * 0.5 - 0.2),
                foot_h + wall_h * 0.5,
                sz * (w * 0.5 - 0.2),
            ],
            id_quat(),
        ));
    }

    // Timber door in the near gable.
    prims.push(prim(
        solid(cuboid_tapered([0.45, 1.9, 1.5], 0.0, timber(TIMBER_DARK))),
        [l * 0.5 - 0.1, foot_h + 0.95, 0.0],
        id_quat(),
    ));

    // Great steep thatched hip roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.6, roof_h, w + 1.8],
            0.5,
            thatch(THATCH_STRAW),
        )),
        [0.0, wall_top + roof_h * 0.5, 0.0],
        id_quat(),
    ));
    // Ridge cap.
    let ridge_x = 2.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [l - 1.0, 0.5, 0.8],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, wall_top + roof_h - 0.1, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: hearth smoke seeping from the ridge.
    root.children.push(fx::hearth_smoke(
        [ridge_x, wall_top + roof_h + 0.2, 0.0],
        0x70F0_CE11,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Minka.build(""), "minka");
    }
}
