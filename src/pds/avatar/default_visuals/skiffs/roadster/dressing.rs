//! What a roadster carries by ornateness and by wear (#1367) - the owner's
//! ladder, agreed on the phase-1 renders:
//!
//! | tier     | adds                                                         |
//! |----------|--------------------------------------------------------------|
//! | every    | a spare wheel on the tail mount                              |
//! | Adorned  | a side-mount spare on the near-side running board            |
//! | Ornate   | a trunk on the tail mount in the spare's place, the folded hood (open cars) |
//! | Worn     | a front wing in grey primer                                  |
//! | Battered | the primer wing, and a jerrycan roped to the off-side board  |
//!
//! ORNATENESS ADDS SECONDARY MASSES, NOT TRINKETS, and WEAR IS MASSES TOO,
//! never texture noise: at 109 px a metre a finial is a pixel and a grime
//! pattern is a smear, but a trunk, a spare and a replacement wing change the
//! machine's silhouette. The chase camera rides behind the car and looks down
//! 22.9 degrees, so everything here is on the tail, the running boards or the
//! wings. The spot lamps the brief suggested were rendered and dropped: on the
//! headlamp crossbar they read from neither quarter, and a pillar spot barely
//! did.
//!
//! Every mass is placed off the plan - the tail mount the body publishes, the
//! running board [`coachwork::board`] reads, the canopy seat's plane - never
//! off a restated fraction, which is what lets the whole ladder stand on all
//! three bodies. And no two masses on any tier pair share a footprint: the
//! tail mount carries the spare OR the trunk, the near-side board's forward
//! end the side spare, the off-side board's after end the jerrycan, the
//! cockpit's after rim the hood; the primer wing is a colour.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, RoadsterTop, WearTier};

use super::super::super::common::{bevel, id_quat, prim, quat_x, quat_z};
use super::super::plan::{BodyPlan, TailMount};
use super::super::{SkiffColours, dim};
use super::coachwork::{self, NEAR_SIDE};
use super::wheels::Wheels;
use super::{line, turned};

/// A spare's outer radius over the road wheels', on the tail and on the side.
const TAIL_SPARE: f32 = 0.52;
const SIDE_SPARE: f32 = 0.58;

/// Whether a car of this wear wears the mismatched wing - drawn by
/// [`coachwork::wings`], because it is the near side's own guard split.
pub(super) fn wears_a_primer_wing(wear: WearTier) -> bool {
    wear >= WearTier::Worn
}

/// Dress the roadster for its tiers.
pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &BodyPlan,
    c: &SkiffColours,
    top: RoadsterTop,
    wheel: &dyn Wheels,
    ornateness: OrnatenessTier,
    wear: WearTier,
) {
    if ornateness < OrnatenessTier::Ornate {
        tail_spare(kids, plan, c, wheel);
    }
    if ornateness >= OrnatenessTier::Adorned {
        side_spare(kids, plan, c, wheel);
    }
    if ornateness == OrnatenessTier::Ornate {
        trunk(kids, plan, c);
        if top == RoadsterTop::Open {
            folded_hood(kids, plan, c);
        }
    }
    if wear == WearTier::Battered {
        jerrycan(kids, plan, c);
    }
}

/// Where the tail spare's centre goes and how it is laid, off the tail mount
/// the body publishes (#1367 defect 4).
///
/// On a deck it leans back on the crown near the tip, as the boat-tail always
/// carried it. On a blunt back it stands nearly upright with its inboard face
/// pressed into the panel by a fraction of its own half-width - contact by
/// construction, because the connectedness guard cannot see a gap at a blunt
/// end ([`TailMount::Back`]).
fn tail_spare_mount(plan: &BodyPlan, wheel: &dyn Wheels) -> ([f32; 3], [f32; 4]) {
    let l = plan.length;
    match plan.tail_mount() {
        TailMount::Deck { at, lift, lean } => {
            let z = at * l;
            (
                [0.0, plan.crown_at(z) + lift * l, z],
                quat_x(-(FRAC_PI_2 - lean)),
            )
        }
        TailMount::Back { sink, lean } => {
            let p = wheel.spare(plan.wheel_r * TAIL_SPARE);
            (
                [0.0, 0.0, plan.tail_z() - p.face + sink * p.half_width],
                quat_x(-(FRAC_PI_2 - lean)),
            )
        }
    }
}

/// The station of a tail spare's inboard face (m) - what the tail-mount guard
/// checks against the drawn end of a blunt tail.
#[cfg(test)]
pub(super) fn tail_spare_face_z(plan: &BodyPlan, wheel: &dyn Wheels) -> f32 {
    let (at, _) = tail_spare_mount(plan, wheel);
    let face = wheel.spare(plan.wheel_r * TAIL_SPARE).face;
    let lean = match plan.tail_mount() {
        TailMount::Deck { lean, .. } | TailMount::Back { lean, .. } => lean,
    };
    at[2] + face * lean.cos()
}

/// The spare every roadster carries short of Ornate, on the tail mount - the
/// rear of a boat tail is otherwise a blank at play distance.
fn tail_spare(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours, wheel: &dyn Wheels) {
    let (at, lay) = tail_spare_mount(plan, wheel);
    let p = wheel.spare(plan.wheel_r * TAIL_SPARE);
    kids.push(turned(&p.tyre, 24, true, c.rubber.clone(), at, lay));
    kids.push(turned(&p.disc, 24, false, c.disc.clone(), at, lay));
}

/// A spare standing on the near-side running board's FORWARD end, face
/// outboard, its inboard face against the body's own flank - a side-mount,
/// the touring car's mark. Sized to the board it stands on.
fn side_spare(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours, wheel: &dyn Wheels) {
    let l = plan.length;
    let b = coachwork::board(plan);
    let r = plan.wheel_r * SIDE_SPARE;
    let p = wheel.spare(r);
    let z = b.front - r * 0.98;
    let y = b.top + r - l * 0.003;
    let x = NEAR_SIDE * (plan.side_at(z, y) + p.face * 0.85);
    let lay = quat_z(-NEAR_SIDE * FRAC_PI_2);
    kids.push(turned(&p.tyre, 24, true, c.rubber.clone(), [x, y, z], lay));
    kids.push(turned(&p.disc, 24, false, c.disc.clone(), [x, y, z], lay));
}

/// A hide trunk on a brass luggage grid, on the TAIL MOUNT - which it takes
/// over from the spare on an Ornate car (the side-mount carries the spare
/// instead). On a deck it rides the tail deck behind the cockpit, as wide as
/// the deck is at its narrower end; on a blunt back it stands on a grid run
/// out of the panel, bedded into it.
fn trunk(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let (size, at, grid) = match plan.tail_mount() {
        TailMount::Deck { .. } => {
            let aft = plan.cockpit_z().0;
            let (z0, z1) = (aft - l * 0.045, aft - l * 0.135);
            let (hw, h) = (plan.half_width_at(z1) * 0.95, l * 0.058);
            let floor = plan.crown_at(z1);
            let (ga, gb) = (z0 + l * 0.01, z1 - l * 0.012);
            (
                [dim(hw * 2.0), dim(h), dim(z0 - z1)],
                [0.0, floor + h * 0.5 - l * 0.004, (z0 + z1) * 0.5],
                [(ga, plan.crown_at(ga)), (gb, plan.crown_at(gb))],
            )
        }
        TailMount::Back { .. } => {
            let tz = plan.tail_z();
            let depth = l * 0.075;
            let (hw, h) = (plan.half_width_at(tz + l * 0.03) * 0.95, l * 0.080);
            let y = -h * 0.25;
            let grid_y = y - h * 0.5;
            (
                [dim(hw * 2.0), dim(h), dim(depth)],
                [0.0, y, tz - depth * 0.5 + l * 0.006],
                [(tz + l * 0.03, grid_y), (tz - depth - l * 0.004, grid_y)],
            )
        }
    };
    kids.push(prim(
        bevel(size, l * 0.008, 4, c.trunk.clone()),
        at,
        id_quat(),
    ));
    // The grid: one brass loop round the trunk's foot - one node, and it is
    // what says "rack".
    let gx = size[0] * 0.5 + l * 0.004;
    let [(za, ya), (zb, yb)] = grid;
    let r = l * 0.0045;
    kids.push(line(
        &[
            ([-gx, ya, za], r),
            ([-gx, yb, zb], r),
            ([gx, yb, zb], r),
            ([gx, ya, za], r),
            ([-gx, ya, za], r),
        ],
        6,
        c.brightwork.clone(),
    ));
}

/// The top of the bodywork at `(x, z)`: the elliptical section's own crown.
fn deck_y(plan: &BodyPlan, x: f32, z: f32) -> f32 {
    let hw = plan.half_width_at(z);
    let t = (x.abs() / hw.max(1e-4)).min(0.999);
    plan.crown_at(z) * (1.0 - t * t).sqrt()
}

/// The hood, down: a canvas roll lying across the cockpit's after rim,
/// bedded on the coaming and following the deck's camber. Seated on the
/// canopy seat's plane - where a hood stands when it is up - at the rim it
/// folds onto when it is down. An open car's alone.
fn folded_hood(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let base = plan.canopy_seat()[1];
    let z = plan.cockpit_z().0 - l * 0.004;
    let r = dim(l * 0.020);
    let hw = plan.half_width_at(z);
    let roll: Vec<([f32; 3], f32)> = [-0.86f32, -0.45, 0.0, 0.45, 0.86]
        .iter()
        .map(|&f| {
            let x = f * hw;
            (
                [x, deck_y(plan, x, z).max(base) + r * 0.45, z],
                r * (1.0 - 0.15 * f.abs()),
            )
        })
        .collect();
    kids.push(line(&roll, 10, c.hood.clone()));
}

/// A jerrycan roped to the off-side running board's AFTER end, where nothing
/// else stands: a Bevel can at a real can's proportions against the car this
/// one models - the first draw was a third of that and read as a matchbox -
/// and one rope over it, board to board.
fn jerrycan(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let b = coachwork::board(plan);
    let side = -NEAR_SIDE;
    let x = side * (b.inner + b.outer) * 0.5;
    let size = [l * 0.036, l * 0.100, l * 0.070];
    let z = b.back + size[2] * 0.5 + l * 0.010;
    let y = b.top + size[1] * 0.5 - l * 0.002;
    kids.push(prim(
        bevel(size.map(dim), l * 0.004, 4, c.can.clone()),
        [x, y, z],
        id_quat(),
    ));
    let (rope, over) = (l * 0.0042, y + size[1] * 0.52);
    kids.push(line(
        &[
            ([x - side * size[0] * 0.62, b.top - l * 0.002, z], rope),
            ([x - side * size[0] * 0.55, over, z], rope),
            ([x + side * size[0] * 0.55, over, z], rope),
            ([x + side * size[0] * 0.62, b.top - l * 0.002, z], rope),
        ],
        6,
        c.hood.clone(),
    ));
}
