//! Signal beacon — a Nordic secondary and the kit's firelit hero. A timber
//! framework on a fieldstone base hoists an iron brazier of burning logs
//! high enough to be seen across the fjord: the warning-fire chain that
//! mustered the fleet. Leaping flame, drifting embers, and a fire crackle
//! bring it alive; its emissive core is the trim escalation's ruin pass
//! snuffs to a cold dead brazier.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
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
        "Timber-framed iron brazier on a fieldstone base, burning as a warning fire."
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
            solid(cylinder_tapered(0.28, post_h, 8, 0.15, timber(WOOD_WARM))),
            [0.0, base_h + post_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Four corner posts.
    let r = 0.95_f32;
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.14,
                post_h - 0.4,
                8,
                0.0,
                timber(WOOD_DARK),
            )),
            [sx * r, base_h + (post_h - 0.4) * 0.5, sz * r],
            id_quat(),
        ));
    }
    // Cross-brace frames at two heights.
    for h in [base_h + post_h * 0.45, base_h + post_h * 0.95] {
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.12, 0.12, 2.0 * r],
                    0.0,
                    timber(WOOD_DARK),
                )),
                [sx * r, h, 0.0],
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
                [0.0, h, sz * r],
                id_quat(),
            ));
        }
    }

    // Timber deck under the brazier.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 0.3, 2.4], 0.0, timber(WOOD_WARM))),
        [0.0, deck_y + 0.15, 0.0],
        id_quat(),
    ));
    // Iron brazier bowl + rim.
    prims.push(prim(
        solid(cylinder_tapered(0.9, 0.7, 12, 0.25, iron(IRON_DARK))),
        [0.0, deck_y + 0.65, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.12, 0.9, iron(IRON_DARK)),
        [0.0, deck_y + 0.95, 0.0],
        id_quat(),
    ));

    // Glowing fire core inside the brazier — the emissive heart, crackling.
    let mut fire = prim(
        sphere(0.6, 3, glow(FIRE_ORANGE, 6.0)),
        [0.0, deck_y + 0.9, 0.0],
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    prims.push(fire);

    let flame_y = deck_y + 1.1;
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
        assert!(super::super::has_emissive(&SignalBeacon.build("")));
    }
}
