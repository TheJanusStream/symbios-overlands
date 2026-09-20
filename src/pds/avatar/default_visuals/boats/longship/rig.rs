//! Her square rig: one mast stepped amidships, one yard, one sail in five
//! apparent bands, two braces, and the masthead VANE that is her only lit
//! slot.
//!
//! # The cap counts the DRAWN top, not the masthead
//!
//! A square rig crosses its yard BELOW the masthead and hangs its sail under
//! the yard, so the mast is the tallest thing she carries and the air-draft
//! cap is held by CONSTRUCTION rather than by a reading (the sloop's rule,
//! #1366). But the mast is not the drawn top: her vane stands 0.011 L over
//! it and an Ornate banner 0.020 L, so the room the cap leaves is the room
//! MINUS the tallest fitting's own reach ([`FITTING_REACH`]). Clamping the
//! mast alone left the drawn top at 2.787 m of a 2.8 m cap with the whole
//! margin spent - #1378's finding 7, here again in a rig.
//!
//! # The sail is sized to the HULL, and her path runs ATHWARTSHIPS
//!
//! Her width is a fraction of the LENGTH - a longship's yard is longer than
//! she is wide by half again - and her foot hangs a hair over the gunwale,
//! so the HOIST is derived from the two and the cloth fills the rig the way
//! a square sail does instead of floating in the middle of a bare mast.
//!
//! The cloth is ONE [`flat_sweep`] whose path runs ATHWARTSHIPS, flattened
//! across Z. That is the junk's flattened-board idiom turned a quarter, and
//! it is what makes a [`band`] of it a VERTICAL stripe: each section is the
//! cloth's own vertical extent at that x, so the outline is free - the head
//! stays straight on the yard while the foot sags by [`SAIL_ROACH`].
//!
//! A STRIPE is that same sweep cut to a band and a third THICKER, so it
//! sandwiches the base cloth instead of lying coplanar with it: adjacent
//! bands would z-fight, a thicker band cannot. FIVE apparent bands cost TWO
//! stripe nodes, because the base cloth shows between and outside them.

use crate::pds::generator::Generator;
use crate::seeded_defaults::WearTier;

use super::super::profile::HullProfile;
use super::super::shape::{band, flat_sweep, line};
use super::super::{AIR_DRAFT_CAP, AIR_DRAFT_MARGIN, LongshipColours, MIN_DIM, hover};
use super::hull::{FLOOR, HOLLOW, depth_at, inner_half_width};

/// Where she steps her mast, as a fraction of the length: a SQUARE rig steps
/// amidships, unlike the sloop's fore-and-aft rig at 0.196 L.
const MAST_ZF: f32 = 0.020;

/// The mast, as a fraction of the length - the brief's 0.5 to 0.6 L - and
/// how far up it the yard is hoisted.
const MAST_PER_LOA: f32 = 0.560;
const YARD_F: f32 = 0.88;

/// The sail's FULL width as a fraction of the length, how far her foot
/// clears the gunwale, how much that foot sags over the hoist, the cloth's
/// thickness, and how many stations her flattened path carries port to
/// starboard.
const SAIL_W: f32 = 0.620;
const SAIL_CLEAR: f32 = 0.045;
const SAIL_ROACH: f32 = 0.085;
const SAIL_SZ: f32 = 0.0110;
const SAIL_STATIONS: usize = 7;

/// The two bands that make FIVE apparent stripes on a seven-station sail:
/// cloth, stripe, cloth, stripe, cloth.
const STRIPES: [(usize, usize); 2] = [(1, 2), (4, 5)];

/// The share of [`SAIL_SZ`] the base cloth is actually drawn at: a stripe
/// band is 1.34 of this and still well under the nominal gauge, so the two
/// sandwich rather than z-fight.
const CLOTH_F: f32 = 0.30;

/// The replaced panel a WORN or BATTERED sail carries, as a band of her own
/// cloth at the port head.
const PATCH: (usize, usize) = (0, 1);

/// How far the tallest masthead fitting stands over the mast's top, as a
/// fraction of the length: the Ornate banner's reach, which is the one the
/// cap has to leave room for. See the module docs.
const FITTING_REACH: f32 = 0.020;

/// Her rig, resolved against the air-draft cap.
pub(super) struct Rig {
    /// Where the mast is stepped, and the height of the bottom boards it
    /// stands on.
    pub(super) step_z: f32,
    pub(super) step_y: f32,
    /// The mast's top, and the yard crossed below it.
    pub(super) top: f32,
    yard_y: f32,
    /// Half the yard's span, and the sail's derived hoist under it.
    half: f32,
    hoist: f32,
    mast_r: f32,
}

impl Rig {
    pub(super) fn new(hull: &HullProfile) -> Self {
        let l = hull.loa;
        let step_z = MAST_ZF * l;
        let step_y = hull.sheer_z(step_z) - depth_at(hull, step_z) * HOLLOW * FLOOR;
        // The room the cap leaves over her step, less the tallest fitting's
        // own reach - see the module docs.
        let room =
            AIR_DRAFT_CAP - hover(hull.draft) - AIR_DRAFT_MARGIN - step_y - l * FITTING_REACH;
        let height = (MAST_PER_LOA * l).min(room);
        let yard_y = step_y + height * YARD_F;
        let foot_y0 = hull.sheer_z(step_z) + l * SAIL_CLEAR;
        Self {
            step_z,
            step_y,
            top: step_y + height,
            yard_y,
            half: SAIL_W * l * 0.5,
            hoist: (yard_y - foot_y0).max(l * 0.10),
            mast_r: l * 0.0135,
        }
    }

    /// The foot's height at `f` of the half width out from the mast: a
    /// shallow sag, so the cloth reads as cloth and the head stays straight
    /// on the yard.
    fn foot_y(&self, f: f32) -> f32 {
        self.yard_y - self.hoist * (1.0 - SAIL_ROACH * (1.0 - f * f))
    }

    /// `(centre, half-height)` across the yard, port to starboard: the
    /// flattened sweep's path, whose every section is the sail's own
    /// vertical extent at that x.
    fn sail_points(&self) -> Vec<([f32; 3], f32)> {
        (0..SAIL_STATIONS)
            .map(|k| {
                let f = -1.0 + 2.0 * k as f32 / (SAIL_STATIONS - 1) as f32;
                let top = self.yard_y - self.hoist * 0.015;
                let bot = self.foot_y(f.abs());
                (
                    [f * self.half, (top + bot) * 0.5, self.step_z],
                    (top - bot) * 0.5,
                )
            })
            .collect()
    }
}

/// The height (m, over her waterline) the rig is resolved to - the mast's
/// top, which on a square rig is the tallest spar she carries.
#[cfg(test)]
pub(super) fn top_of_rig(hull: &HullProfile) -> f32 {
    Rig::new(hull).top
}

/// Whether the air-draft cap actually CLAMPED this hull's mast, rather than
/// her carrying the whole [`MAST_PER_LOA`]. What makes the cap a real bound
/// on her rather than a vacuous one: it binds on her long hulls, and a guard
/// that never saw it bind would prove nothing about the clamp.
#[cfg(test)]
pub(super) fn mast_is_capped(hull: &HullProfile) -> bool {
    Rig::new(hull).top - Rig::new(hull).step_y < MAST_PER_LOA * hull.loa - 1e-9
}

/// The mast, the yard, the sail and her two braces.
pub(super) fn rig(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &LongshipColours,
    r: &Rig,
    wear: WearTier,
) {
    let l = hull.loa;
    kids.push(line(
        &[
            ([0.0, r.step_y - l * 0.004, r.step_z], r.mast_r * 1.25),
            ([0.0, r.yard_y, r.step_z], r.mast_r),
            ([0.0, r.top, r.step_z], r.mast_r * 0.68),
        ],
        8,
        &c.spar,
    ));
    // The yard, laid athwartships, with a truss at its middle where it
    // crosses the mast.
    let yr = l * 0.0090;
    kids.push(line(
        &[
            ([-r.half * 1.03, r.yard_y, r.step_z], yr * 0.72),
            ([0.0, r.yard_y, r.step_z], yr),
            ([r.half * 1.03, r.yard_y, r.step_z], yr * 0.72),
        ],
        6,
        &c.spar,
    ));
    cloth(kids, hull, c, r, wear);
    // The two braces, from the yard arms down and aft to the gunwale.
    for side in [-1.0f32, 1.0] {
        let z = -0.300 * l;
        let d = depth_at(hull, z) * 0.10;
        let hw = inner_half_width(hull, z, d, 1.0);
        kids.push(line(
            &[
                ([side * r.half * 1.02, r.yard_y, r.step_z], l * 0.0032),
                ([side * hw, hull.sheer_z(z) - d, z], l * 0.0030),
            ],
            4,
            &c.rope,
        ));
    }
}

/// The cloth, her two stripe bands and - on a worn or battered kit - the
/// replaced panel at her port head. See the module docs for why a band is a
/// vertical stripe and why it is a third thicker than the cloth it lies on.
fn cloth(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &LongshipColours,
    r: &Rig,
    wear: WearTier,
) {
    let pts = r.sail_points();
    let at = [0.0, 0.0, r.step_z];
    let half_max = pts.iter().map(|&(_, q)| q).fold(0.0f32, f32::max);
    // Flattened to a real thickness - a share of the cloth's nominal gauge,
    // since the band that sandwiches it is a third thicker again - and
    // floored a hair over the sanitiser's own minimum so the record
    // round-trips untouched at the small end of the blueprint.
    let thick = (hull.loa * SAIL_SZ * CLOTH_F).max(MIN_DIM * 1.06);
    let s = thick / (2.0 * half_max);
    kids.push(flat_sweep(&pts, 16, 2, s, &c.sail, at));
    for (i0, i1) in STRIPES {
        kids.push(band(
            flat_sweep(&pts, 16, 2, s * 1.34, &c.stripe, at),
            i0,
            i1,
        ));
    }
    if wear != WearTier::Pristine {
        kids.push(band(
            flat_sweep(&pts, 16, 2, s * 1.22, &c.patch, at),
            PATCH.0,
            PATCH.1,
        ));
    }
}

/// The masthead VANE, on every longship: the Norse vindfloy, a small gilded
/// blade standing aft off the mast's top in the seeded accent.
///
/// It is her ONE lit slot on a luminous kit, and it is what the boats' glow
/// rule asks for - small and HIGH, the sloop's burgee by another name (#1365
/// phase 2). Her other identity surfaces, the shield row along the gunwale
/// and the stripes down her sail, are a hull line and the largest mass she
/// carries; neither may glow.
///
/// Drawn as a flattened BOARD, never a turned knob: a lollipop at the
/// masthead is rule 7's own example of what not to do.
pub(super) fn vane(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours, r: &Rig) {
    let l = hull.loa;
    let pts = [
        ([0.0, r.top - l * 0.008, r.step_z - l * 0.004], l * 0.019),
        ([0.0, r.top - l * 0.016, r.step_z - l * 0.038], l * 0.015),
        ([0.0, r.top - l * 0.026, r.step_z - l * 0.064], l * 0.0070),
    ];
    let widest = pts.iter().map(|&(_, q)| q).fold(0.0f32, f32::max);
    kids.push(flat_sweep(
        &pts,
        10,
        0,
        l * 0.0030 / widest,
        &c.vane,
        [0.0; 3],
    ));
}

/// The ORNATE mass: a long banner streaming aft from the masthead - small
/// and HIGH, which is the boats' glow rule's own shape, and the only thing
/// she carries over her yard.
///
/// It wears the vane's material, not one of its own, and that is a fact of
/// the livery rather than an oversight: the vane and the banner are one
/// cloth in life - the same accent held clear of the same sail - so at the
/// fullest tiers a repaint of one IS a repaint of both. See
/// [`LongshipColours::vane`](super::super::LongshipColours::vane).
pub(super) fn banner(kids: &mut Vec<Generator>, hull: &HullProfile, c: &LongshipColours, r: &Rig) {
    let l = hull.loa;
    let pts = [
        ([0.0, r.top - l * 0.010, r.step_z - l * 0.016], l * 0.030),
        ([0.0, r.top - l * 0.022, r.step_z - l * 0.100], l * 0.023),
        ([0.0, r.top - l * 0.038, r.step_z - l * 0.180], l * 0.011),
    ];
    let widest = pts.iter().map(|&(_, q)| q).fold(0.0f32, f32::max);
    kids.push(flat_sweep(
        &pts,
        10,
        0,
        l * 0.0030 / widest,
        &c.vane,
        [0.0; 3],
    ));
}
