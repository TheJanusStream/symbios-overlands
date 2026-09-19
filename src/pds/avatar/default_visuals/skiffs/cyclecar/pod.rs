//! The cyclecar's pod: one full barrel over the plan's stations, the window
//! band lying in its shell, the accent strip along each flank, and the lamps
//! faired into its nose and wrapped over its tail.
//!
//! Every part here is read off the pod's own surface
//! ([`CyclecarPlan::surface`], [`CyclecarPlan::over_pod`]), so nothing can
//! float off it or sink into it at any size.

use std::f32::consts::PI;

use crate::pds::avatar::livery::CyclecarColours;
use crate::pds::generator::Generator;

use super::{BAND_RUN, CyclecarPlan, PROUD, along_z, line, solid, sweep};

/// The pod: ONE full barrel over the plan's stations, in the scheme - or, on
/// a two-tone, an upper half-pipe in the scheme's colour and a lower one in
/// its second: what "over black" means on a machine with no wings.
pub(super) fn pod(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let path = plan.over_pod(plan.tail_z(), plan.nose_z(), 1.0);
    match &c.lower {
        None => kids.push(sweep(
            &path,
            28,
            plan.scale(),
            [0.0, 1.0],
            0.0,
            c.body.clone(),
        )),
        Some(lower) => {
            kids.push(sweep(
                &path,
                28,
                plan.scale(),
                [0.0, 0.5],
                0.0,
                c.body.clone(),
            ));
            kids.push(sweep(
                &path,
                28,
                plan.scale(),
                [0.5, 1.0],
                0.0,
                lower.clone(),
            ));
        }
    }
}

/// The side windows' angular sectors, as fractions of the turn round the
/// pod from its `+x` flank over the top: the upper flank, 36-68 degrees
/// above the widest line on each side, where a full barrel still stands near
/// upright.
const SIDE_WINDOWS: [[f32; 2]; 2] = [[0.10, 0.19], [0.31, 0.40]];

/// The window band IN the pod shell: two side sectors of a second sweep over
/// the pod's own stations, a windscreen sector across the top at the band's
/// forward end, and a rear-light sector across it at the after end - each
/// standing [`PROUD`] of the pod, which is what keeps its edge clean.
pub(super) fn band(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let (z0, z1) = (plan.at(BAND_RUN.0), plan.at(BAND_RUN.1));
    for cut in SIDE_WINDOWS {
        kids.push(sweep(
            &plan.over_pod(z0, z1, PROUD),
            12,
            plan.scale(),
            cut,
            0.0,
            c.glass.clone(),
        ));
    }
    let screen = z1 + plan.at(0.012);
    kids.push(sweep(
        &plan.over_pod(screen, screen + plan.at(0.050), PROUD),
        16,
        plan.scale(),
        [0.10, 0.40],
        0.0,
        c.glass.clone(),
    ));
    let rear = z0 - plan.at(0.012);
    kids.push(sweep(
        &plan.over_pod(rear - plan.at(0.040), rear, PROUD),
        16,
        plan.scale(),
        [0.14, 0.36],
        0.0,
        c.glass.clone(),
    ));
}

/// The accent strip's angle under the widest line (rad), its radius (of the
/// length) and its run along the pod (fractions of the length).
const STRIP_ANG: f32 = -0.10;
const STRIP_R: f32 = 0.0070;
const STRIP_RUN: (f32, f32) = (-0.440, 0.440);

/// The accent strip, one line a side ON the pod's surface a little under its
/// widest line: the identity trim, lit on a luminous kit.
pub(super) fn strip(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let r = plan.at(STRIP_R);
    for s in [-1.0f32, 1.0] {
        let ang = if s > 0.0 { STRIP_ANG } else { PI - STRIP_ANG };
        let pts: Vec<([f32; 3], f32)> = plan
            .run(plan.at(STRIP_RUN.0), plan.at(STRIP_RUN.1))
            .into_iter()
            .map(|(p, _)| (plan.surface(p[2], ang), r))
            .collect();
        kids.push(line(&pts, 6, &c.trim));
    }
}

/// Two turned headlamps faired into the nose, lit with the tertiary's light
/// as the roadster's are; and a tail light bar wrapped over the tail, facing
/// the chase camera.
pub(super) fn lamps(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let l = plan.length;
    let k = l / 2.7 * 0.70;
    let zl = plan.at(0.455);
    let shell = [
        (0.0, -0.110 * k),
        (0.045 * k, -0.090 * k),
        (0.080 * k, -0.035 * k),
        (0.090 * k, 0.020 * k),
        (0.090 * k, 0.045 * k),
        (0.080 * k, 0.052 * k),
    ];
    let lens = [(0.0, 0.0), (0.077 * k, 0.0), (0.077 * k, 0.010 * k)];
    let ang = 0.20f32;
    for s in [-1.0f32, 1.0] {
        let p = plan.surface(zl, if s > 0.0 { ang } else { PI - ang });
        let at = [p[0] * 0.80, p[1], zl];
        kids.push(solid(&shell, 18, true, &c.body, at, along_z()));
        kids.push(solid(
            &lens,
            16,
            false,
            &c.lamp,
            [at[0], at[1], at[2] + 0.052 * k],
            along_z(),
        ));
    }
    let zt = plan.at(-0.420);
    let bar: Vec<([f32; 3], f32)> = [0.30f32, 0.95, PI / 2.0, PI - 0.95, PI - 0.30]
        .into_iter()
        .map(|a| (plan.surface(zt, a), plan.at(0.011)))
        .collect();
    kids.push(line(&bar, 6, &c.tail_lamp));
}
