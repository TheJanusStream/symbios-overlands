//! The hot-air balloon: a gored envelope stood on end - a rounded crown,
//! widest two thirds of the way up, tapering to the throat - its gores in
//! the scheme's colours round it in turn and its crown cap the owner's own,
//! a wicker basket slung under the throat on four cables, the burner on its
//! frame over the basket and its flame lit (#1661, epic #1656).
//!
//! Prototyped beside the Zeppelin (`target/dump/airships-1656/balloon.py`)
//! and ported number for number, the fan-out's six in one pass. Her hull plan
//! stands on end ([`HullPlan::upright`]): share `x` runs from her crown down,
//! and the sweep stops at [`THROAT_X`], where she opens.
//!
//! # Where the port leaves the prototype, and why
//!
//! 1. The crown cap is the seeded accent, held clear of her first gore -
//!    the fleet's identity rule.
//! 2. An adorned balloon hangs a lantern on her basket's side and an ornate
//!    one burns a second, smaller flame beside the first, inside her
//!    burner's rim, so even she carries three lit parts.

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirshipBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, sphere, superellipsoid, sweep, torus, with_cut,
};
use super::{
    AirshipCraft, AirshipFeel, BalloonColours, HullPlan, HullProfile, MIN_DIM, balloon_colours,
    dim, flat_sweep,
};

/// Her envelope, crown down: an elliptical crown to her widest at 0.38 of
/// her height, then a near-straight taper to the throat - half the
/// blueprint's length tall, at a quarter of its fineness.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.38,
    parallel: 0.0,
    nose_power: 2.0,
    tail_power: 1.3,
    stations: &[
        0.0, 0.012, 0.035, 0.07, 0.12, 0.19, 0.27, 0.38, 0.48, 0.58, 0.67, 0.75, 0.82, 0.88, 0.92,
    ],
    len_scale: 0.50,
    fine_scale: 0.25,
};

/// Where her envelope opens, as a share of her height from the crown.
pub(super) const THROAT_X: f32 = 0.92;
const GORES: usize = 16;
const ENVELOPE_RES: u32 = 32;
const ENVELOPE_SAMPLES: u32 = 6;
/// The crown cap's share of the envelope's arc length.
const CROWN: f32 = 0.09;

/// The basket: its rim under the throat and its half-extents (m, prototype
/// frame).
const BASKET_DROP: f32 = 0.42;
pub(super) const BASKET_HALF: [f32; 3] = [0.23, 0.17, 0.23];
/// The burner over the basket's rim.
const BURNER_RISE: f32 = 0.20;
/// An ornate balloon's second flame: its size against her main one, and how
/// far off the burner's axis it stands - its base inside the burner's rim.
const TWIN_FLAME: f32 = 0.7;
const TWIN_FLAME_OUT: f32 = 0.035;
const FLAME: [(f32, f32); 4] = [(0.045, 0.0), (0.035, 0.05), (0.012, 0.11), (0.0, 0.15)];

pub(super) struct HotAirBalloon;

impl AirshipCraft for HotAirBalloon {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = balloon_colours(ctx);
        let side = dim(plan.len(0.02));
        let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
        let kids = &mut root.children;
        envelope(kids, plan, &c);
        basket(kids, plan, &c);
        dress(kids, plan, &c, ctx.ornateness);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // A drifter: 4.5 of drive over 0.6 tops out at 7.5 m/s, slow to turn
        // and slow to settle.
        AirshipFeel {
            mass_factor: 0.8,
            drive_accel: 4.5,
            yaw_accel: 3.5,
            linear_damping: 0.6,
            angular_damping: 3.0,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        let top = basket_top(plan);
        match aura {
            // Her burner's heat and sparks rise off its flame, into her
            // throat.
            ParticleAura::Embers | ParticleAura::Steam | ParticleAura::Exhaust => {
                [0.0, top + plan.len(BURNER_RISE + 0.20), 0.0]
            }
            _ => [
                0.0,
                top - plan.len(BASKET_HALF[1] * 2.0) - plan.len(0.05),
                0.0,
            ],
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Sail
    }
}

fn envelope(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BalloonColours) {
    let skin = |proud: f32, material: &crate::pds::texture::SovereignMaterialSettings| {
        sweep(
            &plan.upright(plan.len(proud)),
            ENVELOPE_RES,
            ENVELOPE_SAMPLES,
            material.clone(),
        )
    };
    kids.push(prim(skin(0.0, &c.gores[0]), [0.0; 3], id_quat()));
    // Her gores round her in turn, each colour a wedge laid a few
    // millimetres off the first.
    let w = 1.0 / GORES as f32;
    for (k, gore) in c.gores.iter().enumerate().skip(1) {
        for g in (k..GORES).step_by(c.gores.len()) {
            let a = g as f32 * w;
            kids.push(prim(
                with_cut(skin(0.003, gore), [a, a + w], [0.0, 1.0], 0.0),
                [0.0; 3],
                id_quat(),
            ));
        }
    }
    // The crown cap, and the scoop ring round the throat.
    kids.push(prim(
        with_cut(skin(0.007, &c.crown), [0.0, 1.0], [0.0, CROWN], 0.0),
        [0.0; 3],
        id_quat(),
    ));
    kids.push(prim(
        torus(plan.len(0.016), plan.radius(THROAT_X), c.leather.clone()),
        [0.0, plan.z(THROAT_X), 0.0],
        id_quat(),
    ));
}

/// The height of the basket's rim.
fn basket_top(plan: &HullPlan) -> f32 {
    plan.z(THROAT_X) - plan.len(BASKET_DROP)
}

fn cable(
    a: [f32; 3],
    b: [f32; 3],
    material: &crate::pds::texture::SovereignMaterialSettings,
) -> Generator {
    prim(
        sweep(&[(a, MIN_DIM), (b, MIN_DIM)], 6, 2, material.clone()),
        [0.0; 3],
        id_quat(),
    )
}

fn basket(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BalloonColours) {
    let yt = plan.z(THROAT_X);
    let rt = plan.radius(THROAT_X);
    let top = basket_top(plan);
    let [hx, hy, hz] = BASKET_HALF.map(|v| plan.len(v));
    kids.push(prim(
        superellipsoid([hx, hy, hz], 0.25, 0.3, 12, 20, c.wicker.clone()),
        [0.0, top - hy, 0.0],
        id_quat(),
    ));
    // The padded rim.
    kids.push(prim(
        cuboid([hx * 2.08, plan.len(0.03), hz * 2.08], c.leather.clone()),
        [0.0, top, 0.0],
        id_quat(),
    ));
    // Four cables from the throat's ring to the basket's corners, and the
    // burner frame: four uprights off the rim to the burner.
    let yb = top + plan.len(BURNER_RISE);
    for sx in [1.0f32, -1.0] {
        for sz in [1.0f32, -1.0] {
            let a = sz.atan2(sx);
            kids.push(cable(
                [
                    rt * a.cos() * 0.98,
                    yt + plan.len(0.005),
                    rt * a.sin() * 0.98,
                ],
                [sx * hx * 0.92, top - plan.len(0.01), sz * hz * 0.92],
                &c.dark,
            ));
            kids.push(cable(
                [sx * hx * 0.8, top + plan.len(0.01), sz * hz * 0.8],
                [sx * plan.len(0.05), yb, sz * plan.len(0.05)],
                &c.metal,
            ));
        }
    }
    kids.push(prim(
        cylinder(plan.len(0.07), plan.len(0.06), 14, c.metal.clone()),
        [0.0, yb, 0.0],
        id_quat(),
    ));
    kids.push(flame(plan, [0.0, yb + plan.len(0.02), 0.0], 1.0, c));
}

/// A burner's flame standing at `at`, `size` times her main one.
fn flame(plan: &HullPlan, at: [f32; 3], size: f32, c: &BalloonColours) -> Generator {
    let points: Vec<(f32, f32)> = FLAME
        .iter()
        .map(|&(r, h)| (plan.len(r * size), plan.len(h * size)))
        .collect();
    prim(lathe(&points, 12, true, c.flame.clone()), at, id_quat())
}

fn dress(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BalloonColours, tier: OrnatenessTier) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    let top = basket_top(plan);
    let [hx, _, hz] = BASKET_HALF.map(|v| plan.len(v));
    // Sandbags hung over the basket's sides, and a lantern on her -X side.
    for (sx, sz) in [(1.0f32, 0.45f32), (1.0, -0.45), (-1.0, 0.45), (-1.0, -0.45)] {
        kids.push(prim(
            superellipsoid(
                [plan.len(0.03), plan.len(0.045), plan.len(0.04)],
                0.7,
                0.7,
                8,
                10,
                c.sand.clone(),
            ),
            [sx * (hx + plan.len(0.018)), top - plan.len(0.07), sz * hz],
            id_quat(),
        ));
    }
    kids.push(prim(
        sphere(plan.len(0.025), 3, c.lamp.clone()),
        [-(hx + plan.len(0.012)), top - plan.len(0.09), 0.0],
        id_quat(),
    ));
    if tier == OrnatenessTier::Ornate {
        // A pennant on a short staff off the basket's corner, and a second,
        // smaller flame beside the first, inside the burner's rim.
        kids.push(cable(
            [hx * 0.9, top - plan.len(0.02), hz * 0.9],
            [hx * 1.15, top + plan.len(0.28), hz * 1.15],
            &c.metal,
        ));
        kids.push(flat_sweep(
            &[
                (
                    [hx * 1.15, top + plan.len(0.25), hz * 1.15],
                    plan.len(0.035),
                ),
                (
                    [hx * 1.15, top + plan.len(0.20), hz * 1.15 - plan.len(0.20)],
                    plan.len(0.012),
                ),
            ],
            10,
            2,
            0,
            0.08,
            &c.flag,
            None,
        ));
        kids.push(flame(
            plan,
            [
                plan.len(TWIN_FLAME_OUT),
                top + plan.len(BURNER_RISE + 0.02),
                0.0,
            ],
            TWIN_FLAME,
            c,
        ));
    }
}
