//! The scow's wear ladder, on the deck and the gear, never the topsides -
//! from the chase camera they roll away under the deck edge (#1365). Worn: a
//! quadrant of the foredeck re-laid in pale new boards. Battered: that, a
//! patch of tarred felt over half the roof, a weathered tarp thrown over the
//! load, and - in the parts that draw them - the stovepipe knocked askew and
//! one paddle board gone from the stern wheel.

use crate::pds::generator::Generator;

use super::super::ScowColours;
use super::super::profile::HullProfile;
use super::super::shape::{DECK_CROWN, run_z, sweep};
use super::cargo::Hold;
use super::hold_z;
use super::house::House;

/// The re-laid quadrant's arc: a quarter of the crowned deck's section, to
/// one side of the centreline.
const PATCH_CUT: [f32; 2] = [0.30, 0.46];

/// Worn: a quadrant of the foredeck re-laid in new pale boards, a hair proud
/// of the deck.
pub(super) fn deck_patch(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) {
    let l = hull.loa;
    let zf = hold_z(hull).1;
    let pts: Vec<_> = run_z(hull, zf + l * 0.03, zf + l * 0.12)
        .into_iter()
        .map(|z| {
            (
                [0.0, hull.sheer_z(z) - l * 0.002 + l * 0.0015, z],
                hull.half_beam_at(z) * 0.985,
            )
        })
        .collect();
    kids.push(sweep(
        &pts,
        8,
        [1.0, DECK_CROWN * 1.02, 1.0],
        PATCH_CUT,
        &c.patch,
        0.0,
    ));
}

/// Battered: a patch of tarred felt over half the barrel roof.
pub(super) fn roof_patch(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    h: &House,
) {
    let l = hull.loa;
    let eave = h.top - l * 0.002;
    let r = h.roof_r * 1.012;
    kids.push(sweep(
        &[
            ([0.0, eave, h.za + h.ln * 0.10], r),
            ([0.0, eave, h.za + h.ln * 0.55], r),
        ],
        12,
        [1.0, h.roof_s * 1.02, 1.0],
        [0.25, 0.5],
        &c.felt,
        0.0,
    ));
}

/// Battered: a weathered tarp thrown over the after half of the load - a
/// cloth ON the stack, not a vault over the hold (a gunwale-to-gunwale tarp
/// is a covered wagon): an upper half-pipe whose flat underside lies halfway
/// up the stack and whose crown clears its `top`, so the lower courses show
/// under its hem, and whose ends taper so it rounds down over the stack.
pub(super) fn tarp_over(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    hold: &Hold,
    top: f32,
) {
    let l = hull.loa;
    let (z0, z1) = (hold.at(0.0, 0.0)[2], hold.at(0.60, 0.0)[2]);
    let base = hold.y + (top - hold.y) * 0.5;
    let r = hold.w * 0.97;
    let s = (top + l * 0.018 - base) / r;
    kids.push(sweep(
        &[
            ([0.0, base, z0], r * 0.78),
            ([0.0, base, (z0 + z1) * 0.5], r),
            ([0.0, base, z1], r * 0.78),
        ],
        12,
        [1.0, s, 1.0],
        [0.0, 0.5],
        &c.tarp,
        0.0,
    ));
}
