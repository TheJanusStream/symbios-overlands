//! Signal beacon — a Nordic secondary and the kit's firelit hero. A braced
//! timber lattice tower on a fieldstone base hoists an iron fire-basket of
//! burning logs high enough to be seen across the fjord: the warning-fire
//! chain that mustered the fleet. The cage of iron bars lets the blaze show
//! through; leaping flame, drifting embers, and a fire crackle bring it
//! alive. Its emissive core is the trim escalation's ruin pass snuffs to a
//! cold dead brazier.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_z, solid, sphere,
    torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, IRON_DARK, STONE_COLD, WOOD_DARK, WOOD_WARM, fx, iron, rough_stone, timber,
};

pub struct SignalBeacon;

impl CatalogueEntry for SignalBeacon {
    fn slug(&self) -> &'static str {
        "signal_beacon"
    }
    fn name(&self) -> &'static str {
        "Signal Beacon"
    }
    fn description(&self) -> &'static str {
        "Braced timber lattice tower carrying an iron fire-basket, burning as a warning fire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6;
    let post_h = 5.0;
    let deck_y = base_h + post_h;
    let r = 0.95_f32; // corner-post half-spacing

    let mut prims = vec![
        // Fieldstone base — the root.
        prim(
            solid(cuboid_tapered(
                [3.0, base_h, 3.0],
                0.1,
                rough_stone(STONE_COLD),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Central mast.
        prim(
            solid(cylinder_tapered(0.26, post_h, 8, 0.15, timber(WOOD_WARM))),
            [0.0, base_h + post_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Four corner posts.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.14,
                post_h - 0.3,
                8,
                0.0,
                timber(WOOD_DARK),
            )),
            [sx * r, base_h + (post_h - 0.3) * 0.5, sz * r],
            id_quat(),
        ));
    }

    // Cross-braced lattice: an X on every face plus a mid girt ring, so the
    // tower reads as a braced beacon frame, not a bare stool.
    let span = post_h - 0.6;
    let theta = (span / (2.0 * r)).atan();
    let dlen = (span * span + (2.0 * r) * (2.0 * r)).sqrt();
    let mid_y = base_h + post_h * 0.5;
    for sx in [-1.0_f32, 1.0] {
        // ±X faces — diagonals run in Z, rotated about X.
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.1, dlen], 0.0, timber(WOOD_DARK))),
            [sx * r, mid_y, 0.0],
            quat_x(theta),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.1, dlen], 0.0, timber(WOOD_DARK))),
            [sx * r, mid_y, 0.0],
            quat_x(-theta),
        ));
    }
    for sz in [-1.0_f32, 1.0] {
        // ±Z faces — diagonals run in X, rotated about Z.
        prims.push(prim(
            solid(cuboid_tapered([dlen, 0.1, 0.1], 0.0, timber(WOOD_DARK))),
            [0.0, mid_y, sz * r],
            quat_z(theta),
        ));
        prims.push(prim(
            solid(cuboid_tapered([dlen, 0.1, 0.1], 0.0, timber(WOOD_DARK))),
            [0.0, mid_y, sz * r],
            quat_z(-theta),
        ));
    }
    // Top girt ring tying the post heads.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, 0.12, 2.0 * r],
                0.0,
                timber(WOOD_DARK),
            )),
            [sx * r, base_h + post_h - 0.5, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [2.0 * r, 0.12, 0.12],
                0.0,
                timber(WOOD_DARK),
            )),
            [0.0, base_h + post_h - 0.5, sz * r],
            id_quat(),
        ));
    }

    // Timber deck under the brazier.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 0.3, 2.4], 0.0, timber(WOOD_WARM))),
        [0.0, deck_y + 0.15, 0.0],
        id_quat(),
    ));

    // Iron fire-basket: a shallow floor disc, a cage of upright bars, and two
    // hoop rings — open so the blaze shows between the bars.
    let basket_y = deck_y + 0.55;
    prims.push(prim(
        solid(cylinder_tapered(0.85, 0.12, 12, 0.0, iron(IRON_DARK))),
        [0.0, deck_y + 0.36, 0.0],
        id_quat(),
    ));
    for k in 0..10 {
        let a = k as f32 / 10.0 * TAU;
        prims.push(prim(
            solid(cylinder_tapered(0.05, 0.85, 5, 0.0, iron(IRON_DARK))),
            [0.82 * a.cos(), basket_y, 0.82 * a.sin()],
            quat_z(0.18 * a.cos()), // bars splay slightly outward
        ));
    }
    for hy in [deck_y + 0.42, deck_y + 0.92] {
        prims.push(prim(
            torus(0.06, 0.86, iron(IRON_DARK)),
            [0.0, hy, 0.0],
            id_quat(),
        ));
    }

    // Glowing fire core inside the basket — the emissive heart, crackling.
    let mut fire = prim(
        sphere(0.62, 6, glow(FIRE_ORANGE, 6.5)),
        [0.0, basket_y + 0.05, 0.0],
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    prims.push(fire);
    // A couple of charred logs jutting from the blaze.
    for (sx, sz, lean) in [(1.0_f32, 0.4_f32, 0.5_f32), (-0.7, -0.5, -0.4)] {
        prims.push(prim(
            solid(cylinder_tapered(0.07, 0.9, 6, 0.1, timber(WOOD_DARK))),
            [sx * 0.3, basket_y + 0.2, sz * 0.3],
            quat_z(lean),
        ));
    }

    let flame_y = deck_y + 1.0;
    let mut root = assemble(prims);
    // Signature life: leaping flame and embers carried up off the fire.
    root.children
        .push(fx::beacon_flame([0.0, flame_y, 0.0], 0xB3AC_0F12));
    root.children
        .push(fx::rising_embers([0.0, flame_y + 0.3, 0.0], 0xE3BE_0F12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SignalBeacon.build(""), "signal_beacon");
    }

    #[test]
    fn has_firelight() {
        assert!(crate::catalogue::items::util::has_emissive(
            &SignalBeacon.build("")
        ));
    }
}
