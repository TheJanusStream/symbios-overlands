//! Chitinous hive — the Alien-Organic landmark and the kit's lit hero. A
//! swelling tower of stacked chitin bulbs girdled by carapace ribs, a cluster
//! of venting chimney-spouts at the crown, biolume pods glowing through the
//! shell, a glowing maw ringed with fangs on its front, brood pods budding at
//! the foot and flesh tendrils curling out of the creep. ~8 m across, so it
//! anchors the colony and reads as the hive from across the home region. Its
//! biolume is the trim escalation's ruin pass snuffs to a dead grey husk.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the base bulb (the root, `id_quat`).

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BIOLUME_CYAN, CHITIN_DARK, FLESH_PINK, FLESH_RED, SAC_GLOW, chitin, egg_pod, flesh, fx, tendril,
};

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
        // Base chitin bulb — the root (id_quat).
        prim(
            solid(sphere(3.5, 4, chitin(CHITIN_DARK))),
            [0.0, 2.5, 0.0],
            id_quat(),
        ),
    ];

    // Stacked mid + crown bulbs, swelling up the tower.
    prims.push(prim(
        solid(sphere(2.5, 4, chitin(CHITIN_DARK))),
        [0.0, 5.4, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(1.7, 4, chitin(CHITIN_DARK))),
        [0.0, 7.6, 0.0],
        id_quat(),
    ));

    // Cluster of venting chimney-spouts at the crown (not one party-hat cone):
    // short tapered chitin funnels with lit throats, the hive breathing.
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU + 0.5;
        let (cx, cz) = (a.cos() * 0.7, a.sin() * 0.7);
        prims.push(prim(
            solid(cylinder_tapered(0.42, 2.0, 8, 0.4, chitin(CHITIN_DARK))),
            [cx, 9.2, cz],
            quat_x(0.12 * a.sin()),
        ));
        prims.push(prim(
            sphere(0.26, 4, glow(BIOLUME_CYAN, 2.0)),
            [cx, 10.2, cz],
            id_quat(),
        ));
    }

    // Carapace rib bands girdling the bulbs, proud of the shell.
    for (y, major) in [(2.5_f32, 3.55_f32), (4.0, 3.1), (5.4, 2.55), (6.6, 2.0)] {
        prims.push(prim(
            solid(torus(0.28, major, chitin(CHITIN_DARK))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Biolume pods glowing through the shell — clustered, proud, deep cyan.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU + 0.3;
        let y = 3.2 + (i % 3) as f32 * 1.3;
        let rad = if y > 5.0 { 2.7 } else { 3.5 };
        prims.push(prim(
            sphere(0.34 + (i % 2) as f32 * 0.12, 4, glow(BIOLUME_CYAN, 2.0)),
            [a.cos() * rad, y, a.sin() * rad],
            id_quat(),
        ));
    }

    // Glowing maw on the −Z hero front: a puckered chitin lip-ring with a
    // glowing throat behind it and fangs around the rim.
    prims.push(prim(
        solid(torus(0.34, 1.05, chitin(CHITIN_DARK))),
        [0.0, 2.0, -2.95],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        sphere(0.95, 5, glow(SAC_GLOW, 2.0)),
        [0.0, 2.0, -2.7],
        id_quat(),
    ));
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        prims.push(prim(
            solid(cone(0.16, 0.6, 6, chitin(CHITIN_DARK))),
            [a.cos() * 1.0, 2.0 + a.sin() * 1.0, -3.0],
            quat_x(-FRAC_PI_2),
        ));
    }

    // Brood pods budding at the foot — a couple lit, the next generation.
    for (px, pz, lit) in [
        (-2.6_f32, 1.4_f32, true),
        (2.5, 1.0, false),
        (-0.4, -2.7, true),
    ] {
        let pod_mat = if lit {
            glow(SAC_GLOW, 1.9)
        } else {
            flesh(FLESH_PINK)
        };
        prims.push(egg_pod([px, 0.0, pz], 0.6, 1.35, pod_mat, flesh(FLESH_RED)));
    }

    // Flesh tendrils curling out of the creep at the base.
    for i in 0..5 {
        let a = i as f32 / 5.0 * TAU + 0.4;
        prims.push(tendril(
            [a.cos() * 3.0, 0.0, a.sin() * 3.0],
            a,
            0.34,
            0.7,
            4,
            0.45,
            flesh(FLESH_RED),
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
        assert!(crate::catalogue::items::util::has_emissive(
            &ChitinousHive.build("")
        ));
    }
}
