//! The organic floater: a living gasbag of the alien worlds - a jellyfish
//! bell wider than it is tall, glowing radial canals and spots on it, a
//! scalloped margin, and long tentacles that ripple round the ring and trail
//! when she sets off; four oral arms from the adorned tier, a crown of
//! glowing nodules and beaded tentacle tips on the ornate (#1663, epic
//! #1656).
//!
//! Prototyped beside the Zeppelin (`target/dump/airships-1656/floater.py`)
//! and ported number for number, the fan-out's six in one pass. Her hull plan
//! stands on end, as the balloon's does: share `x` runs from her crown down
//! to her margin. Her glow - canals, spots, arms and nodules - is the seeded
//! accent's own light, where the prototype gave each sketch a fixed one: a
//! living thing wears its owner's colour as its light. Her tentacles' tips
//! are beads the colour of her margin, where the prototype lit them: a lamp
//! under a swinging hub cannot be read for whether it shows.
//!
//! Each tentacle hangs from a hidden hub that swings it a few degrees about
//! +X, an eighth of a period behind its neighbour, so one ripple runs round
//! the ring; and leans it against her acceleration, so the tentacles trail when
//! she sets off and swing forward when she stops. Neither turns a tentacle
//! all the way round, so her box holds them where they are drawn
//! ([`super::swept_bounds`]).

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::pds::{Fp, Spin, SpinTerm};
use crate::seeded_defaults::{AirshipBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{
    cuboid, id_quat, prim, quat_xyzw, quat_y, sphere, superellipsoid, sweep, with_cut,
};
use super::{
    AirshipCraft, AirshipFeel, FloaterColours, HullPlan, HullProfile, MIN_DIM, dim, floater_colours,
};

/// Her bell, crown down: a rounded dome to her widest at the margin - a
/// quarter of the blueprint's length tall, at an eighth of its fineness.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.86,
    parallel: 0.14,
    nose_power: 1.7,
    tail_power: 2.0,
    stations: &[
        0.0, 0.02, 0.06, 0.12, 0.20, 0.30, 0.42, 0.55, 0.68, 0.80, 0.90, 0.98,
    ],
    len_scale: 0.26,
    fine_scale: 0.125,
};

/// Her margin, as a share of her height from the crown.
pub(super) const MARGIN_X: f32 = 0.98;
const BELL_RES: u32 = 32;
const BELL_SAMPLES: u32 = 6;
/// Her radial canals: how many, and each one's width (turns).
const CANALS: usize = 8;
const CANAL_W: f32 = 0.018;
/// Her tentacles: how many, how long, and their swing and lean (degrees;
/// seconds; degrees for each m/s^2).
const TENTACLES: usize = 8;
const TENT_LEN: f32 = 1.15;
const SWING_DEG: f32 = 7.0;
const SWING_S: f32 = 2.6;
const LEAN_GAIN: f32 = 2.0;
const LEAN_S: f32 = 2.4;

pub(super) struct Floater;

impl AirshipCraft for Floater {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = floater_colours(ctx);
        let side = dim(plan.len(0.02));
        let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
        let kids = &mut root.children;
        bell(kids, plan, &c);
        tentacles(kids, plan, &c, ctx.ornateness);
        dress(kids, plan, &c, ctx.ornateness);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // She pulses along: 5 of drive over 0.7 tops out at 7.1 m/s, light,
        // and lazy in a turn.
        AirshipFeel {
            mass_factor: 0.7,
            drive_accel: 5.0,
            yaw_accel: 4.0,
            linear_damping: 0.7,
            angular_damping: 3.5,
        }
    }

    fn fx_mount(&self, _aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        // Everything drifts from under her bell, among her tentacles.
        [0.0, plan.z(MARGIN_X) - plan.len(0.10), 0.0]
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Sail
    }
}

fn bell(kids: &mut Vec<Generator>, plan: &HullPlan, c: &FloaterColours) {
    kids.push(prim(
        sweep(&plan.upright(0.0), BELL_RES, BELL_SAMPLES, c.bell.clone()),
        [0.0; 3],
        id_quat(),
    ));
    // Her radial canals, glowing faintly down the bell.
    for k in 0..CANALS {
        let a = k as f32 / CANALS as f32;
        kids.push(prim(
            with_cut(
                sweep(
                    &plan.upright(plan.len(0.003)),
                    BELL_RES,
                    BELL_SAMPLES,
                    c.canal.clone(),
                ),
                [a, a + CANAL_W],
                [0.08, 1.0],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
    let ym = plan.z(MARGIN_X);
    let rm = plan.radius(MARGIN_X);
    // The subumbrella: a darker disc just inside the margin.
    kids.push(prim(
        superellipsoid(
            [rm * 0.94, plan.len(0.03), rm * 0.94],
            1.0,
            1.0,
            8,
            32,
            c.under.clone(),
        ),
        [0.0, ym - plan.len(0.012), 0.0],
        id_quat(),
    ));
    // The scalloped margin: a lobe between each pair of canals.
    for k in 0..CANALS * 2 {
        let a = (k as f32 + 0.5) / (CANALS * 2) as f32 * std::f32::consts::TAU;
        kids.push(prim(
            superellipsoid(
                [plan.len(0.10), plan.len(0.035), plan.len(0.05)],
                0.9,
                0.9,
                8,
                12,
                c.margin.clone(),
            ),
            [
                a.cos() * rm * 0.97,
                ym - plan.len(0.02),
                a.sin() * rm * 0.97,
            ],
            quat_xyzw(quat_y(-a)),
        ));
    }
    // Glowing spots on the bell, one between each pair of canals.
    let (ys, rs) = (plan.z(0.45), plan.radius(0.45) - plan.len(0.012));
    for k in 0..CANALS {
        let a = (k as f32 + 0.5) / CANALS as f32 * std::f32::consts::TAU;
        kids.push(prim(
            sphere(plan.len(0.03), 3, c.glow.clone()),
            [a.cos() * rs, ys, a.sin() * rs],
            id_quat(),
        ));
    }
}

fn tentacles(kids: &mut Vec<Generator>, plan: &HullPlan, c: &FloaterColours, tier: OrnatenessTier) {
    let ym = plan.z(MARGIN_X);
    let rm = plan.radius(MARGIN_X);
    let l = plan.len(TENT_LEN);
    for k in 0..TENTACLES {
        let a = (k as f32 + 0.25) / TENTACLES as f32 * std::f32::consts::TAU;
        let at = [
            a.cos() * rm * 0.80,
            ym - plan.len(0.01),
            a.sin() * rm * 0.80,
        ];
        let mut parts = vec![prim(
            sweep(
                &[
                    ([0.0, plan.len(0.03), 0.0], plan.len(0.030)),
                    ([0.0, -l * 0.25, -plan.len(0.05)], plan.len(0.026)),
                    ([0.0, -l * 0.55, plan.len(0.02)], plan.len(0.020)),
                    ([0.0, -l * 0.80, -plan.len(0.06)], plan.len(0.015)),
                    ([0.0, -l, -plan.len(0.10)], MIN_DIM),
                ],
                10,
                4,
                c.tentacles.clone(),
            ),
            [0.0; 3],
            id_quat(),
        )];
        if tier == OrnatenessTier::Ornate {
            // A bead at the tip - unlit: a lamp must hang on the root to be
            // read for whether it shows, and a tip swings.
            parts.push(prim(
                sphere(plan.len(0.018), 3, c.margin.clone()),
                [0.0, -l, -plan.len(0.10)],
                id_quat(),
            ));
        }
        let side = dim(plan.len(0.02));
        let mut hub = prim(cuboid([side; 3], c.dark.clone()), at, id_quat());
        hub.children = parts;
        hub.spin = Some(Spin {
            axis: Fp3([1.0, 0.0, 0.0]),
            terms: vec![
                SpinTerm::Swing {
                    amplitude: Fp(SWING_DEG),
                    period: Fp(SWING_S),
                    phase: Fp(45.0 * k as f32),
                },
                SpinTerm::Lean {
                    gain: Fp(LEAN_GAIN),
                    period: Fp(LEAN_S),
                },
            ],
        });
        kids.push(hub);
    }
}

fn dress(kids: &mut Vec<Generator>, plan: &HullPlan, c: &FloaterColours, tier: OrnatenessTier) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    let ym = plan.z(MARGIN_X);
    // Four oral arms from the bell's middle, frilled and glowing.
    let l = plan.len(0.70);
    for k in 0..4 {
        let a = (k as f32 + 0.5) / 4.0 * std::f32::consts::TAU;
        let (ox, oz) = (a.cos() * plan.len(0.10), a.sin() * plan.len(0.10));
        kids.push(prim(
            sweep(
                &[
                    ([ox, ym + plan.len(0.02), oz], plan.len(0.055)),
                    ([ox * 1.3, ym - l * 0.35, oz * 1.3], plan.len(0.045)),
                    ([ox * 0.9, ym - l * 0.70, oz * 0.9], plan.len(0.032)),
                    ([ox * 1.2, ym - l, oz * 1.2], plan.len(0.016)),
                ],
                12,
                4,
                c.canal.clone(),
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
    if tier == OrnatenessTier::Ornate {
        // A crown of glowing nodules round the bell's top.
        let (yc, rc) = (plan.z(0.14), plan.radius(0.14) - plan.len(0.01));
        for k in 0..5 {
            let a = k as f32 / 5.0 * std::f32::consts::TAU;
            kids.push(prim(
                sphere(plan.len(0.035), 3, c.glow.clone()),
                [a.cos() * rc, yc + plan.len(0.01), a.sin() * rc],
                id_quat(),
            ));
        }
    }
}
