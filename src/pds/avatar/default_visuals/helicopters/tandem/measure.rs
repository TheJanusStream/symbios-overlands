//! Her box, read off the machine she draws (#1630): her nose's tip, her tail
//! light and her width - her sponsons, or an adorned machine's hoist - as
//! the coaxial's are (#1628), her blades pruned ([`airframe_bounds`]).

use crate::catalogue::items::measure::Bounds;
use crate::seeded_defaults::OrnatenessTier;

use super::super::{PodPlan, TandemColours, airframe_bounds};
use super::{PROFILE, draw};

/// The bounds of the machine she draws at `ornateness` (drawn frame): her
/// airframe's, in the default material - her shape, which is the same in
/// any paint.
pub(super) fn drawn(plan: &PodPlan, ornateness: OrnatenessTier) -> Bounds {
    let mut kids = Vec::new();
    draw(&mut kids, plan, &TandemColours::default(), ornateness);
    airframe_bounds(&kids)
}

/// How far ahead of her first station and aft of her boom's end she is
/// drawn (m, prototype frame) - her nose's tip, leaning a little ahead of
/// its station where her girth tilts it, and her tail light, short of her
/// boom's end, so negative - read off the plain machine ([`drawn`]). An
/// ornate machine's probe reaches past her nose, and stands outside her box
/// on purpose; no tier draws past her tail.
pub(super) fn reach(plan: &PodPlan) -> (f32, f32) {
    let drawn = drawn(plan, OrnatenessTier::Plain);
    let nose = plan.at(0.0, 0.0, plan.nose())[2];
    let tail = plan.at(0.0, 0.0, PROFILE.boom_end)[2];
    (
        (drawn.max.z - nose) / plan.len(1.0),
        (tail - drawn.min.z) / plan.len(1.0),
    )
}
