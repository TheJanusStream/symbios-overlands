//! The buckboard: a low flat bed slung between high wheels, a sprung seat on
//! a riser box, and a dash board at the front - the WildWest's light wagon.
//! Its crown is the SEAT's top, since its sides are only a hand high, so the
//! plan's section is the seat's height over the floor.

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::super::super::common::quat_x;
use super::super::dim;
use super::super::plan::Axle;
use super::bed::{self, Bench, BoxBed};
use super::running_gear::{self, WAGON_WHEEL};
use super::{Bed, Bodywork, Form, WagonPlan, board, dressing};

/// The front wheels nearly the rear's size: a buckboard's bed is low and
/// narrow, so they turn clear under it without being small.
const AXLES: &[Axle] = &[
    Axle {
        at: 1.0,
        paired: true,
        radius: 0.90,
    },
    Axle {
        at: -1.0,
        paired: true,
        radius: 1.0,
    },
];

const FORM: Form = Form {
    section: 0.95,
    bed: Bed::Fixed(-0.400, 0.370),
    seat: Some(0.100),
    axles: AXLES,
};

/// The low sides' height, as a fraction of the length.
const LOW_SIDE: f32 = 0.045;

pub(super) struct Buckboard;

impl Buckboard {
    fn lantern_set(plan: &WagonPlan) -> (f32, f32, f32) {
        let l = plan.length;
        (
            plan.bed_z().1 - l * 0.02,
            l * LOW_SIDE,
            plan.half_w() + l * 0.030,
        )
    }
}

impl Bodywork for Buckboard {
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
        let lo = l * LOW_SIDE;
        bed::box_bed(
            kids,
            plan,
            BoxBed {
                side: lo,
                tail: false,
                stakes: false,
            },
            c,
        );
        let ft = plan.floor_t();
        running_gear::axles(kids, plan, 0.011, [1.7, 0.030], -ft * 0.5 + ft * 0.3, c);
        running_gear::wheels(kids, plan, WAGON_WHEEL, c);
        // The seat stands on a riser box, not on the low sides: it is the
        // tallest thing on her and the crown the plan publishes.
        let z = plan.seat_z();
        let riser = plan.depth() - l * 0.075;
        kids.push(board(
            [hw * 1.7, riser + l * 0.004, l * 0.075],
            &c.boards,
            [0.0, riser * 0.5, z],
            [0.0, 0.0, 0.0, 1.0],
            l * 0.005,
        ));
        bed::sprung_bench(
            kids,
            plan,
            Bench {
                z,
                base_y: riser,
                seat_y: plan.depth(),
                base_x: hw * 0.85 - l * 0.012,
                width: hw * 2.0 * 1.04,
                back: true,
            },
            c,
        );
        // The dash: a board raked steeply up at the bed's front.
        let f = plan.bed_z().1;
        kids.push(board(
            [hw * 1.9, dim(l * 0.011), l * 0.09],
            &c.boards,
            [0.0, lo + l * 0.035, f + l * 0.010],
            quat_x(-1.05),
            l * 0.004,
        ));
        if o != OrnatenessTier::Plain {
            let (z, y, x) = Self::lantern_set(plan);
            bed::lanterns(kids, plan, z, y, x, c);
        }
        dressing::dress(kids, plan, c, o, w);
    }

    fn lantern(&self, plan: &WagonPlan, o: OrnatenessTier) -> Option<[f32; 3]> {
        (o != OrnatenessTier::Plain).then(|| {
            let (z, y, x) = Self::lantern_set(plan);
            bed::lantern_at(plan, z, y, x)
        })
    }

    fn perch(&self, plan: &WagonPlan) -> [f32; 3] {
        [0.0, plan.depth() + plan.length * 0.16, plan.seat_z()]
    }
}
