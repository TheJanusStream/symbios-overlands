//! Her box, read off the machine she draws (#1628): her nose's tip, her
//! shroud's aft lip and her width - her skids, her H-tail's nav lights or an
//! ornate machine's stub wings - as the autogyro's are (#1624), her blades
//! and her pusher's pruned ([`airframe_bounds`]).

use crate::catalogue::items::measure::Bounds;
use crate::seeded_defaults::OrnatenessTier;

use super::super::{CoaxialColours, PodPlan, airframe_bounds};
use super::{PROFILE, draw};

/// The bounds of the machine she draws at `ornateness` (drawn frame): her
/// airframe's, in the default material - her shape, which is the same in
/// any paint.
pub(super) fn drawn(plan: &PodPlan, ornateness: OrnatenessTier) -> Bounds {
    let mut kids = Vec::new();
    draw(&mut kids, plan, &CoaxialColours::default(), ornateness);
    airframe_bounds(&kids)
}

/// How far ahead of her first station and aft of her boom's end she is
/// drawn (m, prototype frame) - her nose's tip, leaning a little ahead of
/// its station where her girth tilts it, and her shroud's aft lip - read off
/// the plain machine ([`drawn`]). No tier draws past either end.
pub(super) fn reach(plan: &PodPlan) -> (f32, f32) {
    let drawn = drawn(plan, OrnatenessTier::Plain);
    let nose = plan.at(0.0, 0.0, plan.nose())[2];
    let tail = plan.at(0.0, 0.0, PROFILE.boom_end)[2];
    (
        (drawn.max.z - nose) / plan.len(1.0),
        (tail - drawn.min.z) / plan.len(1.0),
    )
}
