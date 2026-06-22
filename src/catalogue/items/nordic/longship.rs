//! Longship — a Nordic prop, and the steading's proudest one. A beached
//! clinker-hulled raider: a rounded lapstrake hull on a projecting keel,
//! high curved stems carved into dragon heads fore and aft, a row of painted
//! shields slung along each gunwale, oars fanned out below them, and a
//! striped square sail bent to the yard. Larger than the usual scatter
//! clutter, it reads as the crew's ship drawn up on the shingle.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DRAGON_EYE, IRON_DARK, SHIELD_BLUE, SHIELD_CREAM, SHIELD_GOLD, SHIELD_RED, WOOD_DARK,
    WOOD_WARM, cloth, dragon_head, round_shield, timber,
};

pub struct Longship;

impl CatalogueEntry for Longship {
    fn slug(&self) -> &'static str {
        "longship"
    }
    fn name(&self) -> &'static str {
        "Longship"
    }
    fn description(&self) -> &'static str {
        "Beached clinker-built longship with a dragon prow and shielded gunwales."
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
            clearance: 5.0,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let half_w = 1.0_f32; // hull half-beam
    let hull_len = 9.4_f32;
    let bow = hull_len * 0.5;
    let keel_y = 0.32;
    let hull_y = 0.95; // hull-shell axis height
    let gunwale = 1.55; // top rail height

    let mut prims = vec![
        // Projecting keel plank — the flat id-rotation root (the rounded
        // hull shell that rides on it is a rotated child, so the root never
        // carries a rotation).
        prim(
            solid(cuboid_tapered(
                [hull_len + 1.2, 0.34, 0.42],
                0.7,
                timber(WOOD_DARK),
            )),
            [0.0, keel_y, 0.0],
            id_quat(),
        ),
    ];

    // Rounded lapstrake hull shell — a half-cylinder, round side down, laid
    // along X (axis Y -> X via quat_z). path_cut [0,0.5] keeps the lower
    // half so the curved bottom shows and the deck reads open on top.
    prims.push(prim(
        solid(with_cut(
            cylinder_tapered(half_w, hull_len, 18, 0.0, timber(WOOD_WARM)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, hull_y, 0.0],
        quat_z(FRAC_PI_2),
    ));

    // Clinker strakes — overlapping horizontal battens up each flank,
    // tapered toward the ends, alternating tone for the lapstrake shadow
    // line.
    for sz in [-1.0_f32, 1.0] {
        for (k, &(sy, sw)) in [(0.55_f32, 0.96_f32), (0.9, 0.99), (1.25, 0.92)]
            .iter()
            .enumerate()
        {
            let tone = if k % 2 == 0 { WOOD_DARK } else { WOOD_WARM };
            prims.push(prim(
                solid(cuboid_tapered(
                    [hull_len - 0.6, 0.13, 0.12],
                    0.6,
                    timber(tone),
                )),
                [0.0, sy, sz * sw],
                id_quat(),
            ));
        }
    }

    // Gunwale top rails.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [hull_len - 0.2, 0.16, 0.18],
                0.4,
                timber(WOOD_DARK),
            )),
            [0.0, gunwale, sz * (half_w * 0.9)],
            id_quat(),
        ));
    }

    // Curved dragon stems fore (+X, tall) and aft (-X, shorter). Each is a
    // two-segment stempost sweeping up and inward, capped by a carved head
    // facing outward.
    {
        // Prow.
        prims.push(prim(
            solid(cuboid_tapered([0.34, 1.9, 0.42], 0.2, timber(WOOD_WARM))),
            [bow - 0.15, 1.55, 0.0],
            quat_z(0.2),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.3, 1.7, 0.38], 0.3, timber(WOOD_WARM))),
            [bow - 0.55, 2.95, 0.0],
            quat_z(0.42),
        ));
        prims.push(dragon_head(
            [bow - 0.75, 3.55, 0.0],
            0.95,
            0.0,
            WOOD_WARM,
            DRAGON_EYE,
        ));
        // Stern.
        prims.push(prim(
            solid(cuboid_tapered([0.32, 1.5, 0.4], 0.2, timber(WOOD_WARM))),
            [-bow + 0.15, 1.4, 0.0],
            quat_z(-0.22),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.28, 1.3, 0.36], 0.3, timber(WOOD_WARM))),
            [-bow + 0.5, 2.5, 0.0],
            quat_z(-0.46),
        ));
        prims.push(dragon_head(
            [-bow + 0.7, 3.0, 0.0],
            0.78,
            std::f32::consts::PI,
            WOOD_WARM,
            DRAGON_EYE,
        ));
    }

    // Oars fanned out below the shield line, four to a side, angled down and
    // out through the oar-ports.
    for sz in [-1.0_f32, 1.0] {
        for i in 0..4 {
            let x = -2.7 + i as f32 * 1.8;
            prims.push(prim(
                solid(cylinder_tapered(0.05, 2.4, 6, 0.1, timber(WOOD_DARK))),
                [x, 0.55, sz * 1.6],
                quat_x(sz * 1.25),
            ));
        }
    }

    // Painted shields slung along each gunwale, facing outward.
    let palette = [
        SHIELD_RED,
        SHIELD_BLUE,
        SHIELD_GOLD,
        SHIELD_RED,
        SHIELD_BLUE,
    ];
    for (i, face) in palette.iter().enumerate() {
        let x = -3.4 + i as f32 * 1.7;
        prims.push(round_shield(
            [x, gunwale - 0.35, half_w + 0.12],
            quat_x(FRAC_PI_2),
            *face,
            IRON_DARK,
        ));
        prims.push(round_shield(
            [x, gunwale - 0.35, -(half_w + 0.12)],
            quat_x(-FRAC_PI_2),
            palette[(i + 2) % palette.len()],
            IRON_DARK,
        ));
    }

    // Mast with a striped square sail bent to the yard.
    let mast_h = 5.4;
    prims.push(prim(
        solid(cylinder_tapered(0.17, mast_h, 8, 0.12, timber(WOOD_WARM))),
        [0.0, hull_y + mast_h * 0.5, 0.0],
        id_quat(),
    ));
    let yard_y = hull_y + mast_h - 0.5;
    prims.push(prim(
        solid(cuboid_tapered([4.6, 0.2, 0.2], 0.0, timber(WOOD_DARK))),
        [0.0, yard_y, 0.0],
        id_quat(),
    ));
    // Five alternating red/cream vertical stripes hanging from the yard.
    for i in 0..5 {
        let x = -1.6 + i as f32 * 0.8;
        let (warp, weft) = if i % 2 == 0 {
            (SHIELD_RED, [0.7, 0.2, 0.16])
        } else {
            (SHIELD_CREAM, [0.62, 0.56, 0.42])
        };
        prims.push(prim(
            cuboid_tapered([0.8, 3.0, 0.08], 0.0, cloth(warp, weft)),
            [x, yard_y - 1.6, 0.0],
            id_quat(),
        ));
    }
    // Forestay / backstay lines from the masthead down to the stems.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.025, 5.4, 5, 0.0, timber(WOOD_DARK))),
            [sx * 2.2, hull_y + mast_h * 0.55, 0.0],
            quat_z(sx * 0.7),
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
        assert_sanitize_stable(&Longship.build(""), "longship");
    }
}
