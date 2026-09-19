//! The cart: a plank box on four wheels, the front pair smaller, a sprung
//! bench at the front, and from Adorned up a canvas tilt on swept bows and a
//! pair of lanterns - the covered wagon. The floor of every wagon theme that
//! has no vehicle of its own.

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::bed::{self, Bench, BoxBed};
use super::running_gear::{self, WAGON_WHEEL};
use super::{Bed, Bodywork, FOUR_WHEELS, Form, WagonPlan, dressing};

const FORM: Form = Form {
    section: 0.72,
    bed: Bed::Fixed(-0.470, 0.370),
    seat: Some(0.290),
    axles: FOUR_WHEELS,
};

/// Whether a cart on this tier carries its tilt and lanterns.
pub(super) fn tilted(o: OrnatenessTier) -> bool {
    o != OrnatenessTier::Plain
}

pub(super) struct Cart;

impl Cart {
    /// Where its lantern pair hangs: at the bed's front corners, on irons off
    /// the side tops.
    fn lantern_set(plan: &WagonPlan) -> (f32, f32, f32) {
        let l = plan.length;
        (
            plan.bed_z().1 - l * 0.02,
            plan.depth(),
            plan.half_w() + l * 0.030,
        )
    }
}

impl Bodywork for Cart {
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
        let l = plan.length;
        let h = plan.depth();
        bed::box_bed(
            kids,
            plan,
            BoxBed {
                side: h,
                tail: true,
                stakes: true,
            },
            c,
        );
        let ft = plan.floor_t();
        running_gear::axles(kids, plan, 0.011, [1.7, 0.030], -ft * 0.5 + ft * 0.3, c);
        running_gear::wheels(kids, plan, WAGON_WHEEL, c);
        bed::sprung_bench(
            kids,
            plan,
            Bench {
                z: plan.seat_z(),
                base_y: h,
                seat_y: h + l * 0.067,
                base_x: plan.half_w() - l * 0.006,
                width: plan.half_w() * 2.0 * 0.96,
                back: true,
            },
            c,
        );
        bed::footboard(
            kids,
            plan,
            plan.bed_z().1 - l * 0.010,
            h * 0.55,
            l * 0.11,
            0.55,
            c,
        );
        if tilted(o) {
            bed::tilt(
                kids,
                plan,
                plan.bed_z().0 + l * 0.012,
                plan.seat_z() - l * 0.060,
                c,
            );
            let (z, y, x) = Self::lantern_set(plan);
            bed::lanterns(kids, plan, z, y, x, c);
        }
        dressing::dress(kids, plan, c, o, w);
    }

    fn lantern(&self, plan: &WagonPlan, o: OrnatenessTier) -> Option<[f32; 3]> {
        tilted(o).then(|| {
            let (z, y, x) = Self::lantern_set(plan);
            bed::lantern_at(plan, z, y, x)
        })
    }

    fn perch(&self, plan: &WagonPlan) -> [f32; 3] {
        [0.0, plan.depth() + plan.length * 0.16, plan.seat_z()]
    }
}
