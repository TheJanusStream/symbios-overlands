//! The open car's cockpit: the windscreen frame, the seat, the steering column
//! and its wheel - and on a tourer, the second row's bench.
//!
//! A closed car draws none of this: its cabin would hide every part of it
//! ([`super::hardtop`]).

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;

use super::super::super::common::{bevel, id_quat, prim, quat_x, quat_xyzw};
use super::super::plan::BodyPlan;
use super::super::{SkiffColours, dim};
use super::{TUB_HOLLOW, line, turned};

/// The screen frame, the seat, the column and the wheel, and a bench where the
/// body has a second row.
pub(super) fn build(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    // The screen stands at the cockpit's FORWARD lip, not at the cowl: on the
    // cowl it leaves half a metre of bare scuttle behind it and reads as a
    // hoop planted in the bonnet.
    let fz = plan.cockpit_z().1;
    let foot = plan.crown_at(fz) * 0.55;
    let post = plan.side_at(fz, foot) * 0.86;
    let (rake, top, r) = (l * 0.042, plan.screen_top(), l * 0.0105);
    // A CLOSED loop - post, header, post, sill rail - because an open U of
    // thin chrome reads as a roll hoop rather than as a screen frame. NO glass
    // in it: `SovereignMaterialSettings` has no alpha, so a pane renders as a
    // dark crate (#1359 rule 4).
    kids.push(line(
        &[
            ([-post, foot, fz + l * 0.004], r),
            ([-post * 0.97, top, fz - rake], r),
            ([0.0, top + l * 0.005, fz - rake * 1.10], r),
            ([post * 0.97, top, fz - rake], r),
            ([post, foot, fz + l * 0.004], r),
            ([0.0, foot - l * 0.004, fz + l * 0.010], r),
            ([-post, foot, fz + l * 0.004], r),
        ],
        6,
        c.brightwork.clone(),
    ));
    let sz = plan.seat_z();
    seat(kids, plan, c, sz);
    // Steering column, out of the SCUTTLE - which is what a column comes
    // through. Started from the footwell floor instead, it stands in mid-air.
    let cz = sz + 0.062 * l;
    let wx = -plan.half_width_at(cz) * 0.44;
    kids.push(line(
        &[
            (
                [
                    wx,
                    plan.sill_at(0.0) * TUB_HOLLOW + l * 0.022,
                    fz + l * 0.026,
                ],
                l * 0.0048,
            ),
            ([wx, l * 0.052, sz + 0.120 * l], l * 0.0048),
        ],
        6,
        c.machinery.clone(),
    ));
    kids.push(turned(
        &[
            (0.0, 0.0),
            (l * 0.085, 0.0),
            (l * 0.085, l * 0.006),
            (0.0, l * 0.006),
        ],
        20,
        false,
        c.machinery.clone(),
        [wx, l * 0.058, sz + 0.126 * l],
        quat_x(FRAC_PI_2 - 0.55),
    ));
    // The tourer's second row: the same seat on the station the body names,
    // and the only part that body adds (#1367).
    if let Some(bench) = plan.bench_z() {
        seat(kids, plan, c, bench);
    }
}

/// A Bevel back and a Bevel cushion down in the bored footwell, each sitting on
/// the bore's floor AT ITS OWN STATION. Taking the floor depth amidships
/// instead floats the back a finger clear of it, which is the same class of
/// mistake as a mount on a guessed fraction.
fn seat(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours, sz: f32) {
    let l = plan.length;
    let cz = sz + 0.062 * l;
    kids.push(prim(
        bevel(
            [
                dim(plan.half_width_at(sz) * 1.34),
                dim(l * 0.036),
                dim(l * 0.115),
            ],
            l * 0.018,
            4,
            c.leather.clone(),
        ),
        [
            0.0,
            plan.sill_at(sz) * TUB_HOLLOW + l * 0.075,
            sz - l * 0.018,
        ],
        quat_xyzw(quat_x(FRAC_PI_2 - 0.20)),
    ));
    kids.push(prim(
        bevel(
            [
                dim(plan.half_width_at(cz) * 1.34),
                dim(l * 0.030),
                dim(l * 0.125),
            ],
            l * 0.016,
            4,
            c.leather.clone(),
        ),
        [0.0, plan.sill_at(cz) * TUB_HOLLOW + l * 0.024, cz],
        id_quat(),
    ));
}
