//! Mead hall — the Nordic landmark. A long timber-staved hall on a dressed
//! stone footing under a steep thatched hip roof, ridged end-to-end and
//! crowned at each gable by a carved dragon-head finial. A roof louver
//! breathes hearth smoke and the great timbers moan in the wind. ~22 m
//! long, so it anchors the steading and reads as the chieftain's seat from
//! across the home region.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the stone footing.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, STONE_GREY, THATCH_STRAW, WOOD_DARK, WOOD_WARM, fx, stone, thatch, timber,
};

pub struct MeadHall;

impl CatalogueEntry for MeadHall {
    fn slug(&self) -> &'static str {
        "mead_hall"
    }
    fn name(&self) -> &'static str {
        "Mead Hall"
    }
    fn description(&self) -> &'static str {
        "Long timber hall under a steep thatch roof, gables carved with dragon heads."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 16.0,
            min_spawn_dist: 55.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 22.0_f32; // length (X)
    let w = 8.0_f32; // width (Z)
    let foot_h = 0.6;
    let wall_h = 4.0;
    let wall_top = foot_h + wall_h;
    let roof_h = 3.4;
    let ridge_y = wall_top + roof_h;

    let mut prims = vec![
        // Dressed stone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 1.5, foot_h, w + 1.5],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Long timber walls (front + back).
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([l, wall_h, 0.35], 0.0, timber(WOOD_WARM))),
            [0.0, foot_h + wall_h * 0.5, sz * (w * 0.5 - 0.18)],
            id_quat(),
        ));
    }
    // Gable end walls.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.35, wall_h, w], 0.0, timber(WOOD_DARK))),
            [sx * (l * 0.5 - 0.18), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Steep thatched hip roof — one tapered block, no rotation needed.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 2.0, roof_h, w + 2.0],
            0.55,
            thatch(THATCH_STRAW),
        )),
        [0.0, wall_top + roof_h * 0.5, 0.0],
        id_quat(),
    ));
    // Ridge beam running the length of the roof.
    prims.push(prim(
        solid(cuboid_tapered([l + 1.0, 0.4, 0.6], 0.0, timber(WOOD_DARK))),
        [0.0, ridge_y - 0.1, 0.0],
        id_quat(),
    ));

    // Carved dragon-head finials at each gable apex: a leaning neck post
    // capped by a tapered head and snout.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * (l * 0.5 + 0.2);
        prims.push(prim(
            solid(cylinder_tapered(0.18, 2.4, 8, 0.2, timber(WOOD_DARK))),
            [x, ridge_y + 0.6, 0.0],
            quat_x(0.0),
        ));
        // Head — a stout tapered block leaning outward.
        prims.push(prim(
            solid(cuboid_tapered([0.7, 0.9, 0.5], 0.3, timber(WOOD_WARM))),
            [x + sx * 0.3, ridge_y + 1.9, 0.0],
            quat_x(0.4 * sx),
        ));
        // Snout — a forward cone.
        prims.push(prim(
            cone(0.22, 0.7, 7, timber(WOOD_WARM)),
            [x + sx * 0.7, ridge_y + 2.2, 0.0],
            quat_x(1.57 * sx),
        ));
    }

    // Roof smoke louver, off-centre over the hearth, venting woodsmoke.
    let louver_x = -4.0;
    prims.push(prim(
        solid(cuboid_tapered([1.4, 0.7, 1.4], 0.2, timber(WOOD_DARK))),
        [louver_x, ridge_y + 0.2, 0.0],
        id_quat(),
    ));

    // Recessed door at the near gable, with a warm hearth glow within.
    prims.push(prim(
        solid(cuboid_tapered([0.4, 2.6, 1.8], 0.0, timber(WOOD_DARK))),
        [l * 0.5 - 0.1, foot_h + 1.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.3, 0.8, 1.0], 0.0, glow(FIRE_ORANGE, 2.2)),
        [l * 0.5 - 0.55, foot_h + 0.9, 0.0],
        id_quat(),
    ));

    // Leaning buttress timbers along the long walls.
    for sz in [-1.0_f32, 1.0] {
        for k in 0..3 {
            let x = -l * 0.35 + (k as f32) * (l * 0.35);
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.3, wall_h + 1.0, 0.3],
                    0.0,
                    timber(WOOD_DARK),
                )),
                [x, foot_h + (wall_h + 1.0) * 0.5, sz * (w * 0.5 + 0.25)],
                quat_x(sz * 0.12),
            ));
        }
    }

    let mut root = assemble(prims);
    // Signature life: woodsmoke from the louver, the hall's low wind moan.
    root.children.push(fx::hearth_smoke(
        [louver_x, ridge_y + 0.7, 0.0],
        0x4EAD_DA11,
    ));
    root.audio = fx::wind_moan();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&MeadHall.build(""), "mead_hall");
    }
}
