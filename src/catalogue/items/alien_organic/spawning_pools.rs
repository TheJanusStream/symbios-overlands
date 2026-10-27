//! Spawning pools - an Alien-Organic secondary that leads a shore colony.
//! Four fleshy basins step down toward the water (-Z), each a swollen drum
//! of tissue with a thick lip round a pool of glowing brood-fluid, spilling
//! one into the next. Clutches of lit egg sacs cluster on their rims and in
//! the shallows, and behind them a ribbed stalk rises 15 m to a crown of
//! membrane sails with lit pods at their tips.
//!
//! Each basin is a sub-tree on its drum, the stalk one on its root swell, so
//! a ruined colony knocks each as one.

use std::f32::consts::{PI, TAU};

use crate::catalogue::items::util::{
    attach, cylinder_tapered, footing_disc, glow, id_quat, lathe, nest, prim, prim_scaled,
    quat_mul, quat_y, quat_z, solid, sphere, spine, strut, superellipsoid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BIOLUME_CYAN, BIOLUME_GREEN, CHITIN_DARK, FLESH_PINK, FLESH_RED, MEMBRANE_TEAL, SAC_GLOW,
    chitin, egg_pod, flesh, fx, membrane,
};

/// Spacing circle the settlement keeps round the pools.
const CLEARANCE: f32 = 12.0;

/// A basin: its centre in plan, its radius and the height of its lip.
struct Basin {
    at: [f32; 2],
    r: f32,
    rim: f32,
}

/// The basins, from the highest at the back down to the shallowest at the
/// water's edge.
const BASINS: [Basin; 4] = [
    Basin {
        at: [0.6, 4.8],
        r: 4.6,
        rim: 2.6,
    },
    Basin {
        at: [-6.0, -1.0],
        r: 3.6,
        rim: 1.7,
    },
    Basin {
        at: [4.6, -4.2],
        r: 3.3,
        rim: 1.1,
    },
    Basin {
        at: [-2.0, -8.0],
        r: 2.6,
        rim: 0.6,
    },
];

/// How far below grade every drum reaches, so a basin on a slope shows
/// tissue, not daylight, under its low side.
const SUNK: f32 = 0.6;

/// Where the stalk stands, and the (radius, height) stations of its turned
/// trunk, the root swell at the foot to the bud at the head.
const STALK_AT: [f32; 2] = [6.8, 8.6];
const STALK: [(f32, f32); 14] = [
    (1.7, -0.4),
    (1.35, 1.0),
    (1.05, 2.5),
    (0.9, 4.0),
    (0.8, 5.5),
    (0.72, 7.0),
    (0.66, 8.5),
    (0.62, 10.0),
    (0.6, 11.5),
    (0.66, 13.0),
    (0.95, 14.0),
    (0.9, 14.8),
    (0.55, 15.4),
    (0.0, 15.8),
];
/// Height the crown's sails spring from.
const CROWN_Y: f32 = 14.3;

pub struct SpawningPools;

impl CatalogueEntry for SpawningPools {
    fn slug(&self) -> &'static str {
        "spawning_pools"
    }
    fn name(&self) -> &'static str {
        "Spawning Pools"
    }
    fn description(&self) -> &'static str {
        "Fleshy pool basins stepping down to the shore, glowing egg clutches and a tall ribbed stalk crowned in membrane."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// The brood is laid where the pools can drink the sea.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The brood-fluid in a pool: dark wet membrane lit from within, held low so
/// a broad face keeps its colour.
fn brood_fluid() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.03, 0.16, 0.08]),
        emission_color: Fp3(BIOLUME_GREEN),
        emission_strength: Fp(0.7),
        roughness: Fp(0.08),
        metallic: Fp(0.1),
        ..Default::default()
    }
}

/// A lit egg sac on a stub of tissue.
fn egg(foot: [f32; 3], r: f32) -> Generator {
    egg_pod(foot, r, 1.35, glow(SAC_GLOW, 2.2), flesh(FLESH_RED))
}

/// One basin: the swollen drum of shell, chitin ribs up its belly, the lip
/// round its pool, the fluid in it, a swell of tissue round its foot and the
/// eggs in its shallows.
fn basin(b: &Basin, k: usize) -> Generator {
    let [x, z] = b.at;
    let wall_top = b.rim - 0.3;
    // The drum bellies out low and draws in to its lip.
    let top_r = b.r * 0.86;
    let drum = prim(
        solid(lathe(
            &[
                (b.r * 0.95, -SUNK),
                (b.r * 1.02, wall_top * 0.35),
                (b.r * 0.94, wall_top * 0.8),
                (top_r, wall_top),
            ],
            22,
            true,
            chitin(CHITIN_DARK),
        )),
        [x, 0.0, z],
        id_quat(),
    );
    let lip_minor = 0.34;
    let mut parts = vec![
        prim_scaled(
            torus(lip_minor, top_r - 0.2, flesh(FLESH_PINK)),
            [x, b.rim - 0.12, z],
            id_quat(),
            [1.0, 0.62, 1.0],
        ),
        prim(
            cylinder_tapered(top_r - 0.3, 0.08, 22, 0.0, brood_fluid()),
            [x, b.rim - 0.25, z],
            id_quat(),
        ),
        // The swell of tissue the drum rises out of.
        prim(
            superellipsoid(
                [b.r + 0.7, 0.55_f32.min((b.rim - 0.3) * 0.7), b.r + 0.7],
                1.0,
                1.0,
                flesh(FLESH_RED),
            ),
            [x, 0.0, z],
            id_quat(),
        ),
    ];
    // Ribs of harder shell up the belly, curving in with it.
    let ribs = 5 + k % 2 * 2;
    for i in 0..ribs {
        let a = (i as f32 + 0.5 * k as f32) * TAU / ribs as f32;
        let at = |r: f32, y: f32| [x + r * a.sin(), y, z + r * a.cos()];
        parts.push(strut(
            at(b.r * 1.04, 0.0),
            at(top_r * 1.05, wall_top - 0.05),
            0.16,
            6,
            chitin(CHITIN_DARK),
        ));
    }
    // Eggs half sunk in the shallows, round the pool's middle.
    for i in 0..3 {
        let a = (i as f32 + 0.3 * k as f32) * TAU / 3.0;
        let d = (top_r - 0.9) * 0.55;
        parts.push(prim_scaled(
            sphere(0.32 + 0.05 * i as f32, 6, glow(SAC_GLOW, 2.0)),
            [x + d * a.sin(), b.rim - 0.2, z + d * a.cos()],
            id_quat(),
            [1.0, 1.25, 1.0],
        ));
    }
    nest(drum, parts)
}

/// The spill from one basin's lip down into the next: a tongue of tissue
/// with a lit runnel of fluid along its top.
fn spill(from: &Basin, to: &Basin) -> Vec<Generator> {
    let (dx, dz) = (to.at[0] - from.at[0], to.at[1] - from.at[1]);
    let len = (dx * dx + dz * dz).sqrt();
    let (ux, uz) = (dx / len, dz / len);
    let a = [
        from.at[0] + ux * (from.r * 0.8),
        from.rim - 0.15,
        from.at[1] + uz * (from.r * 0.8),
    ];
    let b = [
        to.at[0] - ux * (to.r * 0.75),
        to.rim - 0.1,
        to.at[1] - uz * (to.r * 0.75),
    ];
    vec![
        strut(a, b, 0.42, 8, flesh(FLESH_PINK)),
        strut(
            [a[0], a[1] + 0.33, a[2]],
            [b[0], b[1] + 0.33, b[2]],
            0.13,
            6,
            brood_fluid(),
        ),
    ]
}

/// The stalk: the turned trunk on its root swell, chitin ribs girdling it,
/// and the crown - a bulb ringed with lit veins, six membrane sails on lit
/// ribs and a pod glowing at each sail's tip.
fn stalk() -> Generator {
    let [x, z] = STALK_AT;
    let swell = prim(
        solid(cylinder_tapered(2.2, 0.7, 16, 0.5, chitin(CHITIN_DARK))),
        [x, 0.15, z],
        id_quat(),
    );
    let trunk = prim(
        solid(lathe(&STALK, 14, true, flesh(FLESH_RED))),
        [x, 0.0, z],
        id_quat(),
    );
    let mut ribs = Vec::new();
    for &(r, h) in &STALK[2..10] {
        ribs.push(prim(
            torus(0.13, r + 0.03, chitin(CHITIN_DARK)),
            [x, h, z],
            id_quat(),
        ));
    }
    ribs.push(prim(
        torus(0.1, 0.97, glow(BIOLUME_GREEN, 2.4)),
        [x, 14.0, z],
        id_quat(),
    ));
    for i in 0..6 {
        let a = i as f32 * TAU / 6.0 + 0.3;
        let (ca, sa) = (a.cos(), a.sin());
        let tilt = 0.42;
        let reach = 3.4;
        let turn = quat_mul(quat_y(-a), quat_z(tilt));
        let c = [
            x + ca * (0.7 + reach * 0.5 * tilt.cos()),
            CROWN_Y + reach * 0.5 * tilt.sin(),
            z + sa * (0.7 + reach * 0.5 * tilt.cos()),
        ];
        ribs.push(prim(
            crate::catalogue::items::util::cuboid_tapered(
                [reach, 0.05, 2.1],
                0.0,
                membrane(MEMBRANE_TEAL),
            ),
            c,
            turn,
        ));
        ribs.push(prim(
            crate::catalogue::items::util::cuboid_tapered(
                [reach, 0.09, 0.09],
                0.0,
                glow(BIOLUME_CYAN, 2.4),
            ),
            c,
            turn,
        ));
        let tip = 0.7 + reach * tilt.cos();
        ribs.push(prim(
            sphere(0.34, 6, glow(BIOLUME_GREEN, 2.4)),
            [x + ca * tip, CROWN_Y + reach * tilt.sin(), z + sa * tip],
            id_quat(),
        ));
    }
    nest(swell, vec![nest(trunk, ribs)])
}

/// A tendril rising out of the ground at `foot` and hooking over toward
/// `out` (rad in plan, 0 at +X), a lit nodule at its tip.
fn curl(foot: [f32; 3], out: f32) -> Generator {
    let (c, s) = (out.cos(), out.sin());
    let at = |d: f32, y: f32| [d * c, y, d * s];
    let stations = [
        (at(0.0, -0.3), 0.5),
        (at(0.25, 1.4), 0.38),
        (at(1.0, 2.6), 0.27),
        (at(2.1, 2.9), 0.17),
        (at(2.8, 2.2), 0.08),
    ];
    let tip = stations[4].0;
    let stem = prim(spine(&stations, 8, 6, flesh(FLESH_PINK)), foot, id_quat());
    nest(
        stem,
        vec![prim(
            sphere(0.16, 6, glow(BIOLUME_GREEN, 2.4)),
            [foot[0] + tip[0], foot[1] + tip[1], foot[2] + tip[2]],
            id_quat(),
        )],
    )
}

fn build_tree() -> Generator {
    // The footing lies inside the top basin's drum.
    let mut root = footing_disc(BASINS[0].r - 0.8, CLEARANCE);
    root.audio = fx::bio_pulse();
    for (k, b) in BASINS.iter().enumerate() {
        attach(&mut root, basin(b, k));
    }
    for pair in [(0, 1), (0, 2), (1, 3)] {
        for part in spill(&BASINS[pair.0], &BASINS[pair.1]) {
            attach(&mut root, part);
        }
    }
    attach(&mut root, stalk());
    // Clutches of egg sacs on the ground between the pools.
    for (cx, cz, n) in [
        (-1.4_f32, -3.2_f32, 3_usize),
        (7.8, 1.6, 4),
        (-7.6, 4.6, 3),
        (1.8, -8.6, 3),
    ] {
        for i in 0..n {
            let a = i as f32 * TAU / n as f32 + cx;
            let r = 0.45 + 0.08 * i as f32;
            attach(
                &mut root,
                egg([cx + 0.8 * a.cos(), 0.0, cz + 0.8 * a.sin()], r),
            );
        }
    }
    // Tendrils curling out of the swells at the colony's edge.
    for (fx_, fz, out) in [
        (-9.4_f32, -2.0_f32, PI),
        (8.2, -5.0, -0.3),
        (-3.6, 9.2, 2.0),
    ] {
        attach(&mut root, curl([fx_, 0.0, fz], out));
    }
    attach(&mut root, fx::spore_drift([0.0, 1.5, -2.0], 0x0B1C_5B0A));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SpawningPools.build(""), "spawning_pools");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = SpawningPools.build("");
        assert_nothing_floats(&g, "spawning_pools");
        assert_no_z_fighting(&g, "spawning_pools");
    }

    #[test]
    fn keeps_its_biolume() {
        assert!(has_emissive(&SpawningPools.build("")));
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&SpawningPools.build("")).unwrap().len();
        eprintln!("spawning_pools record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
