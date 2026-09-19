//! The chariot: two high wheels, a D-shaped car open at the back, and a pole
//! rising to a yoke at the nose - AncientClassical's wagon.
//!
//! The first two-wheeler in the family. The two-wheel anchor set is simply
//! one paired axle, and the BALANCE POINT is the car centred on it
//! ([`super::Bed::OnAxle`]), whatever the seed's wheelbase. Four spokes to a
//! wheel, the Greek wheel; six at Ornate.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::super::super::common::{quat_x, quat_y};
use super::super::dim;
use super::super::plan::Axle;
use super::running_gear::{self, NAVE_HALF_LENGTH, WheelStyle};
use super::{Bed, Bodywork, Form, WagonPlan, board, dressing, line, solid, turned};

/// One axle a little aft of centre, so the pole has the length ahead of it.
const AXLES: &[Axle] = &[Axle {
    at: -0.83,
    paired: true,
    radius: 1.0,
}];

const FORM: Form = Form {
    section: 1.05,
    bed: Bed::OnAxle(0.135),
    seat: None,
    axles: AXLES,
};

const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// The nave's size over a wagon's: a chariot's hub is its strongest part.
const NAVE: f32 = 1.2;

/// The breastwork's kept arc, as a fraction of a full turn: the front and
/// both flanks, open at the back.
const ARC: f32 = 0.62;

pub(super) struct Chariot;

impl Bodywork for Chariot {
    fn form(&self) -> &'static Form {
        &FORM
    }

    fn nave(&self) -> f32 {
        NAVE_HALF_LENGTH * NAVE
    }

    fn build(
        &self,
        kids: &mut Vec<Generator>,
        plan: &WagonPlan,
        c: &WagonColours,
        o: OrnatenessTier,
        w: WearTier,
    ) {
        let (l, hw) = (plan.length, plan.half_w());
        let (a, f) = plan.bed_z();
        let zc = (a + f) * 0.5;
        let ft = plan.floor_t();
        // The floor: a D - a turned disc cut to its front half, and a board
        // behind it.
        let rr = (f - a) * 0.5;
        kids.push(board(
            [hw * 2.0, ft, rr],
            &c.timber,
            [0.0, 0.0, zc - rr * 0.5],
            UPRIGHT,
            l * 0.004,
        ));
        kids.push(turned(
            &[
                (0.0, -ft * 0.5),
                (hw, -ft * 0.5),
                (hw, ft * 0.5),
                (0.0, ft * 0.5),
            ],
            24,
            false,
            &c.timber,
            [0.0, 0.0, zc],
            UPRIGHT,
            0.0,
            [0.0, 0.5],
        ));
        // The breastwork: ONE turned wall, bored thin and cut to the front
        // arc and the flanks. A Lathe's cut starts on its local +X and runs
        // toward +Z, so the quarter turn that centres the kept arc on the
        // nose is the arc's own middle less a right angle.
        let bh = plan.depth() * 1.05;
        kids.push(turned(
            &[(hw, 0.0), (hw * 1.03, bh * 0.6), (hw * 0.98, bh)],
            28,
            true,
            &c.paint,
            [0.0, 0.0, zc],
            quat_y(ARC * PI - FRAC_PI_2),
            0.93,
            [0.0, ARC],
        ));
        let rail: Vec<([f32; 3], f32)> = (0..9)
            .map(|k| {
                let ang = -ARC * PI + 2.0 * ARC * PI * k as f32 / 8.0;
                (
                    [ang.sin() * hw * 0.975, bh, zc + ang.cos() * hw * 0.975],
                    dim(l * 0.008),
                )
            })
            .collect();
        kids.push(line(&rail, 6, &c.brightwork));
        running_gear::axles(kids, plan, 0.011, [1.6, 0.030], ft * 0.2, c);
        running_gear::wheels(
            kids,
            plan,
            WheelStyle {
                spokes: if o == OrnatenessTier::Ornate { 6 } else { 4 },
                tyre: true,
                nave: NAVE,
            },
            c,
        );
        // The pole, out of the floor's front and rising gently to the yoke.
        let pr = dim(l * 0.017);
        let nose = 0.47 * l;
        let tip = [0.0, l * 0.050, nose - l * 0.02];
        kids.push(line(
            &[
                ([0.0, -ft * 0.2, zc - rr * 0.2], pr),
                ([0.0, 0.0, f], pr),
                ([0.0, l * 0.018, (f + nose) * 0.5], pr * 0.92),
                (tip, pr * 0.85),
            ],
            8,
            &c.timber,
        ));
        let yoke: Vec<([f32; 3], f32)> = (0..5)
            .map(|k| {
                let u = -1.0 + 2.0 * k as f32 / 4.0;
                (
                    [u * hw * 1.2, l * 0.042 + l * 0.022 * u * u, nose - l * 0.06],
                    dim(l * 0.011),
                )
            })
            .collect();
        kids.push(line(&yoke, 6, &c.timber));
        if o == OrnatenessTier::Ornate {
            let kr = pr * 1.5;
            let knob = [
                (0.0, -kr * 0.4),
                (kr * 0.7, -kr * 0.2),
                (kr, kr * 0.5),
                (kr * 0.6, kr * 1.2),
                (0.0, kr * 1.4),
            ];
            kids.push(solid(
                &knob,
                12,
                true,
                &c.brightwork,
                [tip[0], tip[1], tip[2] - kr * 0.2],
                quat_x(FRAC_PI_2),
            ));
        }
        dressing::dress(kids, plan, c, o, w);
    }

    fn lantern(&self, _: &WagonPlan, _: OrnatenessTier) -> Option<[f32; 3]> {
        None
    }

    fn perch(&self, plan: &WagonPlan) -> [f32; 3] {
        let (a, f) = plan.bed_z();
        [0.0, plan.depth() * 1.05 + plan.length * 0.12, (a + f) * 0.5]
    }
}
