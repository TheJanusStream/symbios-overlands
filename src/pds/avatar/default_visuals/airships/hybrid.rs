//! The modern hybrid: the Airlander's line - a wide twin-lobed hull whose
//! lobes part amidships and meet in one nose and one tail, panels over each
//! lobe's crown (solar film on the solar schemes), a glass-nosed cabin under
//! her bow, four ducted fans - two on pylons by the cabin, two low on her
//! flanks aft - and canted twin tails (#1662, epic #1656).
//!
//! Prototyped beside the Zeppelin (`target/dump/airships-1656/hybrid.py`)
//! and ported number for number, the fan-out's six in one pass.
//!
//! # Where the port leaves the prototype, and why
//!
//! 1. The cabin's crown is buried below the CREASE between her lobes, where
//!    the prototype measured it from a single lobe's keel - a hand's breadth
//!    lower, so it barely touched her.
//! 2. The fans' blades stop short of their ducts' bore, where the
//!    prototype's tips cut a millimetre into the wall; and the forward pair
//!    hang a little lower and further out, where the prototype's ducts cut
//!    into the lobes over them.
//! 3. The stripe low on each lobe is the seeded accent; the windows are the
//!    fleet's window light.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};
use crate::seeded_defaults::{AirshipBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{
    cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z, rod, sphere, superellipsoid, sweep,
    with_cut, with_shape,
};
use super::{
    AirshipCraft, AirshipFeel, HullPlan, HullProfile, HybridColours, MIN_DIM, dim, flank_bands,
    flat_sweep, hybrid_colours,
};

/// Each lobe: a blunt nose, a long full girth and a full tail - 0.95 of
/// the blueprint's length at 0.86 of its fineness.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.30,
    parallel: 0.18,
    nose_power: 2.4,
    tail_power: 2.1,
    stations: &[
        0.0, 0.01, 0.03, 0.065, 0.11, 0.17, 0.25, 0.34, 0.44, 0.54, 0.64, 0.73, 0.81, 0.88, 0.94,
        1.0,
    ],
    len_scale: 0.95,
    fine_scale: 0.86,
};

/// Each lobe's axis off her centreline, in its own radius there - so the
/// lobes part amidships and meet in one nose and one tail.
pub(super) const LOBE: f32 = 0.50;
const HULL_RES: u32 = 32;
const HULL_SAMPLES: u32 = 6;

/// The cabin: her nose as a share, her length, greatest radius, width over
/// height, and how far her crown sits inside the crease (m, prototype frame).
const CAB_X: f32 = 0.10;
const CAB_R: f32 = 0.15;
const CAB_FLAT: f32 = 1.25;
const CAB_BURY: f32 = 0.05;
const CAB_WIDEST: f32 = 0.22;
const CAB_PROFILE: [(f32, f32); 8] = [
    (0.0, 0.05),
    (0.04, 0.10),
    (0.11, 0.135),
    (0.22, CAB_R),
    (0.48, CAB_R),
    (0.66, CAB_R * 0.9),
    (0.78, CAB_R * 0.66),
    (0.86, 0.05),
];

/// The ducted fans: the duct's radius, length and bore, the blades' length
/// and their middles' distance from the hub, and the turn (m, prototype
/// frame; degrees a second).
const DUCT_R: f32 = 0.12;
const DUCT_LEN: f32 = 0.12;
const DUCT_BORE: f32 = 0.86;
const BLADE_LEN: f32 = 0.08;
const BLADE_MID: f32 = 0.055;
const FAN_DEG_S: f32 = 1200.0;
/// The forward pair: how far out of the cabin's flank, and how far under
/// its centreline (m, prototype frame).
const FORE_FAN_OUT: f32 = 0.23;
const FORE_FAN_DROP: f32 = 0.05;
/// The aft pair's station, and its bearing off a lobe's widest line.
const AFT_FAN_X: f32 = 0.72;
const AFT_FAN_DEG: f32 = -50.0;

/// The tails: each fin's middle as a share and its half-extents.
const FIN_X: f32 = 0.86;
const FIN_HALF: [f32; 3] = [0.022, 0.22, 0.24];

pub(super) struct Hybrid;

impl AirshipCraft for Hybrid {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = hybrid_colours(ctx);
        let side = dim(plan.len(0.02));
        let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
        let kids = &mut root.children;
        hull(kids, plan, &c);
        cabin(kids, plan, &c);
        fans(kids, plan, &c);
        tail(kids, plan, &c);
        dress(kids, plan, &c, ctx.ornateness);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // The quickest of them: 8.5 of drive over 0.8 tops out at 10.6 m/s,
        // her fans vectoring her round.
        AirshipFeel {
            mass_factor: 1.4,
            drive_accel: 8.5,
            yaw_accel: 6.5,
            linear_damping: 0.8,
            angular_damping: 5.0,
        }
    }

    fn fx_mount(&self, _aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        let (zf, cy) = cab_axis(plan);
        [
            0.0,
            cy - plan.len(CAB_R) - plan.len(0.05),
            zf - plan.len(0.40),
        ]
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Electric
    }

    #[cfg(test)]
    fn lobes(&self, plan: &HullPlan, x: f32) -> Vec<[f32; 2]> {
        let off = lobe_x(plan, x);
        vec![[off, 0.0], [-off, 0.0]]
    }
}

/// A lobe's axis off her centreline at share `x`.
fn lobe_x(plan: &HullPlan, x: f32) -> f32 {
    LOBE * plan.radius(x)
}

/// One lobe's stations, `proud` (m) off its skin.
fn lobe_points(plan: &HullPlan, side: f32, proud: f32) -> Vec<([f32; 3], f32)> {
    plan.stations(0.0)
        .into_iter()
        .map(|([_, y, z], r)| ([side * LOBE * r, y, z], r + proud))
        .collect()
}

fn hull(kids: &mut Vec<Generator>, plan: &HullPlan, c: &HybridColours) {
    for side in [1.0f32, -1.0] {
        kids.push(prim(
            sweep(
                &lobe_points(plan, side, 0.0),
                HULL_RES,
                HULL_SAMPLES,
                c.hull.clone(),
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
    for (k, side) in [1.0f32, -1.0].into_iter().enumerate() {
        // The panels over each lobe's crown, and the stripe low on its outer
        // flank (+X on the +X lobe, -X on the other).
        kids.push(prim(
            with_cut(
                sweep(
                    &lobe_points(plan, side, plan.len(0.004)),
                    HULL_RES,
                    HULL_SAMPLES,
                    c.top.clone(),
                ),
                [0.17, 0.33],
                [0.14, 0.80],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
        kids.push(prim(
            with_cut(
                sweep(
                    &lobe_points(plan, side, plan.len(0.003)),
                    HULL_RES,
                    HULL_SAMPLES,
                    c.stripe.clone(),
                ),
                flank_bands(-14.0, -8.0)[k],
                [0.08, 0.86],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
}

/// `(z of her cabin's nose, its centreline's height)`: her crown CAB_BURY
/// inside the crease between her lobes at her widest station.
fn cab_axis(plan: &HullPlan) -> (f32, f32) {
    let zf = plan.z(CAB_X);
    let r = plan.radius(plan.x(zf - plan.len(CAB_WIDEST)));
    let crease = -(r * r - (LOBE * r).powi(2)).sqrt();
    (zf, crease + plan.len(CAB_BURY) - plan.len(CAB_R))
}

fn cab_points(plan: &HullPlan, proud: f32) -> Vec<([f32; 3], f32)> {
    let (zf, cy) = cab_axis(plan);
    CAB_PROFILE
        .iter()
        .map(|&(dz, r)| ([0.0, cy, zf - plan.len(dz)], plan.len(r + proud)))
        .collect()
}

fn cabin(kids: &mut Vec<Generator>, plan: &HullPlan, c: &HybridColours) {
    kids.push(flat_sweep(
        &cab_points(plan, 0.0),
        24,
        6,
        0,
        CAB_FLAT,
        &c.cabin,
        None,
    ));
    let skin = cab_points(plan, 0.003);
    // Her glass nose, wrapped round the front of the cabin, and the lit
    // windows down each side.
    kids.push(flat_sweep(
        &skin,
        24,
        6,
        0,
        CAB_FLAT,
        &c.glass,
        Some(([0.0, 0.5], [0.0, 0.18])),
    ));
    for cut in flank_bands(11.0, 29.0) {
        kids.push(flat_sweep(
            &skin,
            24,
            6,
            0,
            CAB_FLAT,
            &c.lamp,
            Some((cut, [0.22, 0.74])),
        ));
    }
}

/// A ducted fan at `at`: its duct along her, and five blades on a hub
/// turning inside it.
fn ducted_fan(
    kids: &mut Vec<Generator>,
    plan: &HullPlan,
    at: [f32; 3],
    rate: f32,
    c: &HybridColours,
) {
    kids.push(prim(
        with_cut(
            cylinder(plan.len(DUCT_R), plan.len(DUCT_LEN), 24, c.duct.clone()),
            [0.0, 1.0],
            [0.0, 1.0],
            DUCT_BORE,
        ),
        at,
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    let mut parts = vec![prim(
        sphere(plan.len(0.03), 3, c.metal.clone()),
        [0.0; 3],
        id_quat(),
    )];
    for k in 0..5 {
        let angle = (72.0 * k as f32).to_radians();
        let mid = plan.len(BLADE_MID);
        parts.push(prim(
            cuboid(
                [plan.len(BLADE_LEN), plan.len(0.026), MIN_DIM],
                c.dark.clone(),
            ),
            [angle.cos() * mid, angle.sin() * mid, 0.0],
            quat_xyzw(quat_z(angle)),
        ));
    }
    let side = dim(plan.len(0.02));
    let mut hub = prim(cuboid([side; 3], c.dark.clone()), at, id_quat());
    hub.children = parts;
    hub.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Constant { rate: Fp(rate) },
    ));
    kids.push(hub);
}

fn fans(kids: &mut Vec<Generator>, plan: &HullPlan, c: &HybridColours) {
    let (zf, cy) = cab_axis(plan);
    for side in [1.0f32, -1.0] {
        // Forward: on a pylon off the cabin's flank, low enough that its
        // duct turns clear of the lobe over it.
        let x = side * (plan.len(CAB_R * CAB_FLAT) + plan.len(FORE_FAN_OUT));
        let at = [x, cy - plan.len(FORE_FAN_DROP), zf - plan.len(0.36)];
        kids.push(flat_sweep(
            &[
                (
                    [side * plan.len(CAB_R * CAB_FLAT * 0.6), at[1], at[2]],
                    plan.len(0.045),
                ),
                (
                    [x - side * plan.len(DUCT_R * 0.8), at[1], at[2]],
                    plan.len(0.045),
                ),
            ],
            12,
            2,
            1,
            0.30,
            &c.cabin,
            None,
        ));
        ducted_fan(kids, plan, at, FAN_DEG_S * side, c);
        // Aft: low on the lobe's outer flank, on a strut.
        let za = plan.z(AFT_FAN_X);
        let ra = plan.radius(AFT_FAN_X);
        let lx = side * lobe_x(plan, AFT_FAN_X);
        let a = AFT_FAN_DEG.to_radians();
        let d = ra + plan.len(DUCT_R) + plan.len(0.03);
        let at = [lx + side * d * a.cos(), d * a.sin(), za];
        let inner = ra - plan.len(0.02);
        let outer = plan.len(DUCT_R * 0.85);
        kids.push(rod(
            [lx + side * inner * a.cos(), inner * a.sin(), za],
            [at[0] - side * outer * a.cos(), at[1] - outer * a.sin(), za],
            plan.len(0.022),
            c.cabin.clone(),
        ));
        ducted_fan(kids, plan, at, -FAN_DEG_S * side, c);
    }
}

fn tail(kids: &mut Vec<Generator>, plan: &HullPlan, c: &HybridColours) {
    // On each lobe a fin canted out 25 degrees from the vertical and one
    // 120 degrees down, tapered slabs turned out from the lobe's axis (a
    // turn of `deg` about +Z carries +Y to (-sin, cos)); the lower ones
    // carry her nav lights, red to port (+X).
    let [hx, hs, hc] = FIN_HALF.map(|v| plan.len(v));
    let zc = plan.z(FIN_X);
    let d = plan.len(0.05) + hs;
    for side in [1.0f32, -1.0] {
        let lx = side * lobe_x(plan, FIN_X);
        for (deg, lamp) in [(side * -25.0, None), (side * -120.0, Some(side))] {
            let r = deg.to_radians();
            kids.push(prim(
                with_shape(
                    superellipsoid([hx, hs, hc], 0.25, 0.45, 10, 20, c.fins.clone()),
                    [0.3, 0.45],
                    [0.0; 3],
                    [0.0, -plan.len(0.10)],
                ),
                [lx - r.sin() * d, r.cos() * d, zc],
                quat_xyzw(quat_z(r)),
            ));
            if let Some(side) = lamp {
                let tip = plan.len(0.05) + 2.0 * hs - plan.len(0.01);
                let light = if side > 0.0 { &c.port } else { &c.starboard };
                kids.push(prim(
                    sphere(plan.len(0.016), 3, light.clone()),
                    [
                        lx - r.sin() * tip,
                        r.cos() * tip,
                        zc - plan.len(0.10) + plan.len(0.01),
                    ],
                    id_quat(),
                ));
            }
        }
    }
}

fn dress(kids: &mut Vec<Generator>, plan: &HullPlan, c: &HybridColours, tier: OrnatenessTier) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    let (zf, cy) = cab_axis(plan);
    // A sensor dome under her cabin's chin, and a strobe on each lobe's
    // crown.
    kids.push(prim(
        sphere(plan.len(0.05), 3, c.glass.clone()),
        [
            0.0,
            cy - plan.len(CAB_R) + plan.len(0.005),
            zf - plan.len(0.16),
        ],
        id_quat(),
    ));
    for side in [1.0f32, -1.0] {
        kids.push(prim(
            sphere(plan.len(0.016), 3, c.white.clone()),
            [
                side * lobe_x(plan, 0.5),
                plan.radius(0.5) + plan.len(0.008),
                plan.z(0.5),
            ],
            id_quat(),
        ));
    }
    if tier == OrnatenessTier::Ornate {
        // Her air-cushion landing pads under each lobe.
        for side in [1.0f32, -1.0] {
            kids.push(prim(
                superellipsoid(
                    [plan.len(0.10), plan.len(0.05), plan.len(0.30)],
                    0.6,
                    0.8,
                    10,
                    16,
                    c.duct.clone(),
                ),
                [
                    side * lobe_x(plan, 0.55),
                    -plan.radius(0.55) - plan.len(0.01),
                    plan.z(0.55),
                ],
                id_quat(),
            ));
        }
    }
}
