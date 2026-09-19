//! The hearse: a tall glazed body on the cart's running gear, a coachman's
//! box seat high at the front, and lanterns lit on every tier -
//! GothicHorror's wagon, always in mourning black.
//!
//! Its glass is LIGHT, not a volume (#1359 rule 4): candle-lit bands on both
//! sides and the back, two iron mullions a side. In the accent colour they
//! read as beige panels in daylight; candle-warm, they read as a lit hearse.

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::super::dim;
use super::bed::{self, Bench, BoxBed};
use super::running_gear::{self, WAGON_WHEEL};
use super::{Bed, Bodywork, FOUR_WHEELS, Form, WagonPlan, board, dressing, line, solid};

const FORM: Form = Form {
    section: 0.55,
    bed: Bed::Fixed(-0.470, 0.370),
    seat: Some(0.290),
    axles: FOUR_WHEELS,
};

const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// The glazed body's height over the bed's sides, as a fraction of the length.
const BODY_H: f32 = 0.23;

pub(super) struct Hearse;

impl Hearse {
    /// The glazed body's forward end (m), a little aft of the box seat.
    fn body_front(plan: &WagonPlan) -> f32 {
        plan.seat_z() - plan.length * 0.050
    }

    fn lantern_set(plan: &WagonPlan) -> (f32, f32, f32) {
        let l = plan.length;
        (
            Self::body_front(plan) - l * 0.012,
            plan.depth() + l * BODY_H * 0.35,
            plan.half_w() + l * 0.028,
        )
    }
}

impl Bodywork for Hearse {
    fn form(&self) -> &'static Form {
        &FORM
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
        let h = plan.depth();
        bed::box_bed(
            kids,
            plan,
            BoxBed {
                side: h,
                tail: true,
                stakes: false,
            },
            c,
        );
        let ft = plan.floor_t();
        running_gear::axles(kids, plan, 0.011, [1.7, 0.030], -ft * 0.5 + ft * 0.3, c);
        running_gear::wheels(kids, plan, WAGON_WHEEL, c);
        let (a, f) = plan.bed_z();
        let body_f = Self::body_front(plan);
        let (ln, zc) = (body_f - a, (a + body_f) * 0.5);
        let ch = l * BODY_H;
        kids.push(board(
            [hw * 2.0, ch, ln],
            &c.paint,
            [0.0, h + ch * 0.5 - l * 0.004, zc],
            UPRIGHT,
            l * 0.010,
        ));
        // The glass, drawn as light, and its mullions.
        let (wy, wh) = (h + ch * 0.52, ch * 0.62);
        for s in [-1.0f32, 1.0] {
            kids.push(board(
                [dim(l * 0.006), wh, ln * 0.80],
                &c.lamp,
                [s * (hw + l * 0.001), wy, zc],
                UPRIGHT,
                l * 0.002,
            ));
            for zf in [-0.17f32, 0.17] {
                let (x, z) = (s * (hw + l * 0.004), zc + ln * zf);
                kids.push(line(
                    &[
                        ([x, wy - wh * 0.5, z], dim(l * 0.005)),
                        ([x, wy + wh * 0.5, z], dim(l * 0.005)),
                    ],
                    4,
                    &c.iron,
                ));
            }
        }
        kids.push(board(
            [hw * 1.4, wh, dim(l * 0.006)],
            &c.lamp,
            [0.0, wy, a - l * 0.001],
            UPRIGHT,
            l * 0.002,
        ));
        // An overhanging roof, and a rail round it seated in it.
        let ry = h + ch - l * 0.004;
        kids.push(board(
            [hw * 2.16, dim(l * 0.016), ln + l * 0.030],
            &c.paint,
            [0.0, ry + l * 0.006, zc],
            UPRIGHT,
            l * 0.012,
        ));
        let (rx, rz) = (hw * 0.95, ln * 0.5 - l * 0.004);
        let corners = [(-rx, -rz), (rx, -rz), (rx, rz), (-rx, rz)];
        let rail: Vec<([f32; 3], f32)> = corners
            .iter()
            .chain(std::iter::once(&corners[0]))
            .map(|&(x, z)| ([x, ry + l * 0.0165, zc + z], dim(l * 0.0055)))
            .collect();
        kids.push(line(&rail, 5, &c.brightwork));
        // The coachman's box: a bench high at the front on a riser bedded
        // into the sides it stands between.
        let rise = ch * 0.45;
        kids.push(board(
            [hw * 2.0 - l * 0.010, rise + l * 0.008, l * 0.08],
            &c.paint,
            [0.0, h + rise * 0.5 - l * 0.004, plan.seat_z()],
            UPRIGHT,
            l * 0.006,
        ));
        bed::sprung_bench(
            kids,
            plan,
            Bench {
                z: plan.seat_z(),
                base_y: h + rise,
                seat_y: h + rise + l * 0.05,
                base_x: hw * 0.9 - l * 0.012,
                width: hw * 2.0 * 0.96,
                back: false,
            },
            c,
        );
        bed::footboard(kids, plan, f - l * 0.010, h * 0.55, l * 0.11, 0.55, c);
        let (z, y, x) = Self::lantern_set(plan);
        bed::lanterns(kids, plan, z, y, x, c);
        if o == OrnatenessTier::Ornate {
            let urn = [
                (0.0, 0.0),
                (l * 0.012, 0.0),
                (l * 0.009, l * 0.018),
                (l * 0.014, l * 0.034),
                (l * 0.006, l * 0.050),
                (0.0, l * 0.056),
            ];
            for &(x, z) in &corners {
                kids.push(solid(
                    &urn,
                    10,
                    true,
                    &c.brightwork,
                    [x, ry + l * 0.012, zc + z],
                    UPRIGHT,
                ));
            }
        }
        dressing::dress(kids, plan, c, o, w);
    }

    fn lantern(&self, plan: &WagonPlan, _: OrnatenessTier) -> Option<[f32; 3]> {
        let (z, y, x) = Self::lantern_set(plan);
        Some(bed::lantern_at(plan, z, y, x))
    }

    fn perch(&self, plan: &WagonPlan) -> [f32; 3] {
        let l = plan.length;
        [0.0, plan.depth() + l * (BODY_H + 0.06), plan.seat_z()]
    }
}
