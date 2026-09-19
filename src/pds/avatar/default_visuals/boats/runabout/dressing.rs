//! The runabout's ladder of secondary masses by ornateness and wear (#1372):
//! the pieces more than one variant carries. Each is a MASS that reads at
//! play distance, and each wear mark is on a DECK, because at the chase
//! camera's 22.9 degree down-angle a boat's topsides roll away under her deck
//! edge - a primer patch on the topsides was drawn three times in phase 1
//! and never showed (#1365's lesson, rendered again).

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::WearTier;

use super::super::profile::HullProfile;
use super::super::{RunaboutColours, dim};
use super::hull::{DECK_CROWN, UPRIGHT, box_at, line, panel, run_z, sole_depth, sweep};

/// The primer panel's angular band on the deck's UPPER half-pipe: one
/// quadrant, the centreline to just inside the deck edge - on the side the
/// chase camera sees after the travel yaw (checked by render).
const PRIMER_CUT: [f32; 2] = [0.25, 0.49];

/// A transom ensign on a raked chrome staff - the launch's Adorned mark.
pub(super) fn flagstaff(kids: &mut Vec<Generator>, hull: &HullProfile, c: &RunaboutColours) {
    let l = hull.loa;
    let t = hull.transom_z();
    let y = hull.sheer_z(t);
    let top = [0.0, y + l * 0.16, t - l * 0.04];
    kids.push(line(
        &[
            ([0.0, y - l * 0.004, t + l * 0.012], l * 0.0055),
            (top, l * 0.0045),
        ],
        6,
        &c.chrome,
    ));
    kids.push(box_at(
        [dim(l * 0.004), l * 0.040, l * 0.060],
        &c.flag,
        [0.0, top[1] - l * 0.024, top[2] - l * 0.030 + l * 0.004],
    ));
}

/// A canvas canopy on two chrome hoops over the cockpit from `za` to `zf` -
/// the Ornate launch's big mass.
pub(super) fn surrey_top(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    (za, zf): (f32, f32),
) {
    let l = hull.loa;
    let h = l * 0.25;
    let r = l * 0.0055;
    let mut hoops = [(0.0f32, 0.0f32, 0.0f32); 2];
    for (i, z) in [zf - l * 0.045, za + l * 0.015].into_iter().enumerate() {
        let hb = hull.half_beam_at(z) * 0.94;
        let y = hull.sheer_z(z);
        kids.push(line(
            &[
                ([-hb, y - l * 0.004, z], r),
                ([-hb * 0.97, y + h, z], r),
                ([0.0, y + h + l * 0.020, z], r),
                ([hb * 0.97, y + h, z], r),
                ([hb, y - l * 0.004, z], r),
            ],
            8,
            &c.chrome,
        ));
        hoops[i] = (z, hb, y + h);
    }
    let [(z1, hb1, y1), (z2, hb2, y2)] = hoops;
    let over = l * 0.03;
    let canopy = [
        ([0.0, y2 + l * 0.004, z2 - over], hb2 * 1.02),
        (
            [0.0, (y1 + y2) * 0.5 + l * 0.004, (z1 + z2) * 0.5],
            (hb1 + hb2) * 0.51,
        ),
        ([0.0, y1 + l * 0.004, z1 + over], hb1 * 1.02),
    ];
    kids.push(sweep(
        &canopy,
        12,
        [1.0, 0.16, 1.0],
        [0.0, 0.5],
        &c.canvas,
        0.0,
    ));
}

/// A replaced foredeck panel in grey primer: one quadrant of the crowned
/// deck's own sweep from `z0` to `z1`, a hair proud of it.
fn primer_panel(hull: &HullProfile, c: &RunaboutColours, z0: f32, z1: f32) -> Generator {
    let l = hull.loa;
    let pts: Vec<_> = run_z(hull, z0, z1)
        .into_iter()
        .map(|z| {
            (
                [0.0, hull.sheer_z(z) - l * 0.002 + l * 0.0015, z],
                hull.half_beam_at(z) * 0.985,
            )
        })
        .collect();
    sweep(
        &pts,
        8,
        [1.0, DECK_CROWN * 1.02, 1.0],
        PRIMER_CUT,
        &c.primer,
        0.0,
    )
}

/// Where the engine hatch a tarp is lashed over lies: its centre, its length
/// and the height of its top.
pub(super) struct Hatch {
    pub(super) z: f32,
    pub(super) len: f32,
    pub(super) top: f32,
}

/// The monohull wear ladder. Worn: a primer panel let into the foredeck.
/// Battered: that, a tarp lashed over the engine hatch, and a fuel can in the
/// cockpit.
pub(super) fn wear_ladder(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &RunaboutColours,
    ctx: &PartCtx,
    (za, zf): (f32, f32),
    hatch: &Hatch,
) {
    let l = hull.loa;
    if matches!(ctx.wear, WearTier::Worn | WearTier::Battered) {
        kids.push(primer_panel(hull, c, 0.14 * l, 0.34 * l));
    }
    if ctx.wear == WearTier::Battered {
        let hb = hull.half_beam_at(hatch.z);
        let tarp = [
            (
                [0.0, hatch.top - l * 0.012, hatch.z - hatch.len * 0.55],
                hb * 0.58,
            ),
            ([0.0, hatch.top - l * 0.004, hatch.z], hb * 0.66),
            (
                [0.0, hatch.top - l * 0.012, hatch.z + hatch.len * 0.45],
                hb * 0.58,
            ),
        ];
        kids.push(sweep(&tarp, 10, [1.0, 0.30, 1.0], [0.0, 0.5], &c.tarp, 0.0));
        let z = za + l * 0.12;
        let x = hull.half_beam_at(z) * 0.52;
        let can = l * 0.050;
        let sole = hull.sheer_z((za + zf) * 0.5) - sole_depth(hull);
        kids.push(panel(
            [can * 0.55, can * 1.2, can * 0.95],
            &c.can,
            [x, sole + can * 0.6, z],
            UPRIGHT,
            l * 0.004,
        ));
    }
}
