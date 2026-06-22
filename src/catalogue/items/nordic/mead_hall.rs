//! Mead hall — the Nordic landmark. A long timber-staved hall on a dressed
//! stone footing under a steep thatched gable roof, ridged end-to-end and
//! crowned at each gable by a carved dragon-head finial on crossed
//! bargeboards. A roof louver breathes hearth smoke and the great timbers
//! moan in the wind; the long shore-facing wall carries the carved door and
//! firelit windows. ~22 m long, so it anchors the steading and reads as the
//! chieftain's seat from across the home region.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the stone footing.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DRAGON_EYE, FIRE_ORANGE, STONE_GREY, THATCH_STRAW, WOOD_DARK, WOOD_WARM, dragon_head, fx,
    gable_roof, stone, thatch, timber,
};

/// Warm window light, a touch deeper than the hearth glow.
const HALL_LIGHT: [f32; 3] = [1.0, 0.66, 0.28];

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
    let l = 22.0_f32; // length (X) — ridge runs this way
    let w = 8.0_f32; // width (Z)
    let foot_h = 0.6;
    let wall_h = 3.6;
    let wall_top = foot_h + wall_h;
    let roof_h = 5.0; // steep thatch
    let ridge_y = wall_top + roof_h;
    let eave = 1.0; // roof overhang each side

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

    // Long timber stave walls (front -Z + back +Z).
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([l, wall_h, 0.4], 0.0, timber(WOOD_WARM))),
            [0.0, foot_h + wall_h * 0.5, sz * (w * 0.5 - 0.2)],
            id_quat(),
        ));
    }
    // Gable end walls, carried up into the roof triangle so no daylight
    // shows under the thatch.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.4, wall_h, w], 0.0, timber(WOOD_DARK))),
            [sx * (l * 0.5 - 0.2), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
        // Triangular timber gable infill above the eave (thin in X, pinched
        // in Z to the ridge) — the stave-built gable face.
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [0.4, roof_h, w],
                [0.0, 0.94],
                timber(WOOD_DARK),
            )),
            [sx * (l * 0.5 - 0.2), wall_top + roof_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Vertical stave posts proud of each long wall — the timber rhythm of a
    // stave hall.
    for sz in [-1.0_f32, 1.0] {
        for k in 0..7 {
            let x = -l * 0.5 + 1.6 + k as f32 * ((l - 3.2) / 6.0);
            prims.push(prim(
                solid(cuboid_tapered([0.34, wall_h, 0.22], 0.0, timber(WOOD_DARK))),
                [x, foot_h + wall_h * 0.5, sz * (w * 0.5 + 0.02)],
                id_quat(),
            ));
        }
    }

    // Steep thatched gable roof.
    prims.push(gable_roof(
        [l + 2.0, roof_h, w + 2.0 * eave],
        [0.0, wall_top + roof_h * 0.5, 0.0],
        thatch(THATCH_STRAW),
    ));
    // Ridge beam running the length of the roof, overhanging the gables.
    prims.push(prim(
        solid(cuboid_tapered([l + 2.4, 0.42, 0.5], 0.0, timber(WOOD_DARK))),
        [0.0, ridge_y - 0.15, 0.0],
        id_quat(),
    ));

    // Crossed gable bargeboards rising into carved dragon heads at each
    // apex — the hall's signature crown.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * (l * 0.5 + 0.1);
        // Two boards crossing at the peak (an X over the gable).
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.26, roof_h * 0.95, 0.3],
                    0.0,
                    timber(WOOD_WARM),
                )),
                [x, wall_top + roof_h * 0.55, sz * 0.0],
                quat_x(sz * (w * 0.5 / roof_h).atan()),
            ));
        }
        // Dragon head springing outward off the apex.
        let yaw = if sx > 0.0 { 0.0 } else { std::f32::consts::PI };
        prims.push(dragon_head(
            [x + sx * 0.25, ridge_y - 0.1, 0.0],
            1.05,
            yaw,
            WOOD_WARM,
            DRAGON_EYE,
        ));
    }

    // Roof smoke louver, off-centre over the hearth, venting woodsmoke — a
    // raised timber lantern.
    let louver_x = -4.0;
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.8, 1.2], 0.25, timber(WOOD_DARK))),
        [louver_x, ridge_y - 0.1, 0.0],
        id_quat(),
    ));

    // Shore-facing (-Z) entrance: a carved door under a small gabled porch,
    // with a warm hearth glow spilling out.
    let zf = -(w * 0.5 - 0.05);
    prims.push(prim(
        solid(cuboid_tapered([2.0, 2.8, 0.3], 0.0, timber(WOOD_DARK))),
        [0.0, foot_h + 1.4, zf - 0.08],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.5, 2.2, 0.2], 0.0, glow(FIRE_ORANGE, 2.4)),
        [0.0, foot_h + 1.2, zf - 0.2],
        id_quat(),
    ));
    // Porch posts + a little gabled hood over the door (kept shallow so it
    // does not occlude the wall from the elevated camera).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.22, 2.7, 0.22], 0.0, timber(WOOD_WARM))),
            [sx * 1.3, foot_h + 1.35, zf - 1.2],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [3.2, 1.0, 1.6],
            [0.6, 0.0],
            thatch(THATCH_STRAW),
        )),
        [0.0, wall_top - 0.4, zf - 0.6],
        id_quat(),
    ));

    // Firelit shuttered windows flanking the door along the -Z wall.
    for sx in [-1.0_f32, 1.0] {
        for &dx in &[3.6_f32, 7.2] {
            prims.push(prim(
                solid(cuboid_tapered([1.0, 1.1, 0.18], 0.0, timber(WOOD_DARK))),
                [sx * dx, foot_h + 2.4, zf + 0.04],
                id_quat(),
            ));
            prims.push(prim(
                cuboid_tapered([0.66, 0.78, 0.1], 0.0, glow(HALL_LIGHT, 2.0)),
                [sx * dx, foot_h + 2.4, zf - 0.06],
                id_quat(),
            ));
        }
    }

    // Leaning buttress timbers along the long walls.
    for sz in [-1.0_f32, 1.0] {
        for k in 0..3 {
            let x = -l * 0.35 + (k as f32) * (l * 0.35);
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.32, wall_h + 1.0, 0.32],
                    0.0,
                    timber(WOOD_DARK),
                )),
                [x, foot_h + (wall_h + 1.0) * 0.5, sz * (w * 0.5 + 0.3)],
                quat_x(sz * 0.12),
            ));
        }
    }

    let mut root = assemble(prims);
    // Signature life: woodsmoke from the louver, the hall's low wind moan.
    root.children.push(fx::hearth_smoke(
        [louver_x, ridge_y + 0.5, 0.0],
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
