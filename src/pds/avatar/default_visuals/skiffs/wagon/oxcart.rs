//! The ox-cart: FeudalJapan's gissha - a lacquered cabin between two big
//! wheels, under a swept barrel roof whose eaves sweep UP front and back, on
//! two long shafts to a crossbar.
//!
//! The roof is the cart's tilt idiom turned to a different end: one swept
//! upper half-pipe, flared and raised at both ends, flattened by its own node
//! scale. The cane blinds hang front and back, so the one the chase camera
//! sees first is the pale panel at her stern; the side window is lit.

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::super::dim;
use super::super::plan::Axle;
use super::running_gear::{self, NAVE_HALF_LENGTH, WheelStyle};
use super::{Bed, Bodywork, Form, WagonPlan, board, dressing, half_pipe, line};

/// One axle under the cabin's middle; its wheels a little bigger than a
/// wagon's, as a gissha's are.
const AXLES: &[Axle] = &[Axle {
    at: -0.53,
    paired: true,
    radius: 1.04,
}];

const FORM: Form = Form {
    section: 0.60,
    bed: Bed::OnAxle(0.230),
    seat: None,
    axles: AXLES,
};

const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// The cabin's height, as a fraction of the length.
const CABIN_H: f32 = 0.26;
/// The roof's node scale on y: a low barrel, not a dome.
const ROOF_SCALE_Y: f32 = 0.55;
/// The nave's size over a wagon's.
const NAVE: f32 = 1.3;

pub(super) struct OxCart;

impl OxCart {
    /// The roof's swept stations: the cabin's length and an overhang each
    /// way, the eaves flared and lifted at both ends.
    fn roof(plan: &WagonPlan) -> Vec<([f32; 3], f32)> {
        let (l, hw) = (plan.length, plan.half_w());
        let (a, f) = plan.bed_z();
        let (ln, ch, oh) = (f - a, l * CABIN_H, l * 0.07);
        (0..7)
            .map(|i| {
                let t = i as f32 / 6.0;
                let e = (2.0 * t - 1.0).powi(4);
                (
                    [
                        0.0,
                        ch - l * 0.004 + l * 0.045 * e,
                        a - oh + (ln + 2.0 * oh) * t,
                    ],
                    hw * (1.12 + 0.10 * e),
                )
            })
            .collect()
    }
}

impl Bodywork for OxCart {
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
        let (zc, ln) = ((a + f) * 0.5, f - a);
        let ft = plan.floor_t();
        kids.push(board(
            [hw * 2.0, ft, ln],
            &c.timber,
            [0.0, 0.0, zc],
            UPRIGHT,
            l * 0.004,
        ));
        let ch = l * CABIN_H;
        kids.push(board(
            [hw * 2.0, ch, ln],
            &c.paint,
            [0.0, ch * 0.5, zc],
            UPRIGHT,
            l * 0.006,
        ));
        for s in [-1.0f32, 1.0] {
            kids.push(board(
                [dim(l * 0.006), ch * 0.34, ln * 0.34],
                &c.lamp,
                [s * (hw + l * 0.001), ch * 0.62, zc - ln * 0.08],
                UPRIGHT,
                l * 0.002,
            ));
        }
        for (z, out) in [(a, -1.0f32), (f, 1.0)] {
            kids.push(board(
                [hw * 1.6, ch * 0.78, dim(l * 0.008)],
                &c.blind,
                [0.0, ch * 0.55, z + out * l * 0.002],
                UPRIGHT,
                l * 0.002,
            ));
        }
        let roof = Self::roof(plan);
        kids.push(half_pipe(&roof, ROOF_SCALE_Y, 0.90, &c.paint));
        if o == OrnatenessTier::Ornate {
            // A gilt ridge along the crown, stopping short of the eaves so it
            // cannot stand off their upswept ends.
            let ridge: Vec<([f32; 3], f32)> = roof[1..6]
                .iter()
                .map(|&(p, r)| ([0.0, p[1] + r * ROOF_SCALE_Y * 0.97, p[2]], dim(l * 0.009)))
                .collect();
            kids.push(line(&ridge, 6, &c.brightwork));
        }
        running_gear::axles(kids, plan, 0.012, [1.7, 0.034], ft * 0.2, c);
        running_gear::wheels(
            kids,
            plan,
            WheelStyle {
                spokes: 16,
                tyre: false,
                nave: NAVE,
            },
            c,
        );
        // Two shafts forward to a crossbar.
        let sr = dim(l * 0.012);
        let nose = 0.49 * l;
        for s in [-1.0f32, 1.0] {
            let x = s * hw * 0.78;
            kids.push(line(
                &[
                    ([x, -ft * 0.2, zc], sr),
                    ([x, 0.0, f], sr),
                    ([x, l * 0.03, (f + nose) * 0.5], sr),
                    ([x * 0.92, l * 0.075, nose - l * 0.012], sr * 0.9),
                ],
                8,
                &c.timber,
            ));
        }
        kids.push(line(
            &[
                ([-hw, l * 0.085, nose - l * 0.012], dim(l * 0.011)),
                ([hw, l * 0.085, nose - l * 0.012], dim(l * 0.011)),
            ],
            6,
            &c.timber,
        ));
        dressing::dress(kids, plan, c, o, w);
    }

    fn lantern(&self, _: &WagonPlan, _: OrnatenessTier) -> Option<[f32; 3]> {
        None
    }

    fn perch(&self, plan: &WagonPlan) -> [f32; 3] {
        let (a, f) = plan.bed_z();
        [0.0, plan.length * (CABIN_H + 0.12), (a + f) * 0.5]
    }
}
