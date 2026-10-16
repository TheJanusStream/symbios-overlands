//! The sky galleon: a fantasy sailing ship of the air - a wooden hull with a
//! raised sterncastle and lit stern windows, slung on rigging under a long
//! gasbag, a wing sail spread flat from each gunwale, a bowsprit and a stern
//! rudder (#1660, epic #1656).
//!
//! Prototyped beside the Zeppelin (`target/dump/airships-1656/galleon.py`)
//! and ported number for number, the fan-out's six in one pass.
//!
//! # Where the port leaves the prototype, and why
//!
//! 1. Her rigging's feet and her wing sails' roots are made fast a hair
//!    under her deck, inside her hull, where the prototype's stood a
//!    centimetre or six over its cut face - touching nothing, so her hull
//!    and her wings hung in the air beside her gasbag.
//! 2. The band round her gasbag's girth is the seeded accent, where the
//!    prototype drew it in each sketch's colour - the fleet's identity rule.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirshipBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{
    cuboid, id_quat, lathe, prim, quat_x, quat_xyzw, rod, sphere, superellipsoid, sweep, with_cut,
};
use super::{
    AirshipCraft, AirshipFeel, GalleonColours, HullPlan, HullProfile, MIN_DIM, dim, flat_sweep,
    galleon_colours,
};

/// Her gasbag: a rounded nose and a long full girth over a fuller tail -
/// 0.92 of the blueprint's length at 0.82 of its fineness.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.30,
    parallel: 0.26,
    nose_power: 2.1,
    tail_power: 1.9,
    stations: &[
        0.0, 0.01, 0.03, 0.065, 0.11, 0.17, 0.24, 0.32, 0.42, 0.52, 0.62, 0.71, 0.80, 0.88, 0.95,
        1.0,
    ],
    len_scale: 0.92,
    fine_scale: 0.82,
};

const BAG_RES: u32 = 32;
const BAG_SAMPLES: u32 = 6;
/// The band round her gasbag's girth amidships (shares of its arc length).
const BAND_RUN: [f32; 2] = [0.42, 0.50];

/// Her hull: stem and transom as shares of the gasbag's length, her deck at
/// the waist under the gasbag's lowest line, her half-beam and her side's
/// depth at the waist, and the deck's rise bow to stern (m, prototype frame).
const HULL_X: (f32, f32) = (0.18, 0.80);
const HANG: f32 = 0.42;
const HALF_BEAM: f32 = 0.27;
pub(super) const DEPTH: f32 = 0.30;
const SHEER: [f32; 7] = [0.10, 0.04, 0.0, 0.0, 0.02, 0.06, 0.12];
const CASTLE_L: f32 = 0.42;

/// The rigging's stations, the bearing it meets the gasbag at, and how far
/// under the deck its feet are made fast inside the hull.
const RIG_X: [f32; 5] = [0.24, 0.36, 0.48, 0.60, 0.72];
const RIG_DEG: f32 = -35.0;
const RIG_SET: f32 = 0.01;

/// The wing sails: where the root's leading edge stands (share), the span
/// out from her side, and the chords at root and tip (m, prototype frame).
const WING_X: f32 = 0.44;
const WING_SPAN: f32 = 0.78;
const WING_ROOT: f32 = 0.62;
const WING_TIP: f32 = 0.16;
const WING_SWEEP: f32 = 0.30;
const WING_SET: f32 = 0.01;

pub(super) struct SkyGalleon;

impl AirshipCraft for SkyGalleon {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = galleon_colours(ctx);
        let side = dim(plan.len(0.02));
        let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
        let kids = &mut root.children;
        gasbag(kids, plan, &c);
        hull(kids, plan, &c);
        rigging(kids, plan, &c);
        wings(kids, plan, &c);
        dress(kids, plan, &c, ctx.ornateness);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // A ship under sail: 6.5 of drive over 0.75 tops out at 8.7 m/s,
        // turning like a heavy hull.
        AirshipFeel {
            mass_factor: 1.2,
            drive_accel: 6.5,
            yaw_accel: 5.5,
            linear_damping: 0.75,
            angular_damping: 5.0,
        }
    }

    fn fx_mount(&self, _aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        // Under her keel, amidships.
        [
            0.0,
            deck_y(plan) - plan.len(DEPTH) - plan.len(0.05),
            plan.z(0.49),
        ]
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Sail
    }
}

fn gasbag(kids: &mut Vec<Generator>, plan: &HullPlan, c: &GalleonColours) {
    kids.push(prim(
        sweep(&plan.stations(0.0), BAG_RES, BAG_SAMPLES, c.bag.clone()),
        [0.0; 3],
        id_quat(),
    ));
    kids.push(prim(
        with_cut(
            sweep(
                &plan.stations(plan.len(0.003)),
                BAG_RES,
                BAG_SAMPLES,
                c.band.clone(),
            ),
            [0.0, 1.0],
            BAND_RUN,
            0.0,
        ),
        [0.0; 3],
        id_quat(),
    ));
    let cap: Vec<(f32, f32)> = [(0.030, 0.0), (0.016, 0.018), (0.0, 0.034)]
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    for (x, turn) in [(0.0f32, FRAC_PI_2), (1.0, -FRAC_PI_2)] {
        kids.push(prim(
            lathe(&cap, 12, true, c.gilt.clone()),
            [0.0, 0.0, plan.z(x) - plan.len(0.010).copysign(turn)],
            quat_xyzw(quat_x(turn)),
        ));
    }
}

fn deck_y(plan: &HullPlan) -> f32 {
    -plan.girth() - plan.len(HANG)
}

/// Her hull's seven stations stem to transom, `proud` (m, prototype frame)
/// off its skin: fine at the stem, broad aft to a wide transom, the deck
/// rising by the sheer.
fn hull_points(plan: &HullPlan, proud: f32) -> Vec<([f32; 3], f32)> {
    let (zb, zs) = (plan.z(HULL_X.0), plan.z(HULL_X.1));
    let yd = deck_y(plan);
    let hb = plan.len(HALF_BEAM);
    SHEER
        .iter()
        .enumerate()
        .map(|(k, &rise)| {
            let t = k as f32 / (SHEER.len() - 1) as f32;
            let w = hb * (0.18 + 0.82 * (PI * (0.08 + t * 0.62).min(1.0)).sin().powf(0.6));
            (
                [0.0, yd + plan.len(rise), zb + (zs - zb) * t],
                w + plan.len(proud),
            )
        })
        .collect()
}

fn hull(kids: &mut Vec<Generator>, plan: &HullPlan, c: &GalleonColours) {
    let hb = plan.len(HALF_BEAM);
    let depth = plan.len(DEPTH) / hb;
    kids.push(flat_sweep(
        &hull_points(plan, 0.0),
        24,
        5,
        1,
        depth,
        &c.hull,
        Some(([0.5, 1.0], [0.0, 1.0])),
    ));
    // A gilded wale along each side, just under the gunwale.
    for band in [[0.53, 0.57], [0.93, 0.97]] {
        kids.push(flat_sweep(
            &hull_points(plan, 0.004),
            24,
            5,
            1,
            depth,
            &c.gilt,
            Some((band, [0.02, 0.98])),
        ));
    }
    let yd = deck_y(plan);
    let (zb, zs) = (plan.z(HULL_X.0), plan.z(HULL_X.1));
    // The sterncastle: a raised cabin over her last fifth, a gilded roof,
    // and her lit stern windows looking aft.
    let castle_l = plan.len(CASTLE_L);
    let zc = zs + castle_l * 0.5;
    let ys = yd + plan.len(SHEER[SHEER.len() - 1]);
    kids.push(prim(
        superellipsoid(
            [hb * 0.82, plan.len(0.11), castle_l * 0.5],
            0.25,
            0.25,
            12,
            20,
            c.hull.clone(),
        ),
        [0.0, ys + plan.len(0.07), zc],
        id_quat(),
    ));
    kids.push(prim(
        cuboid(
            [hb * 1.30, plan.len(0.025), castle_l * 1.05],
            c.gilt.clone(),
        ),
        [0.0, ys + plan.len(0.18), zc],
        id_quat(),
    ));
    for k in 0..3 {
        kids.push(prim(
            cuboid([hb * 0.30, plan.len(0.06), MIN_DIM], c.lamp.clone()),
            [
                (k as f32 - 1.0) * hb * 0.48,
                ys + plan.len(0.08),
                zs + plan.len(0.004),
            ],
            id_quat(),
        ));
    }
    // The bowsprit, and the rudder on her transom.
    let yb = yd + plan.len(SHEER[0]);
    kids.push(rod(
        [0.0, yb - plan.len(0.02), zb - plan.len(0.10)],
        [0.0, yb + plan.len(0.10), zb + plan.len(0.42)],
        plan.len(0.018),
        c.spars.clone(),
    ));
    kids.push(flat_sweep(
        &[
            (
                [0.0, yd - plan.len(0.02), zs + plan.len(0.02)],
                plan.len(0.10),
            ),
            (
                [0.0, yd - plan.len(0.26), zs - plan.len(0.04)],
                plan.len(0.10),
            ),
        ],
        12,
        2,
        0,
        0.12,
        &c.spars,
        None,
    ));
}

fn rigging(kids: &mut Vec<Generator>, plan: &HullPlan, c: &GalleonColours) {
    let pts = hull_points(plan, 0.0);
    let n = pts.len() - 1;
    let a = RIG_DEG.to_radians();
    for x in RIG_X {
        // The gunwale's half-beam and height at this station, by the hull's
        // own sheer.
        let t = (x - HULL_X.0) / (HULL_X.1 - HULL_X.0);
        let i = ((t * n as f32) as usize).min(n - 1);
        let f = t * n as f32 - i as f32;
        let w = pts[i].1 + (pts[i + 1].1 - pts[i].1) * f;
        let y = pts[i].0[1] + (pts[i + 1].0[1] - pts[i].0[1]) * f;
        let z = plan.z(x);
        let r = plan.radius(x) - plan.len(0.012);
        for side in [1.0f32, -1.0] {
            kids.push(prim(
                sweep(
                    &[
                        ([side * w * 0.92, y - plan.len(RIG_SET), z], MIN_DIM),
                        ([side * r * a.cos(), r * a.sin(), z], MIN_DIM),
                    ],
                    6,
                    2,
                    c.rope.clone(),
                ),
                [0.0; 3],
                id_quat(),
            ));
        }
    }
}

fn wings(kids: &mut Vec<Generator>, plan: &HullPlan, c: &GalleonColours) {
    let yd = deck_y(plan);
    let zr = plan.z(WING_X);
    let hb = plan.len(HALF_BEAM);
    let sweep_aft = plan.len(WING_SWEEP);
    // The root sits a hair under the deck, so the sail meets the hull.
    let y = yd - plan.len(WING_SET);
    for side in [1.0f32, -1.0] {
        let root = side * hb * 0.7;
        let tip = side * (hb + plan.len(WING_SPAN));
        // A sail spread flat from the gunwale, its tip swept aft: a sweep
        // along the span, squashed thin across Y.
        kids.push(flat_sweep(
            &[
                (
                    [root, y, zr - plan.len(WING_ROOT) * 0.5],
                    plan.len(WING_ROOT) * 0.5,
                ),
                (
                    [
                        root + (tip - root) * 0.55,
                        y - plan.len(0.04),
                        zr - sweep_aft * 0.55 - plan.len(0.20),
                    ],
                    plan.len(0.22),
                ),
                (
                    [tip, y - plan.len(0.09), zr - sweep_aft - plan.len(0.10)],
                    plan.len(WING_TIP) * 0.5,
                ),
            ],
            16,
            6,
            1,
            0.06,
            &c.sail,
            None,
        ));
        // Its yard along the leading edge.
        kids.push(rod(
            [root, y + plan.len(0.012), zr + plan.len(0.02)],
            [tip, y - plan.len(0.08), zr - sweep_aft + plan.len(0.02)],
            plan.len(0.012),
            c.spars.clone(),
        ));
    }
}

fn dress(kids: &mut Vec<Generator>, plan: &HullPlan, c: &GalleonColours, tier: OrnatenessTier) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    let yd = deck_y(plan);
    let (zb, zs) = (plan.z(HULL_X.0), plan.z(HULL_X.1));
    let ys = yd + plan.len(SHEER[SHEER.len() - 1]);
    let yb = yd + plan.len(SHEER[0]);
    // Lanterns: one on the sterncastle's roof, one at the bowsprit's foot.
    kids.push(prim(
        sphere(plan.len(0.026), 3, c.lamp.clone()),
        [0.0, ys + plan.len(0.21), zs + plan.len(0.03)],
        id_quat(),
    ));
    kids.push(prim(
        sphere(plan.len(0.022), 3, c.lamp.clone()),
        [0.0, yb + plan.len(0.02), zb + plan.len(0.02)],
        id_quat(),
    ));
    if tier == OrnatenessTier::Ornate {
        // A gilded figurehead under the bowsprit, and a pennant streaming
        // aft and down off the gasbag's tail.
        kids.push(prim(
            superellipsoid(
                [plan.len(0.035), plan.len(0.06), plan.len(0.05)],
                0.8,
                0.8,
                10,
                12,
                c.gilt.clone(),
            ),
            [0.0, yb - plan.len(0.03), zb + plan.len(0.03)],
            id_quat(),
        ));
        let z = plan.z(1.0);
        kids.push(flat_sweep(
            &[
                ([0.0, 0.0, z + plan.len(0.02)], plan.len(0.04)),
                ([0.0, -plan.len(0.10), z - plan.len(0.20)], plan.len(0.026)),
                ([0.0, -plan.len(0.16), z - plan.len(0.40)], plan.len(0.012)),
            ],
            12,
            4,
            0,
            0.08,
            &c.band,
            None,
        ));
    }
}
