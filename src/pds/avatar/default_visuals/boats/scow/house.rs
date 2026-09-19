//! The deckhouse aft - the one mass the chase camera sees whole, and so where
//! the scow wears her scheme - and the stovepipe up through its roof.

use crate::pds::generator::Generator;

use super::super::super::common::quat_z;
use super::super::ScowColours;
use super::super::profile::HullProfile;
use super::super::shape::{UPRIGHT, line, panel, sweep, turned};
use super::HOUSE;
use super::hull::deck_y;

/// The walls' height over the deck at their lowest foot, as a fraction of
/// the length.
const WALL_H: f32 = 0.110;

/// The walls' half-width over the deck's half-beam.
const HOUSE_HALF: f32 = 0.70;

/// The barrel roof's radius over the walls' half-width, and its rise over
/// that radius.
const ROOF_OVER: f32 = 1.14;
const ROOF_RISE: f32 = 0.30;

/// Where the deckhouse stands: every part of it, and every mount over it,
/// reads these.
pub(super) struct House {
    pub(super) za: f32,
    pub(super) zf: f32,
    pub(super) zm: f32,
    /// Its length (m).
    pub(super) ln: f32,
    /// The walls' half-width (m).
    pub(super) hw: f32,
    /// The box's floor, bedded under the deck, and the walls' top.
    base: f32,
    pub(super) top: f32,
    /// The barrel roof's radius, and its section factor.
    pub(super) roof_r: f32,
    pub(super) roof_s: f32,
}

impl House {
    pub(super) fn new(hull: &HullProfile) -> Self {
        let l = hull.loa;
        let (za, zf) = (HOUSE.0 * l, HOUSE.1 * l);
        let zm = (za + zf) * 0.5;
        let hw = hull.half_beam_at(za).min(hull.half_beam_at(zf)) * HOUSE_HALF;
        // The deck under it rises aft with the sheer; the box's floor is
        // bedded under the deck at the house's lowest wall foot.
        let foot = [za, zm, zf]
            .into_iter()
            .map(|z| deck_y(hull, z, hw))
            .fold(f32::INFINITY, f32::min);
        Self {
            za,
            zf,
            zm,
            ln: zf - za,
            hw,
            base: foot - l * 0.008,
            top: foot + l * WALL_H,
            roof_r: hw * ROOF_OVER,
            roof_s: ROOF_RISE,
        }
    }

    /// The top of the barrel roof `x` off the centreline.
    pub(super) fn roof_y(&self, x: f32) -> f32 {
        self.top + self.roof_r * self.roof_s * (1.0 - (x / self.roof_r).powi(2)).max(0.0).sqrt()
    }

    /// Where the stovepipe stands on the roof: its foot, its height, and its
    /// radius.
    fn stovepipe(&self, hull: &HullProfile) -> ([f32; 3], f32, f32) {
        let l = hull.loa;
        let x = -self.hw * 0.42;
        let z = self.zf - self.ln * 0.28;
        ([x, self.roof_y(x) - l * 0.010, z], l * 0.150, l * 0.0115)
    }

    /// The stovepipe's open top, knocked `lean` radians askew about the
    /// length - the Steam mount, which is why [`BoatCraft::fx_mount`] takes
    /// the seed: a battered scow's steam leaves her leaning pipe.
    ///
    /// [`BoatCraft::fx_mount`]: super::super::BoatCraft::fx_mount
    pub(super) fn stovepipe_top(&self, hull: &HullProfile, lean: f32) -> [f32; 3] {
        let ([x, y, z], h, _) = self.stovepipe(hull);
        [x - lean.sin() * h, y + lean.cos() * h, z]
    }
}

/// A timber-boarded box aft under an overhanging barrel roof, the roof trim
/// along each of its shoulders, a lit window band down each side and two
/// either side of the door astern.
pub(super) fn deckhouse(kids: &mut Vec<Generator>, hull: &HullProfile, c: &ScowColours) -> House {
    let l = hull.loa;
    let h = House::new(hull);
    let wall_h = h.top - h.base;
    kids.push(panel(
        [h.hw * 2.0, wall_h, h.ln],
        &c.house,
        [0.0, (h.base + h.top) * 0.5, h.zm],
        UPRIGHT,
        l * 0.012,
    ));
    // The barrel roof: an upper half-pipe, overhanging all round.
    let over = l * 0.014;
    let eave = h.top - l * 0.002;
    kids.push(sweep(
        &[
            ([0.0, eave, h.za - over], h.roof_r),
            ([0.0, eave, h.zf + over], h.roof_r),
        ],
        12,
        [1.0, h.roof_s, 1.0],
        [0.0, 0.5],
        &c.roof,
        0.0,
    ));
    // The roof trim: a strip along each crown shoulder of the roof, bedded
    // half into it, in the seed's accent - her identity line, in frame from
    // above where a topsides line never is (#1365), and lit through `trim`
    // on a luminous style (the canal barge's neon outline). At 0.80 of the
    // roof's radius, inside its SAMPLED extent: the guard samples a tube at
    // six ring points, so its box reaches only 0.866 of the radius across and
    // a strip further out is rejected before it is tested.
    let xr = h.roof_r * 0.80;
    let yr = h.roof_y(xr);
    for side in [-1.0f32, 1.0] {
        kids.push(line(
            &[
                ([side * xr, yr, h.za - over * 0.6], l * 0.0055),
                ([side * xr, yr, h.zf + over * 0.6], l * 0.0055),
            ],
            6,
            &c.trim,
        ));
    }
    // Lit window bands - no glass volume (#1359 rule 4) - down each side and
    // either side of the door astern.
    let band_h = l * 0.024;
    let wy = h.base + wall_h * 0.62;
    for side in [-1.0f32, 1.0] {
        kids.push(panel(
            [l * 0.006 + 0.012, band_h, h.ln * 0.58],
            &c.window,
            [side * h.hw, wy, h.zm + h.ln * 0.04],
            UPRIGHT,
            0.0,
        ));
    }
    kids.push(panel(
        [h.hw * 0.62, wall_h * 0.78, 0.012 + l * 0.004],
        &c.door,
        [0.0, h.base + wall_h * 0.47, h.za],
        UPRIGHT,
        0.0,
    ));
    for side in [-1.0f32, 1.0] {
        kids.push(panel(
            [h.hw * 0.42, band_h, 0.012 + l * 0.004],
            &c.window,
            [side * h.hw * 0.66, wy, h.za],
            UPRIGHT,
            0.0,
        ));
    }
    h
}

/// A turned stovepipe up through the roof with a collar at its head, bored
/// open at the top - the Steam mount - knocked `lean` radians askew on a
/// battered scow. At uniform scale, so the lean is a rotation the sanitiser
/// and the guard both read straight.
pub(super) fn stovepipe(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    h: &House,
    lean: f32,
) {
    let (foot, height, r) = h.stovepipe(hull);
    kids.push(turned(
        &[
            (r, 0.0),
            (r, height * 0.90),
            (r * 1.55, height * 0.97),
            (r * 1.55, height),
        ],
        12,
        false,
        &c.iron,
        foot,
        quat_z(lean),
        0.62,
    ));
}
