//! The tug's works, forward of amidships - which is what makes her a tug
//! beside the scow's house aft: the engine casing on the deck, the
//! wheelhouse on its fore end under an overhanging roof, and the tall raked
//! funnel abaft that, turned. Every part of them, and every mount over them,
//! reads one [`Works`].

use crate::pds::generator::Generator;
use crate::seeded_defaults::WearTier;

use super::super::super::common::quat_x;
use super::super::TugColours;
use super::super::profile::HullProfile;
use super::super::shape::{UPRIGHT, panel, turned};
use super::hull::{deck_edge, deck_y};

/// The engine casing, aft end to fore end, and the wheelhouse on the
/// casing's fore end, as fractions of the length.
const CASING: (f32, f32) = (-0.200, 0.240);
const WHEELHOUSE: (f32, f32) = (0.080, 0.240);

/// The casing's height over the deck at its after end, as a fraction of the
/// length, and its half-width over the deck's.
const CASING_H: f32 = 0.060;
const CASING_HALF: f32 = 0.66;

/// The wheelhouse walls' height, as a fraction of the length, and their
/// half-width over the casing's.
const WH_H: f32 = 0.105;
const WH_HALF: f32 = 0.95;

/// Where the funnel's foot stands, its height and its barrel's radius, as
/// fractions of the length, and how far it rakes aft (rad).
const FUNNEL_ZF: f32 = -0.020;
const FUNNEL_H: f32 = 0.300;
const FUNNEL_R: f32 = 0.050;
const FUNNEL_RAKE: f32 = 0.16;

/// The company band's lower and upper edges, as fractions of the funnel's
/// height - under the soot at every wear.
const BAND: (f32, f32) = (0.46, 0.58);

/// Where the casing, the wheelhouse and the funnel stand: every part of
/// them, and every mount over them, reads these.
pub(super) struct Works {
    /// The casing's after end and its middle.
    pub(super) za: f32,
    zm: f32,
    /// The casing's length (m).
    ln: f32,
    /// The casing's half-width (m).
    pub(super) hw: f32,
    /// The casing's floor, bedded under the deck, and its top.
    pub(super) base: f32,
    pub(super) top: f32,
    /// The wheelhouse's after and fore ends, its walls' half-width and its
    /// walls' top.
    pub(super) wa: f32,
    wf: f32,
    whw: f32,
    pub(super) wtop: f32,
    /// The funnel's foot on the casing, its height and its barrel's radius.
    pub(super) funnel_foot: [f32; 3],
    funnel_h: f32,
    funnel_r: f32,
}

impl Works {
    pub(super) fn new(hull: &HullProfile) -> Self {
        let l = hull.loa;
        let (za, zf) = (CASING.0 * l, CASING.1 * l);
        let zm = (za + zf) * 0.5;
        let hw = [za, zm, zf]
            .into_iter()
            .map(|z| deck_edge(hull, z))
            .fold(f32::INFINITY, f32::min)
            * CASING_HALF;
        // The deck under it follows the sheer; the box's floor is bedded
        // under the deck at the casing's lowest wall foot, and its top is
        // level, measured from the after end.
        let foot = [za, zm, zf]
            .into_iter()
            .map(|z| deck_y(hull, z, hw))
            .fold(f32::INFINITY, f32::min);
        let top = deck_y(hull, za, hw) + l * CASING_H;
        Self {
            za,
            zm,
            ln: zf - za,
            hw,
            base: foot - l * 0.008,
            top,
            wa: WHEELHOUSE.0 * l,
            wf: WHEELHOUSE.1 * l,
            whw: hw * WH_HALF,
            wtop: top + l * WH_H,
            funnel_foot: [0.0, top - l * 0.010, FUNNEL_ZF * l],
            funnel_h: l * FUNNEL_H,
            funnel_r: l * FUNNEL_R,
        }
    }

    /// The wheelhouse's middle, fore and aft.
    pub(super) fn wheelhouse_zm(&self) -> f32 {
        (self.wa + self.wf) * 0.5
    }

    /// A point `h` up the funnel's raked axis.
    fn funnel_point(&self, h: f32) -> [f32; 3] {
        let [x, y, z] = self.funnel_foot;
        [x, y + FUNNEL_RAKE.cos() * h, z - FUNNEL_RAKE.sin() * h]
    }

    /// The funnel's mouth - the same function's point at the full height,
    /// rake included, which is where her steam and her embers leave it.
    pub(super) fn mouth(&self) -> [f32; 3] {
        self.funnel_point(self.funnel_h)
    }
}

/// The engine casing: a Bevel box on the deck from abaft the funnel to under
/// the wheelhouse, and its top, a deck in the roof's colour a hair proud.
pub(super) fn casing(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours, w: &Works) {
    let l = hull.loa;
    kids.push(panel(
        [w.hw * 2.0, w.top - w.base, w.ln],
        &c.house,
        [0.0, (w.base + w.top) * 0.5, w.zm],
        UPRIGHT,
        l * 0.016,
    ));
    kids.push(panel(
        [w.hw * 2.0 - l * 0.006, l * 0.010, w.ln - l * 0.006],
        &c.roof,
        [0.0, w.top + l * 0.001, w.zm],
        UPRIGHT,
        l * 0.014,
    ));
}

/// The wheelhouse on the casing's fore end: a Bevel box under an
/// overhanging roof, and a lit window band all round - front, sides AND
/// back, so the chase camera sees it from astern. No glass volume (#1359
/// rule 4).
pub(super) fn wheelhouse(kids: &mut Vec<Generator>, hull: &HullProfile, c: &TugColours, w: &Works) {
    let l = hull.loa;
    let ln = w.wf - w.wa;
    let zm = w.wheelhouse_zm();
    let h = w.wtop - w.top;
    kids.push(panel(
        [w.whw * 2.0, h + l * 0.006, ln],
        &c.house,
        [0.0, w.top + h * 0.5 - l * 0.003, zm],
        UPRIGHT,
        l * 0.012,
    ));
    let over = l * 0.014;
    kids.push(panel(
        [w.whw * 2.0 + over * 2.0, l * 0.012, ln + over * 2.0],
        &c.roof,
        [0.0, w.wtop + l * 0.004, zm],
        UPRIGHT,
        l * 0.020,
    ));
    let band_h = l * 0.030;
    let wy = w.top + h * 0.66;
    let t = 0.012 + l * 0.004;
    for side in [-1.0f32, 1.0] {
        kids.push(panel(
            [t, band_h, ln * 0.80],
            &c.window,
            [side * w.whw, wy, zm],
            UPRIGHT,
            0.0,
        ));
    }
    for z in [w.wf, w.wa] {
        kids.push(panel(
            [w.whw * 1.70, band_h, t],
            &c.window,
            [0.0, wy, z],
            UPRIGHT,
            0.0,
        ));
    }
}

/// How far down the funnel the soot reaches, as a fraction of its height: a
/// worn stack is sooted further down than a clean one.
fn soot_from(wear: WearTier) -> f32 {
    match wear {
        WearTier::Pristine => 0.84,
        WearTier::Worn => 0.72,
        WearTier::Battered => 0.66,
    }
}

/// The tall raked funnel, TURNED - the legacy smokestack's recipe: a fuller
/// base, a tapering barrel and a flared cap. The body in the company colour,
/// the band in the seed's accent, and a black top whose closed cap is the
/// dark mouth; a battered stack has rusted through at its base. All of them
/// on ONE foot and ONE rake, at uniform scale, so every band rakes with the
/// body and the mouth ([`Works::mouth`]) is the same point theirs is.
///
/// Each band's first profile point is a foot ring bedded INSIDE the barrel.
/// Do not tidy it away: the guard samples a turned part as rings at its
/// profile points, and a band standing only proud of the barrel reads to it
/// as floating wherever the barrel's own rings land on the band's edge
/// heights - and on screen it was a centimetre's gap.
pub(super) fn funnel(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &TugColours,
    w: &Works,
    wear: WearTier,
) {
    let l = hull.loa;
    let (r, h) = (w.funnel_r, w.funnel_h);
    let e = l * 0.0030;
    let rake = quat_x(-FUNNEL_RAKE);
    kids.push(turned(
        &[
            (r * 1.18, 0.0),
            (r * 1.06, h * 0.10),
            (r, h * 0.30),
            (r * 0.97, h * 0.86),
        ],
        20,
        true,
        &c.funnel,
        w.funnel_foot,
        rake,
        0.0,
    ));
    let foot = r * 0.95;
    kids.push(turned(
        &[
            (foot, h * BAND.0 - e),
            (r + e, h * BAND.0),
            (r * 0.99 + e, h * BAND.1),
        ],
        20,
        false,
        &c.band,
        w.funnel_foot,
        rake,
        0.0,
    ));
    let soot = soot_from(wear);
    kids.push(turned(
        &[
            (foot, h * soot - e),
            (r * 0.99 + e, h * soot),
            (r * 0.975 + e, h * 0.93),
            (r * 1.14, h * 0.975),
            (r * 1.12, h),
        ],
        20,
        false,
        &c.funnel_top,
        w.funnel_foot,
        rake,
        0.0,
    ));
    if wear == WearTier::Battered {
        kids.push(turned(
            &[
                (r * 1.18 + e, 0.0),
                (r * 1.06 + e, h * 0.10),
                (r * 0.998 + e, h * 0.26),
            ],
            20,
            true,
            &c.rust,
            w.funnel_foot,
            rake,
            0.0,
        ));
    }
}
