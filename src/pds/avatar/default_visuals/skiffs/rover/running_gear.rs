//! The rover's running gear: a rocker and a bogie a side through shared
//! joints, one differential bar athwart both rocker pivots, a stub axle at
//! every hub, and six small drum wheels.
//!
//! **Every member is drawn THROUGH a joint another member also runs
//! through** - the dune buggy's law (#1374). Each arm ends ON a centreline
//! another arm passes along, so the connectedness guard registers a joint
//! between thin tubes that nothing has been fattened to make.
//!
//! A wheel is two turned nodes, the buggy's: a tyre whose profile ends off
//! the axis is capped with a full disc (the disc-cap trap, #1359), so the
//! rim has to stand proud of BOTH end caps. No cleats: circumferential
//! grousers cost 906 B over six wheels and read NOWHERE, at 12 m or at 2x,
//! because a Lathe's normals are radial and a groove therefore neither
//! shades nor notches the outline. Transverse cleat boxes do read - and cost
//! 60 nodes, 14 KB, 25 components in the guard and 8 mm under the ground
//! line (#1378 Q-G).

use crate::pds::avatar::livery::RoverColours;
use crate::pds::generator::Generator;

use super::{
    ARM_R, AxleLine, RoverPlan, TYRE_W, line, outboard, rim_profile, side, solid, tyre_profile,
};

/// How far up from the rear hub's line toward the datum the bogie's pivot
/// stands.
///
/// The bogie pivots midway between the two wheels it carries and about half
/// way up toward the deck; the rocker pivots where the front wheel's load
/// and the bogie's two balance, which is A THIRD of the way from the bogie's
/// pivot to the front hub - and on the datum, so the differential bar
/// through both rocker pivots runs through the deck itself.
const BOGIE_UP: f32 = 0.52;

/// The rocker-bogie's joints on one side, read off the plan.
///
/// A landmark struct rather than a bag of loose numbers, because the five
/// points travel together: three hubs, the bogie's pivot and the rocker's,
/// and every member of the linkage is two of them.
#[derive(Clone, Copy, Debug)]
pub(super) struct Bogie {
    /// The plane the rocker and the bogie lie in (m from the centreline).
    x: f32,
    front: AxleLine,
    mid: AxleLine,
    rear: AxleLine,
    /// The BOGIE's pivot: midway between the mid and rear stations, part way
    /// up from their hubs toward the deck.
    zq: f32,
    yq: f32,
    /// The ROCKER's pivot, ON THE DATUM (`y = 0`) - which is why the
    /// differential bar through both of them runs through the deck itself.
    zp: f32,
}

impl Bogie {
    pub(super) fn of(plan: &RoverPlan) -> Self {
        let (front, mid, rear) = (plan.axle(1.0), plan.axle(0.0), plan.axle(-1.0));
        let zq = (mid.z + rear.z) * 0.5;
        Self {
            x: plan.arm_x(),
            front,
            mid,
            rear,
            zq,
            yq: rear.y * (1.0 - BOGIE_UP),
            zp: zq + (front.z - zq) / 3.0,
        }
    }

    /// The ROCKER on side `s`: front hub - knee - the rocker's pivot - and
    /// on down to the bogie's pivot, so the two arms share a joint.
    fn rocker(&self, plan: &RoverPlan, s: f32) -> [([f32; 3], f32); 3] {
        let r = ARM_R * plan.length;
        [
            ([s * self.x, self.front.y, self.front.z], r),
            ([s * self.x, 0.0, self.zp], r * 1.15),
            ([s * self.x, self.yq, self.zq], r),
        ]
    }

    /// The BOGIE on side `s`: mid hub - the bogie's pivot - rear hub.
    fn bogie(&self, plan: &RoverPlan, s: f32) -> [([f32; 3], f32); 3] {
        let r = ARM_R * plan.length * 0.92;
        [
            ([s * self.x, self.mid.y, self.mid.z], r),
            ([s * self.x, self.yq, self.zq], r * 1.10),
            ([s * self.x, self.rear.y, self.rear.z], r),
        ]
    }
}

/// The whole linkage: a rocker and a bogie a side, the differential bar and
/// six stub axles - eleven thin tubes.
pub(super) fn rocker_bogie(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let f = Bogie::of(plan);
    let r = ARM_R * plan.length;
    for s in [-1.0f32, 1.0] {
        kids.push(line(&f.rocker(plan, s), 8, &c.arm));
        kids.push(line(&f.bogie(plan, s), 8, &c.arm));
    }
    // The DIFFERENTIAL BAR: one tube athwart the machine through both
    // rockers' pivots and the deck between them. It is what hangs the
    // running gear on the rover, and at the narrow-track corner - where the
    // arm plane is INSIDE the deck's flank - it is the only thing that does.
    kids.push(line(
        &[
            ([-f.x - r, 0.0, f.zp], r * 1.2),
            ([-f.x, 0.0, f.zp], r * 1.2),
            ([f.x, 0.0, f.zp], r * 1.2),
            ([f.x + r, 0.0, f.zp], r * 1.2),
        ],
        8,
        &c.arm,
    ));
    // A STUB AXLE at every hub, drawn from inboard of the arm plane THROUGH
    // the arm's own end joint and on into the rim.
    for (at, rad) in plan.wheels() {
        let s = side(at[0]);
        let hub = at[0].abs() - rad * TYRE_W * 0.30;
        kids.push(line(
            &[
                ([s * (f.x - r), at[1], at[2]], r * 0.95),
                ([s * f.x, at[1], at[2]], r * 0.95),
                ([s * hub, at[1], at[2]], r * 0.95),
            ],
            6,
            &c.arm,
        ));
    }
}

/// The six wheels on the plan's anchors, the front axle first and `-x`
/// before `+x`. A worn machine's near-REAR rim - the one the chase quarter
/// shows whole - is bare steel: on a kit as luminous as every one of hers,
/// that is a DEAD rim among five lit ones, which is the cyclecar's rule and
/// reads at 12 m.
pub(super) fn wheels(
    kids: &mut Vec<Generator>,
    plan: &RoverPlan,
    c: &RoverColours,
    odd: Option<usize>,
) {
    for (i, (at, r)) in plan.wheels().into_iter().enumerate() {
        let w = r * TYRE_W;
        let lay = outboard(at[0]);
        kids.push(solid(&tyre_profile(r, w), 20, true, &c.tyre, at, lay));
        let rim = if Some(i) == odd { &c.odd_rim } else { &c.rim };
        kids.push(solid(&rim_profile(r, w), 12, false, rim, at, lay));
    }
}
