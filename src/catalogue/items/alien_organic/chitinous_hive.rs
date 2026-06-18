//! Chitinous hive — the Alien-Organic landmark and the kit's lit hero. A
//! stacked bulb of dark chitin plating banded by ribs, biolume vents glowing
//! through the shell, a glowing maw at its foot and flesh tendrils curling
//! from the base. ~8 m across, so it anchors the colony and reads as the hive
//! from across the home region. Its biolume is the trim escalation's ruin pass
//! snuffs to a dead grey husk.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the base bulb.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_CYAN, CHITIN_DARK, FLESH_RED, SAC_GLOW, chitin, flesh, fx};

pub struct ChitinousHive;

impl CatalogueEntry for ChitinousHive {
    fn slug(&self) -> &'static str {
        "chitinous_hive"
    }
    fn name(&self) -> &'static str {
        "Chitinous Hive"
    }
    fn description(&self) -> &'static str {
        "Stacked chitin bulb banded by ribs, biolume vents aglow and a glowing maw."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Base chitin bulb — the root.
        prim(
            solid(sphere(3.6, 3, chitin(CHITIN_DARK))),
            [0.0, 2.6, 0.0],
            id_quat(),
        ),
    ];

    // Stacked mid bulb + crown bulb, tapering up.
    prims.push(prim(
        solid(sphere(2.6, 3, chitin(CHITIN_DARK))),
        [0.0, 5.6, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(1.8, 3.2, 8, chitin(CHITIN_DARK))),
        [0.0, 8.2, 0.0],
        id_quat(),
    ));

    // Chitin rib bands girdling the bulbs.
    prims.push(prim(
        solid(torus(0.3, 3.5, chitin(CHITIN_DARK))),
        [0.0, 2.6, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.22, 2.5, chitin(CHITIN_DARK))),
        [0.0, 5.6, 0.0],
        id_quat(),
    ));

    // Biolume vents glowing through the shell — emissive.
    for i in 0..5 {
        let a = i as f32 / 5.0 * TAU;
        prims.push(prim(
            sphere(0.4, 3, glow(BIOLUME_CYAN, 2.8)),
            [a.cos() * 3.3, 3.4 + (i % 2) as f32 * 1.6, a.sin() * 3.3],
            id_quat(),
        ));
    }
    // Glowing maw at the foot — emissive.
    prims.push(prim(
        sphere(0.9, 3, glow(SAC_GLOW, 2.4)),
        [0.0, 1.6, 3.2],
        id_quat(),
    ));

    // Flesh tendrils curling from the base.
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU + 0.4;
        prims.push(prim(
            solid(cylinder_tapered(0.3, 2.4, 6, 0.7, flesh(FLESH_RED))),
            [a.cos() * 3.2, 1.0, a.sin() * 3.2],
            quat_x(0.7),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: the hive's pulse and drifting spores.
    root.audio = fx::bio_pulse();
    root.children
        .push(fx::spore_drift([0.0, 2.0, 4.0], 0x0A11_8112));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ChitinousHive.build(""), "chitinous_hive");
    }

    #[test]
    fn has_biolume() {
        assert!(super::super::has_emissive(&ChitinousHive.build("")));
    }
}
