//! The steam dirigible: a striped spindle envelope pointed at both ends, a
//! long wooden spar slung under it on fans of rope, an open boat gondola hung
//! from the spar with a brass boiler and its funnel amidships, a big
//! two-blade wooden pusher at her stern and a triangular sail rudder at the
//! spar's tail - the line of Giffard's 1852 steam airship and Renard and
//! Krebs' La France (#1659, epic #1656).
//!
//! Prototyped beside the Zeppelin (`target/dump/airships-1656/steam.py`)
//! and ported number for number, the fan-out's six in one pass.
//!
//! # Where the port leaves the prototype, and why
//!
//! 1. An adorned ship's lanterns stand on a short post at each end of the
//!    gondola, where the prototype hung them in the air over its ends.
//! 2. An ornate ship's pennant streams aft and down off the spar's nose,
//!    where the prototype's streamed forward.
//! 3. The airscrew's shaft leaves her stern under the deck, through her
//!    hull, where the prototype's ran over the deck, touching nothing.
//! 4. The sail rudder is the seeded accent, the fleet's identity rule.
//! 5. Her rail runs round her gunwale through the heads of the posts that
//!    carry it, where the prototype's ran straight at her bow's beam,
//!    inboard of her midship posts and touching neither.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};
use crate::seeded_defaults::{AirshipBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_z, rod, sphere,
    sweep, with_cut,
};
use super::{
    AirshipCraft, AirshipFeel, HullPlan, HullProfile, MIN_DIM, SteamColours, dim, flat_sweep,
    gores, steam_colours,
};

/// Her envelope: a spindle, its ogive nose and tail nearly mirror images,
/// at the blueprint's length and 0.68 of its fineness.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.48,
    parallel: 0.04,
    nose_power: 1.45,
    tail_power: 1.5,
    stations: &[
        0.0, 0.02, 0.05, 0.09, 0.14, 0.21, 0.30, 0.40, 0.50, 0.60, 0.70, 0.79, 0.86, 0.91, 0.95,
        1.0,
    ],
    len_scale: 1.0,
    fine_scale: 0.68,
};

const HULL_RES: u32 = 36;
const HULL_SAMPLES: u32 = 6;
const GORES: usize = 12;

/// The spar: its ends as shares, its axis under the envelope's lowest line
/// and its radius; the ropes' stations and the bearing they leave the
/// envelope at, under her widest line.
const SPAR_X: (f32, f32) = (0.08, 0.93);
const SPAR_DROP: f32 = 0.24;
const SPAR_R: f32 = 0.022;
const ROPE_X: [f32; 7] = [0.16, 0.27, 0.38, 0.50, 0.62, 0.73, 0.84];
const ROPE_DEG: f32 = -38.0;

/// The gondola: her bow and stern as shares, her deck under the spar, her
/// half-beam and her depth (m, prototype frame).
const GONDOLA_X: (f32, f32) = (0.30, 0.66);
const GONDOLA_HANG: f32 = 0.36;
const GONDOLA_HALF_BEAM: f32 = 0.15;
pub(super) const GONDOLA_DEPTH: f32 = 0.15;
/// How far under the deck the airscrew's shaft leaves her stern.
const SHAFT_DROP: f32 = 0.04;
/// The airscrew's tip radius and turn (degrees a second).
const PROP_R: f32 = 0.17;
const PROP_DEG_S: f32 = 720.0;

pub(super) struct SteamDirigible;

impl AirshipCraft for SteamDirigible {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = steam_colours(ctx);
        let side = dim(plan.len(0.02));
        let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
        let kids = &mut root.children;
        envelope(kids, plan, &c);
        rigging(kids, plan, &c);
        gondola(kids, plan, &c);
        drive(kids, plan, &c);
        dress(kids, plan, &c, ctx.ornateness);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // The ponderous one: 5.5 of drive over 0.75 tops out at 7.3 m/s,
        // she turns slowest of them all, and her heavy damping stops a turn
        // the moment it is let go.
        AirshipFeel {
            mass_factor: 1.3,
            drive_accel: 5.5,
            yaw_accel: 4.5,
            linear_damping: 0.75,
            angular_damping: 5.5,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        let yd = deck_y(plan);
        let zm = gondola_middle(plan);
        match aura {
            // Her boiler's steam, smoke and sparks leave her funnel's mouth.
            ParticleAura::Steam | ParticleAura::Exhaust | ParticleAura::Embers => {
                [0.0, yd + plan.len(0.285), zm - plan.len(0.02)]
            }
            _ => [0.0, yd - plan.len(GONDOLA_DEPTH) - plan.len(0.05), zm],
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Steam
    }
}

fn envelope(kids: &mut Vec<Generator>, plan: &HullPlan, c: &SteamColours) {
    kids.push(prim(
        sweep(
            &plan.stations(0.0),
            HULL_RES,
            HULL_SAMPLES,
            c.gore_a.clone(),
        ),
        [0.0; 3],
        id_quat(),
    ));
    for cut in gores(GORES, 0.0) {
        kids.push(prim(
            with_cut(
                sweep(
                    &plan.stations(plan.len(0.003)),
                    HULL_RES,
                    HULL_SAMPLES,
                    c.gore_b.clone(),
                ),
                cut,
                [0.0, 1.0],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
    // A brass cap over each tip.
    let cap: Vec<(f32, f32)> = [(0.034, 0.0), (0.020, 0.020), (0.0, 0.042)]
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    for (x, turn) in [(0.0f32, FRAC_PI_2), (1.0, -FRAC_PI_2)] {
        kids.push(prim(
            lathe(&cap, 12, true, c.fittings.clone()),
            [0.0, 0.0, plan.z(x) - plan.len(0.012).copysign(turn)],
            quat_xyzw(quat_x(turn)),
        ));
    }
}

fn spar_y(plan: &HullPlan) -> f32 {
    -plan.girth() - plan.len(SPAR_DROP)
}

fn rigging(kids: &mut Vec<Generator>, plan: &HullPlan, c: &SteamColours) {
    let y = spar_y(plan);
    kids.push(rod(
        [0.0, y, plan.z(SPAR_X.0)],
        [0.0, y, plan.z(SPAR_X.1)],
        plan.len(SPAR_R),
        c.timber.clone(),
    ));
    let a = ROPE_DEG.to_radians();
    for x in ROPE_X {
        let r = plan.radius(x) - plan.len(0.012);
        let z = plan.z(x);
        for side in [1.0f32, -1.0] {
            kids.push(rope(
                [side * r * a.cos(), r * a.sin(), z],
                [0.0, y + plan.len(SPAR_R) * 0.5, z],
                c,
            ));
        }
    }
}

/// A rope: the thinnest straight rod the sanitiser keeps.
fn rope(a: [f32; 3], b: [f32; 3], c: &SteamColours) -> Generator {
    prim(
        sweep(&[(a, MIN_DIM), (b, MIN_DIM)], 6, 2, c.rope.clone()),
        [0.0; 3],
        id_quat(),
    )
}

fn deck_y(plan: &HullPlan) -> f32 {
    spar_y(plan) - plan.len(GONDOLA_HANG)
}

fn gondola_middle(plan: &HullPlan) -> f32 {
    (plan.z(GONDOLA_X.0) + plan.z(GONDOLA_X.1)) * 0.5
}

/// The gondola's half-beam a share `t` of the way from her bow to her stern.
fn half_beam(plan: &HullPlan, t: f32) -> f32 {
    plan.len(GONDOLA_HALF_BEAM) * (PI * (0.06 + 0.88 * t)).sin().max(0.0).sqrt()
}

fn gondola(kids: &mut Vec<Generator>, plan: &HullPlan, c: &SteamColours) {
    let yd = deck_y(plan);
    let (zb, zs) = (plan.z(GONDOLA_X.0), plan.z(GONDOLA_X.1));
    let along = |t: f32| zb + (zs - zb) * t;
    // Her hull: a half-pipe bow to stern, its cut face the deck - a sweep
    // flattened to her depth over her half-beam.
    let hb = plan.len(GONDOLA_HALF_BEAM);
    let points: Vec<([f32; 3], f32)> = (0..7)
        .map(|k| {
            let t = k as f32 / 6.0;
            ([0.0, yd, along(t)], half_beam(plan, t).max(hb * 0.08))
        })
        .collect();
    kids.push(flat_sweep(
        &points,
        20,
        4,
        1,
        plan.len(GONDOLA_DEPTH) / hb,
        &c.timber,
        Some(([0.5, 1.0], [0.0, 1.0])),
    ));
    // Her rail: a post at each end and amidships a side, and the rail on
    // them; four hangers from the spar to the rail's ends.
    let rail_y = yd + plan.len(0.09);
    for side in [1.0f32, -1.0] {
        for t in [0.12f32, 0.5, 0.88] {
            let w = half_beam(plan, t) * 0.9;
            kids.push(post(
                [side * w, yd - plan.len(0.02), along(t)],
                [side * w, rail_y, along(t)],
                c,
            ));
        }
        // The rail runs round her gunwale through the posts' heads.
        let rail: Vec<([f32; 3], f32)> = [0.12f32, 0.31, 0.5, 0.69, 0.88]
            .iter()
            .map(|&t| ([side * half_beam(plan, t) * 0.9, rail_y, along(t)], MIN_DIM))
            .collect();
        kids.push(prim(
            sweep(&rail, 6, 4, c.timber.clone()),
            [0.0; 3],
            id_quat(),
        ));
        for t in [0.12f32, 0.88] {
            let w = half_beam(plan, t) * 0.9;
            kids.push(rope(
                [side * w, rail_y - plan.len(0.005), along(t)],
                [0.0, spar_y(plan), along(t)],
                c,
            ));
        }
    }
    // The boiler amidships, its firebox door glowing, and its funnel with
    // a brass crown.
    let zm = gondola_middle(plan);
    kids.push(prim(
        cylinder(plan.len(0.07), plan.len(0.17), 16, c.fittings.clone()),
        [0.0, yd + plan.len(0.075), zm],
        id_quat(),
    ));
    kids.push(prim(
        cylinder(plan.len(0.03), plan.len(0.012), 12, c.fire.clone()),
        [0.0, yd + plan.len(0.03), zm + plan.len(0.066)],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    kids.push(prim(
        cylinder(plan.len(0.022), plan.len(0.13), 12, c.dark.clone()),
        [0.0, yd + plan.len(0.21), zm - plan.len(0.02)],
        id_quat(),
    ));
    kids.push(prim(
        cylinder(plan.len(0.032), plan.len(0.02), 12, c.fittings.clone()),
        [0.0, yd + plan.len(0.275), zm - plan.len(0.02)],
        id_quat(),
    ));
}

/// A timber post or rail: the thinnest straight rod the sanitiser keeps.
fn post(a: [f32; 3], b: [f32; 3], c: &SteamColours) -> Generator {
    prim(
        sweep(&[(a, MIN_DIM), (b, MIN_DIM)], 6, 2, c.timber.clone()),
        [0.0; 3],
        id_quat(),
    )
}

fn drive(kids: &mut Vec<Generator>, plan: &HullPlan, c: &SteamColours) {
    let yd = deck_y(plan);
    let zs = plan.z(GONDOLA_X.1);
    // The shaft runs out of her stern under the deck, where her hull is.
    let y = yd - plan.len(SHAFT_DROP);
    kids.push(rod(
        [0.0, y, zs + plan.len(0.06)],
        [0.0, y, zs - plan.len(0.05)],
        plan.len(0.014),
        c.dark.clone(),
    ));
    // The airscrew: two wooden blades on a brass boss, drawn upright.
    let mut parts = vec![prim(
        sphere(plan.len(0.024), 3, c.fittings.clone()),
        [0.0; 3],
        id_quat(),
    )];
    let root = 0.014;
    let mid = plan.len(root + PROP_R) * 0.5;
    for k in 0..2 {
        let angle = (90.0 + 180.0 * k as f32).to_radians();
        parts.push(prim(
            cuboid(
                [plan.len(PROP_R - root), plan.len(0.04), MIN_DIM],
                c.timber.clone(),
            ),
            [angle.cos() * mid, angle.sin() * mid, 0.0],
            quat_xyzw(quat_mul(quat_z(angle), quat_x(20f32.to_radians()))),
        ));
    }
    let side = dim(plan.len(0.02));
    let mut hub = prim(
        cuboid([side; 3], c.dark.clone()),
        [0.0, y, zs - plan.len(0.06)],
        id_quat(),
    );
    hub.children = parts;
    hub.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Constant {
            rate: Fp(PROP_DEG_S),
        },
    ));
    kids.push(hub);
    // The sail rudder: a triangle of canvas hung on the spar's tail.
    let ys = spar_y(plan);
    let zt = plan.z(SPAR_X.1);
    kids.push(flat_sweep(
        &[
            ([0.0, ys, zt + plan.len(0.30)], plan.len(0.012)),
            ([0.0, ys, zt + plan.len(0.06)], plan.len(0.17)),
            ([0.0, ys, zt - plan.len(0.04)], plan.len(0.20)),
        ],
        16,
        4,
        0,
        0.06,
        &c.rudder,
        None,
    ));
}

fn dress(kids: &mut Vec<Generator>, plan: &HullPlan, c: &SteamColours, tier: OrnatenessTier) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    // A lantern on a short post at each end of the gondola.
    let yd = deck_y(plan);
    let (zb, zs) = (plan.z(GONDOLA_X.0), plan.z(GONDOLA_X.1));
    for t in [0.05f32, 0.95] {
        let z = zb + (zs - zb) * t;
        kids.push(post(
            [0.0, yd - plan.len(0.01), z],
            [0.0, yd + plan.len(0.09), z],
            c,
        ));
        kids.push(prim(
            sphere(plan.len(0.024), 3, c.lamp.clone()),
            [0.0, yd + plan.len(0.10), z],
            id_quat(),
        ));
    }
    if tier == OrnatenessTier::Ornate {
        // A pennant streaming aft and down off the spar's nose.
        let y = spar_y(plan);
        let z0 = plan.z(SPAR_X.0);
        kids.push(flat_sweep(
            &[
                ([0.0, y, z0 + plan.len(0.01)], plan.len(0.045)),
                (
                    [0.0, y - plan.len(0.12), z0 - plan.len(0.15)],
                    plan.len(0.030),
                ),
                (
                    [0.0, y - plan.len(0.18), z0 - plan.len(0.32)],
                    plan.len(0.012),
                ),
            ],
            12,
            4,
            0,
            0.08,
            &c.pennant,
            None,
        ));
    }
}
