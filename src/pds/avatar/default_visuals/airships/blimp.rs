//! The blimp: a short fat non-rigid envelope, a broad band down each flank
//! with the owner's pinstripe along it and, on an adorned ship, the lit night
//! sign in it, a glazed car snug under the belly with an outrigger engine
//! either side, and an X tail - the Goodyear GZ-20's line (#1658, epic
//! #1656).
//!
//! Prototyped in generator JSON beside the Zeppelin's
//! (`target/dump/airships-1656/blimp.py`) and ported number for number, as
//! the six fan-out types were all built in one pass at the owner's word -
//! their review follows rather than precedes the port.
//!
//! # Where the port leaves the prototype, and why
//!
//! 1. The band crosses her widest line, so on her `-X` flank it is two
//!    wedges either side of the sweep's seam ([`flank_bands`]); the
//!    prototype drew one that started below `0` turns, which the sanitiser
//!    clamped - her `-X` band stopped at her widest line.
//! 2. The camera pod's tally light sits half sunk in the pod's face, where
//!    the prototype hid it inside her car's keel.
//! 3. The pinstripe is the seeded accent; the windows and the sign are the
//!    fleet's window light.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};
use crate::seeded_defaults::{AirshipBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{
    cuboid, id_quat, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_z, sphere, superellipsoid,
    sweep, with_cut, with_shape,
};
use super::{
    AirshipCraft, AirshipFeel, BlimpColours, HullPlan, HullProfile, MIN_DIM, blimp_colours, dim,
    flank_bands, flat_sweep,
};

/// Her envelope: an elliptical nose over 0.36 of her length, a short full
/// girth, and a full, rounded tail - 0.86 of the blueprint's length at 0.8 of
/// its fineness, a short fat teardrop.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.36,
    parallel: 0.06,
    nose_power: 2.0,
    tail_power: 2.4,
    stations: &[
        0.0, 0.01, 0.03, 0.065, 0.11, 0.17, 0.25, 0.36, 0.45, 0.55, 0.65, 0.74, 0.82, 0.89, 0.95,
        1.0,
    ],
    len_scale: 0.86,
    fine_scale: 0.80,
};

const HULL_RES: u32 = 32;
const HULL_SAMPLES: u32 = 6;

/// The band down each flank, about her widest line, and its run; the
/// pinstripe along its top edge; the night sign inside it.
const BAND_DEG: (f32, f32) = (-16.0, 18.0);
const BAND_RUN: [f32; 2] = [0.16, 0.70];
const STRIPE_DEG: (f32, f32) = (18.0, 22.0);
const SIGN_DEG: (f32, f32) = (-10.0, 12.0);
const SIGN_RUN: [f32; 2] = [0.22, 0.62];
const NOSE_CAP: f32 = 0.035;

/// The car: her nose as a share of the envelope, her greatest radius, her
/// width over her height, how far her crown sits inside the envelope at her
/// widest station, and her stations (m, prototype frame).
const CAR_X: f32 = 0.30;
pub(super) const CAR_R: f32 = 0.14;
const CAR_FLAT: f32 = 0.88;
const CAR_BURY: f32 = 0.045;
const CAR_WIDEST: f32 = 0.18;
const CAR_PROFILE: [(f32, f32); 8] = [
    (0.00, 0.035),
    (0.03, 0.090),
    (0.09, 0.126),
    (0.18, CAR_R),
    (0.40, CAR_R),
    (0.55, CAR_R * 0.95),
    (0.68, CAR_R * 0.74),
    (0.80, 0.05),
];

/// The engines: each nacelle's axis off her centreline, its nose aft of the
/// car's nose, its length and radius, and the airscrew's (m, prototype frame).
const ENGINE_OUT: f32 = 0.30;
const ENGINE_AFT: f32 = 0.46;
const NACELLE_L: f32 = 0.24;
const NACELLE_R: f32 = 0.050;
const PROP_R: f32 = 0.105;
const PROP_DEG_S: f32 = 1000.0;

/// The X tail: each fin's middle as a share, its half-extents (thickness,
/// half span, half root chord), its root's distance from the axis and how
/// far aft its tip is swept (m, prototype frame).
const FIN_X: f32 = 0.835;
const FIN_HALF: [f32; 3] = [0.024, 0.20, 0.25];
const FIN_IN: f32 = 0.06;
const FIN_SWEEP: f32 = 0.12;

pub(super) struct Blimp;

impl AirshipCraft for Blimp {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = blimp_colours(ctx);
        let side = dim(plan.len(0.02));
        let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
        let kids = &mut root.children;
        envelope(kids, plan, &c, ctx.ornateness);
        car(kids, plan, &c);
        engines(kids, plan, &c);
        tail(kids, plan, &c);
        dress(kids, plan, &c, ctx.ornateness);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // Lighter and quicker to turn than the Zeppelin, a little more
        // damped: 7.5 of drive over 0.85 tops out at 8.8 m/s.
        AirshipFeel {
            mass_factor: 0.9,
            drive_accel: 7.5,
            yaw_accel: 8.0,
            linear_damping: 0.85,
            angular_damping: 4.5,
        }
    }

    fn fx_mount(&self, _aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        // Everything issues from under her car.
        let (zf, cy) = car_axis(plan);
        [
            0.0,
            cy - plan.len(CAR_R) - plan.len(0.05),
            zf - plan.len(0.30),
        ]
    }

    fn propulsion(&self) -> Propulsion {
        // Two piston engines turning airscrews, as the GZ-20's did.
        Propulsion::Propeller
    }
}

fn envelope(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BlimpColours, tier: OrnatenessTier) {
    kids.push(prim(
        sweep(
            &plan.stations(0.0),
            HULL_RES,
            HULL_SAMPLES,
            c.envelope.clone(),
        ),
        [0.0; 3],
        id_quat(),
    ));
    let skin = |material: &crate::pds::texture::SovereignMaterialSettings,
                proud: f32,
                cut: [f32; 2],
                run: [f32; 2]| {
        prim(
            with_cut(
                sweep(
                    &plan.stations(plan.len(proud)),
                    HULL_RES,
                    HULL_SAMPLES,
                    material.clone(),
                ),
                cut,
                run,
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        )
    };
    kids.push(skin(&c.nose, 0.003, [0.0, 1.0], [0.0, NOSE_CAP]));
    let cone: Vec<(f32, f32)> = [(0.036, 0.0), (0.026, 0.016), (0.0, 0.034)]
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    kids.push(prim(
        lathe(&cone, 14, true, c.metal.clone()),
        [0.0, 0.0, plan.z(0.0) - plan.len(0.016)],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    for cut in flank_bands(BAND_DEG.0, BAND_DEG.1) {
        kids.push(skin(&c.band, 0.003, cut, BAND_RUN));
    }
    for cut in flank_bands(STRIPE_DEG.0, STRIPE_DEG.1) {
        kids.push(skin(&c.stripe, 0.004, cut, BAND_RUN));
    }
    if tier != OrnatenessTier::Plain {
        for cut in flank_bands(SIGN_DEG.0, SIGN_DEG.1) {
            kids.push(skin(&c.lamp, 0.005, cut, SIGN_RUN));
        }
    }
    kids.push(prim(
        sphere(plan.len(0.016), 3, c.white.clone()),
        [0.0, 0.0, plan.z(1.0) - plan.len(0.004)],
        id_quat(),
    ));
}

/// `(z of her car's nose, its centreline's height)`: her crown CAR_BURY
/// inside the envelope at her widest station.
fn car_axis(plan: &HullPlan) -> (f32, f32) {
    let zf = plan.z(CAR_X);
    let widest = zf - plan.len(CAR_WIDEST);
    let y = -plan.radius(plan.x(widest)) + plan.len(CAR_BURY) - plan.len(CAR_R);
    (zf, y)
}

fn car_points(plan: &HullPlan, proud: f32) -> Vec<([f32; 3], f32)> {
    let (zf, cy) = car_axis(plan);
    CAR_PROFILE
        .iter()
        .map(|&(dz, r)| ([0.0, cy, zf - plan.len(dz)], plan.len(r + proud)))
        .collect()
}

fn car(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BlimpColours) {
    kids.push(flat_sweep(
        &car_points(plan, 0.0),
        24,
        6,
        0,
        CAR_FLAT,
        &c.car,
        None,
    ));
    let skin = car_points(plan, 0.003);
    // The windscreen over her bow, and the cabin's lit windows down each
    // flank.
    kids.push(flat_sweep(
        &skin,
        24,
        6,
        0,
        CAR_FLAT,
        &c.glass,
        Some(([0.06, 0.44], [0.0, 0.16])),
    ));
    for cut in flank_bands(2.0, 34.0) {
        kids.push(flat_sweep(
            &skin,
            24,
            6,
            0,
            CAR_FLAT,
            &c.lamp,
            Some((cut, [0.18, 0.78])),
        ));
    }
    // Nav lights low on her flanks under the windscreen: red to port (+X).
    let (zf, cy) = car_axis(plan);
    for (side, lamp) in [(1.0, &c.port), (-1.0, &c.starboard)] {
        kids.push(prim(
            sphere(plan.len(0.015), 3, lamp.clone()),
            [
                side * plan.len(CAR_R * CAR_FLAT - 0.004),
                cy - plan.len(0.03),
                zf - plan.len(0.10),
            ],
            id_quat(),
        ));
    }
}

fn engines(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BlimpColours) {
    let (zf, cy) = car_axis(plan);
    for side in [1.0f32, -1.0] {
        let x = side * plan.len(ENGINE_OUT);
        let y = cy + plan.len(0.02);
        let z0 = zf - plan.len(ENGINE_AFT);
        let nacelle: Vec<([f32; 3], f32)> = [
            (0.0, 0.014),
            (0.03, 0.036),
            (0.08, NACELLE_R),
            (0.15, NACELLE_R * 0.95),
            (0.21, NACELLE_R * 0.72),
            (NACELLE_L, 0.020),
        ]
        .iter()
        .map(|&(dz, r)| ([x, y, z0 - plan.len(dz)], plan.len(r)))
        .collect();
        kids.push(prim(
            sweep(&nacelle, 16, 4, c.car.clone()),
            [0.0; 3],
            id_quat(),
        ));
        // The outrigger: a faired strut from inside the car to inside the
        // nacelle.
        let zs = z0 - plan.len(0.10);
        kids.push(flat_sweep(
            &[
                (
                    [side * plan.len(CAR_R * CAR_FLAT * 0.5), y, zs],
                    plan.len(0.05),
                ),
                ([x, y, zs], plan.len(0.05)),
            ],
            12,
            2,
            1,
            0.28,
            &c.car,
            None,
        ));
        kids.push(airscrew(
            plan,
            [x, y, z0 - plan.len(NACELLE_L + 0.008)],
            -PROP_DEG_S * side,
            c,
        ));
    }
}

/// A three-blade pusher on a hidden hub, turning about +Z, drawn at rest
/// with a blade upright.
fn airscrew(plan: &HullPlan, at: [f32; 3], rate: f32, c: &BlimpColours) -> Generator {
    let mut parts = vec![prim(
        sphere(plan.len(0.022), 3, c.metal.clone()),
        [0.0; 3],
        id_quat(),
    )];
    let root = 0.014;
    let mid = plan.len(root + PROP_R) * 0.5;
    for k in 0..3 {
        let angle = (90.0 + 120.0 * k as f32).to_radians();
        parts.push(prim(
            cuboid(
                [plan.len(PROP_R - root), plan.len(0.024), MIN_DIM],
                c.dark.clone(),
            ),
            [angle.cos() * mid, angle.sin() * mid, 0.0],
            quat_xyzw(quat_mul(quat_z(angle), quat_x(18f32.to_radians()))),
        ));
    }
    let side = dim(plan.len(0.02));
    let mut hub = prim(cuboid([side; 3], c.dark.clone()), at, id_quat());
    hub.children = parts;
    hub.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Constant { rate: Fp(rate) },
    ));
    hub
}

fn tail(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BlimpColours) {
    // Four fins on the diagonals: tapered slabs whose span runs out along a
    // turn of `deg` about +Z (which carries +Y to (-sin, cos)), their roots
    // deep in the tail, their tips swept aft. A slab's extents are its own,
    // with no node scale, so turning it is a similarity.
    let [hx, hs, hc] = FIN_HALF.map(|v| plan.len(v));
    let zc = plan.z(FIN_X);
    let d = plan.len(FIN_IN) + hs;
    for deg in [-45.0f32, 45.0, 135.0, -135.0] {
        let r = deg.to_radians();
        kids.push(prim(
            with_shape(
                superellipsoid([hx, hs, hc], 0.25, 0.45, 10, 20, c.fins.clone()),
                [0.3, 0.48],
                [0.0; 3],
                [0.0, -plan.len(FIN_SWEEP)],
            ),
            [-r.sin() * d, r.cos() * d, zc],
            quat_xyzw(quat_z(r)),
        ));
    }
}

fn dress(kids: &mut Vec<Generator>, plan: &HullPlan, c: &BlimpColours, tier: OrnatenessTier) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    // A red beacon on her crown, amidships.
    kids.push(prim(
        sphere(plan.len(0.02), 3, c.beacon.clone()),
        [0.0, plan.radius(0.40) + plan.len(0.006), plan.z(0.40)],
        id_quat(),
    ));
    if tier == OrnatenessTier::Ornate {
        // The broadcast camera pod under her car's bow: the pod, its lens
        // and its red tally light half sunk in its face.
        let (zf, cy) = car_axis(plan);
        let y = cy - plan.len(CAR_R) - plan.len(0.025);
        let zp = zf - plan.len(0.14);
        kids.push(prim(
            sphere(plan.len(0.05), 3, c.dark.clone()),
            [0.0, y, zp],
            id_quat(),
        ));
        kids.push(prim(
            sphere(plan.len(0.03), 3, c.glass.clone()),
            [0.0, y - plan.len(0.012), zp + plan.len(0.035)],
            id_quat(),
        ));
        kids.push(prim(
            sphere(dim(plan.len(0.012)), 3, c.beacon.clone()),
            [0.0, y + plan.len(0.025), zp + plan.len(0.043)],
            id_quat(),
        ));
    }
}
