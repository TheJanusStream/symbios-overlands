//! The cabin and the nose (#1630 Q-A): the cabin one rounded box, the nose's
//! last section carried aft to her ramp; the nose a sweep from its tip into
//! the cabin's front, its glazing laid on it - the windscreen over its top
//! and the chin windows under it; and down each of the cabin's flanks a row
//! of lit windows and, under them, the cheat line.
//!
//! Every skin is a second sweep over the nose's OWN stations, a few
//! millimetres proud, trimmed fore and aft by arc length and round the
//! section by angle (`0` is `+X`, `0.25` the crown, `0.5` is `-X`) - each set
//! of bands mirror-symmetric, so the same whichever way round a sweep's frame
//! turns: the utility's skins (#1613).

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::super::common::{
    cuboid, id_quat, prim, quat_x, quat_xyzw, spine, superellipsoid, with_cut,
};
use super::super::{PodPlan, TandemColours, dim};
use super::{CABIN_FRONT, CABIN_REAR, at_drawn, cabin_section};

/// The cabin's two exponents - its profile along her and its section across
/// her, both boxy - and its facets: latitudes nose to ramp, longitudes
/// round the section.
const CABIN_NS: f32 = 0.22;
const CABIN_EW: f32 = 0.30;
const CABIN_LAT: u32 = 24;
const CABIN_LON: u32 = 36;

/// The facets round the nose's section, and round a skin's - a skin's are
/// finer, so its cut edges stay crisp against the nose.
const NOSE_RES: u32 = 28;
const SKIN_RES: u32 = 32;

/// The cabin: one rounded box from inside the nose to her ramp.
pub(super) fn cabin(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (cy, w, h) = cabin_section(plan);
    let rear = plan.stretch(CABIN_REAR);
    // A quarter turn about +X lays the superellipsoid's north-south axis
    // along her, nose ahead, and its section across her, upright.
    kids.push(prim(
        superellipsoid(
            [
                plan.len(w),
                plan.len((CABIN_FRONT - rear) * 0.5),
                plan.len(h),
            ],
            CABIN_NS,
            CABIN_EW,
            CABIN_LAT,
            CABIN_LON,
            c.body.clone(),
        ),
        at_drawn(plan, 0.0, cy, (CABIN_FRONT + rear) * 0.5),
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
}

/// The cabin's drawn section at station `z` (prototype frame): its
/// centreline height, its half-width and half-height, and how much of
/// either its superellipsoid keeps there - 1 amidships, falling toward her
/// ends, 0 past them. The superellipsoid's own axes, as the quarter turn
/// lays them: north and south along her, the section across her and
/// upright.
fn drawn_section(plan: &PodPlan, z: f32) -> (f32, f32, f32, f32) {
    let (cy, w, h) = cabin_section(plan);
    let rear = plan.stretch(CABIN_REAR);
    let (mid, half) = ((CABIN_FRONT + rear) * 0.5, (CABIN_FRONT - rear) * 0.5);
    let along = 1.0 - term((plan.stretch(z) - mid) / half, CABIN_NS);
    (cy, w, h, along.max(0.0).powf(CABIN_NS / CABIN_EW))
}

/// A superellipsoid's implicit term: `v`, a share of its half-extent, raised
/// to `2 / e`.
fn term(v: f32, e: f32) -> f32 {
    v.abs().powf(2.0 / e)
}

/// How far out from her centreline the cabin is drawn at height `y` and
/// station `z` (prototype frame), its own superellipsoid solved for its
/// half-width there: under the section's `w` wherever it rounds in, toward
/// her belly and her ends - where a part set off `w` stood clear of it
/// (#1577). 0 where the cabin does not reach.
pub(super) fn cabin_flank(plan: &PodPlan, y: f32, z: f32) -> f32 {
    let (cy, w, h, kept) = drawn_section(plan, z);
    w * (kept - term((y - cy) / h, CABIN_EW))
        .max(0.0)
        .powf(CABIN_EW * 0.5)
}

/// How high the cabin is drawn at `x` out from her centreline and station
/// `z` (prototype frame), its own superellipsoid solved for its top there:
/// under the section's roof wherever it rounds in, toward her flanks and
/// her ends (#1577). Her centreline height where the cabin does not reach.
pub(super) fn cabin_roof(plan: &PodPlan, x: f32, z: f32) -> f32 {
    let (cy, w, h, kept) = drawn_section(plan, z);
    cy + h * (kept - term(x / w, CABIN_EW)).max(0.0).powf(CABIN_EW * 0.5)
}

/// A sweep over the nose's own stations, `proud` (prototype metres) over
/// it, with the nose's section scale.
fn over_nose(
    plan: &PodPlan,
    proud: f32,
    res: u32,
    material: SovereignMaterialSettings,
) -> Generator {
    let mut g = prim(
        spine(&plan.pod_sweep(proud), res, material),
        [0.0; 3],
        id_quat(),
    );
    g.transform.scale = Fp3([1.0, plan.section(), 1.0]);
    g
}

/// The nose: one sweep, its tip to inside the cabin's front.
pub(super) fn nose(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    kids.push(over_nose(plan, 0.0, NOSE_RES, c.body.clone()));
}

/// A skin laid on the nose between prototype stations `z0 < z1`, one sweep
/// per angular band.
fn skin(
    kids: &mut Vec<Generator>,
    plan: &PodPlan,
    (z0, z1): (f32, f32),
    bands: &[(f32, f32)],
    material: &SovereignMaterialSettings,
    proud: f32,
) {
    // The sweep runs tip first, so the forward station is the smaller
    // fraction.
    let (t0, t1) = (plan.along(z1), plan.along(z0));
    for &band in bands {
        let mut g = over_nose(plan, proud, SKIN_RES, material.clone());
        g.kind = with_cut(g.kind, [band.0, band.1], [t0, t1], 0.0);
        kids.push(g);
    }
}

/// The nose's glazing, in the prototype's order.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    // The windscreen: one wrap over the nose's top and round its cheeks.
    skin(kids, plan, (1.20, 1.565), &[(0.04, 0.46)], &c.glass, 0.006);
    // The chin windows under it, a keel strip left between.
    skin(
        kids,
        plan,
        (1.28, 1.52),
        &[(0.58, 0.70), (0.80, 0.92)],
        &c.glass,
        0.007,
    );
}

/// The cabin's windows (prototype frame): their stations nose to ramp, all
/// stretched; each one's size; how far inside the flank its middle stands,
/// and how far over the cabin's centreline.
const WINDOWS_Z: [f32; 6] = [0.62, 0.36, 0.10, -0.16, -0.42, -0.68];
const WINDOW: [f32; 3] = [0.030, 0.055, 0.055];
const WINDOW_IN: f32 = 0.012;
const WINDOW_RISE: f32 = 0.06;

/// A row of lit windows down each flank, port (`+X`) first at each
/// station.
pub(super) fn windows(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (cy, w, _) = cabin_section(plan);
    for z in WINDOWS_Z {
        for side in [1.0f32, -1.0] {
            kids.push(prim(
                cuboid(WINDOW.map(|v| plan.len(v)), c.lamp.clone()),
                plan.at(side * (w - WINDOW_IN), cy + WINDOW_RISE, z),
                id_quat(),
            ));
        }
    }
}

/// The cheat line (prototype frame): its thickness and height, how far
/// outside the flank its middle stands and how far under the cabin's
/// centreline, and how far short of the cabin's front and of its rear it
/// stops - the rear's RIGID off the stretched rear.
const STRIPE_THICK: f32 = 0.012;
const STRIPE_H: f32 = 0.032;
const STRIPE_OUT: f32 = 0.002;
const STRIPE_DROP: f32 = 0.07;
const STRIPE_FORE: f32 = 0.06;
const STRIPE_AFT: f32 = 0.12;

/// The cheat line down each flank, under the windows, the cabin's front to
/// its ramp - the seed's accent, the fleet's identity slot - port first.
pub(super) fn cheat_line(kids: &mut Vec<Generator>, plan: &PodPlan, c: &TandemColours) {
    let (cy, w, _) = cabin_section(plan);
    let (fore, aft) = (
        CABIN_FRONT - STRIPE_FORE,
        plan.stretch(CABIN_REAR) + STRIPE_AFT,
    );
    for side in [1.0f32, -1.0] {
        kids.push(prim(
            cuboid(
                [
                    dim(plan.len(STRIPE_THICK)),
                    plan.len(STRIPE_H),
                    plan.len(fore - aft),
                ],
                c.stripe.clone(),
            ),
            at_drawn(
                plan,
                side * (w + STRIPE_OUT),
                cy - STRIPE_DROP,
                (fore + aft) * 0.5,
            ),
            id_quat(),
        ));
    }
}
