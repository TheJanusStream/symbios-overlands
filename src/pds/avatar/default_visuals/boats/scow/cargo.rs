//! What the scow carries in her hold: one match over [`ScowLoad`], since
//! every load stands on the same floor in the same well.
//!
//! ORNATENESS is the load's height - from the chase camera the deckhouse
//! hides the hold's after end, so a load has to pile high to read, and a
//! Plain load mostly hiding behind the house is what a light load should do.
//! Pine crates, not dark oak: dark cargo on the dark hold floor never read.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::seeded_defaults::{OrnatenessTier, ScowLoad};

use super::super::super::common::{quat_x, quat_z};
use super::super::ScowColours;
use super::super::profile::HullProfile;
use super::super::shape::{UPRIGHT, panel, turned};
use super::hull::{floor_depth, inner_half_width};
use super::{hold_z, piled};

/// A packing crate's side, as a fraction of the length.
const CRATE: f32 = 0.118;

/// The hold's floor box: every load stands on it.
pub(super) struct Hold {
    /// The stowage's after and forward ends, a little inside the hold's.
    z0: f32,
    z1: f32,
    /// Its half-width, clear of the shell's inner face.
    pub(super) w: f32,
    /// The floor's top.
    pub(super) y: f32,
}

impl Hold {
    pub(super) fn new(hull: &HullProfile, floor_y: f32) -> Self {
        let l = hull.loa;
        let (za, zf) = hold_z(hull);
        let depth = floor_depth(hull) - l * 0.03;
        Self {
            z0: za + l * 0.02,
            z1: zf - l * 0.02,
            w: inner_half_width(hull, za, depth).min(inner_half_width(hull, zf, depth)) - l * 0.006,
            y: floor_y,
        }
    }

    /// A point on the floor: `u` aft (0) to forward (1), `v` port (-1) to
    /// starboard (1).
    pub(super) fn at(&self, u: f32, v: f32) -> [f32; 3] {
        [v * self.w, self.y, self.z0 + (self.z1 - self.z0) * u]
    }
}

/// Stow `load` for the ornateness tier. Returns the load's top over its
/// after half - what a battered scow's tarp is thrown over.
pub(super) fn stow(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    hold: &Hold,
    load: ScowLoad,
    orn: OrnatenessTier,
) -> f32 {
    match load {
        // The frontier scow carries freight; her stern wheel is her own.
        ScowLoad::Freight | ScowLoad::Sternwheel => freight(kids, hull, c, hold, orn),
        ScowLoad::Hay => hay(kids, hull, c, hold, orn),
        ScowLoad::Scrap => scrap(kids, hull, c, hold, orn),
    }
}

/// A crate of `size` standing on `at`; returns its top.
fn packing_crate(
    kids: &mut Vec<Generator>,
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    size: [f32; 3],
) -> f32 {
    kids.push(panel(
        size,
        m,
        [at[0], at[1] + size[1] * 0.5, at[2]],
        UPRIGHT,
        size[0] * 0.06,
    ));
    at[1] + size[1]
}

/// A cask stood on end.
fn cask(kids: &mut Vec<Generator>, c: &ScowColours, at: [f32; 3], r: f32, h: f32) {
    kids.push(turned(
        &[(r * 0.84, 0.0), (r, h * 0.5), (r * 0.84, h)],
        14,
        true,
        &c.cask,
        [at[0], at[1] - 0.002, at[2]],
        UPRIGHT,
        0.0,
    ));
}

/// A steel drum with two rolling hoops, stood on end; returns its top.
fn drum(
    kids: &mut Vec<Generator>,
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    r: f32,
    h: f32,
) -> f32 {
    kids.push(turned(
        &[
            (r, 0.0),
            (r, h * 0.30),
            (r * 1.05, h * 0.32),
            (r, h * 0.34),
            (r, h * 0.66),
            (r * 1.05, h * 0.68),
            (r, h * 0.70),
            (r, h),
        ],
        14,
        false,
        m,
        [at[0], at[1] - 0.002, at[2]],
        UPRIGHT,
        0.0,
    ));
    at[1] + h
}

/// A tyre lying flat; returns its top.
fn tyre(kids: &mut Vec<Generator>, c: &ScowColours, at: [f32; 3], r: f32, w: f32) -> f32 {
    kids.push(turned(
        &[(r, 0.0), (r, w)],
        16,
        false,
        &c.tyre,
        [at[0], at[1] - 0.002, at[2]],
        UPRIGHT,
        0.55,
    ));
    at[1] + w
}

/// A bale of `size` on `at`; returns its top.
fn bale(
    kids: &mut Vec<Generator>,
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    size: [f32; 3],
) -> f32 {
    kids.push(panel(
        size,
        m,
        [at[0], at[1] + size[1] * 0.5, at[2]],
        UPRIGHT,
        size[0] * 0.18,
    ));
    at[1] + size[1]
}

/// Which way a propped panel leans.
#[derive(Clone, Copy)]
enum Lean {
    /// About x, its top toward the bow.
    Fore,
    /// About z, its top toward port.
    Port,
}

/// A panel `[width, height, thickness]` standing on its bottom edge at
/// `foot` and leaning `lean` radians off the vertical - propped, not
/// floating. Returns its top.
fn leaning(
    kids: &mut Vec<Generator>,
    m: &SovereignMaterialSettings,
    foot: [f32; 3],
    [w, h, t]: [f32; 3],
    lean: f32,
    axis: Lean,
) -> f32 {
    let (centre, rotation, size) = match axis {
        Lean::Fore => (
            [
                foot[0],
                foot[1] + h * 0.5 * lean.cos(),
                foot[2] + h * 0.5 * lean.sin(),
            ],
            quat_x(lean),
            [w, h, t],
        ),
        Lean::Port => (
            [
                foot[0] - h * 0.5 * lean.sin(),
                foot[1] + h * 0.5 * lean.cos(),
                foot[2],
            ],
            quat_z(lean),
            [t, h, w],
        ),
    };
    kids.push(panel(size, m, centre, rotation, 0.0));
    foot[1] + h * lean.cos()
}

/// Crates and casks - every grimy theme's load, and the frontier's. Plain:
/// two crates and two casks. Adorned: a second pair of crates and one stacked
/// on the first. Ornate: another on the second and a third tier on top.
fn freight(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    hold: &Hold,
    orn: OrnatenessTier,
) -> f32 {
    let l = hull.loa;
    let cs = (l * CRATE).min(hold.w * 0.96);
    let ch = cs * 0.86;
    let (r, bh) = (l * 0.038, l * 0.105);
    let mut top = hold.y;
    for v in [-0.5, 0.5] {
        top = top.max(packing_crate(
            kids,
            &c.crate_a,
            hold.at(0.22, v),
            [cs, ch, cs * 0.94],
        ));
    }
    for dx in [-1.0, 1.0] {
        cask(kids, c, hold.at(0.80, dx * 0.40), r, bh);
    }
    if piled(orn) {
        for v in [-0.5, 0.5] {
            packing_crate(
                kids,
                &c.crate_b,
                hold.at(0.52, v),
                [cs, ch * 0.92, cs * 0.94],
            );
        }
        let [x, y, z] = hold.at(0.22, 0.0);
        top = top.max(packing_crate(
            kids,
            &c.crate_a,
            [x, y + ch - l * 0.004, z],
            [cs * 1.3, ch * 0.9, cs * 0.9],
        ));
    }
    if orn == OrnatenessTier::Ornate {
        let [x, y, z] = hold.at(0.52, 0.0);
        let y2 = packing_crate(
            kids,
            &c.crate_b,
            [x, y + ch * 0.92 - l * 0.004, z],
            [cs * 1.2, ch * 0.8, cs * 0.85],
        );
        let [x, _, z] = hold.at(0.37, 0.0);
        top = top.max(packing_crate(
            kids,
            &c.crate_a,
            [x, y2 - l * 0.004, z],
            [cs * 0.8, ch * 0.7, cs * 0.75],
        ));
    }
    top
}

/// The farm barge's bales in a stepped stack: one course Plain, two Adorned,
/// a three-high haystack Ornate - the stack barge's read.
fn hay(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    hold: &Hold,
    orn: OrnatenessTier,
) -> f32 {
    const SIX: &[(f32, f32)] = &[
        (0.17, -0.5),
        (0.17, 0.5),
        (0.50, -0.5),
        (0.50, 0.5),
        (0.83, -0.5),
        (0.83, 0.5),
    ];
    const FOUR: &[(f32, f32)] = &[(0.33, -0.5), (0.33, 0.5), (0.67, -0.5), (0.67, 0.5)];
    // A single course of four spreads wider than four stacked on six.
    const FOUR_ALONE: &[(f32, f32)] = &[(0.30, -0.5), (0.30, 0.5), (0.70, -0.5), (0.70, 0.5)];
    const TWO: &[(f32, f32)] = &[(0.50, -0.5), (0.50, 0.5)];
    let l = hull.loa;
    let (bl, bw, bh) = (l * 0.125, (l * 0.080).min(hold.w * 0.48), l * 0.062);
    let courses: &[&[(f32, f32)]] = match orn {
        OrnatenessTier::Plain => &[FOUR_ALONE],
        OrnatenessTier::Adorned => &[SIX, TWO],
        OrnatenessTier::Ornate => &[SIX, FOUR, TWO],
    };
    let mut top = hold.y;
    for (k, course) in courses.iter().enumerate() {
        for (i, &(u, v)) in course.iter().enumerate() {
            let [x, y, z] = hold.at(u, v * 0.98);
            let m = if (i + k) % 2 == 0 { &c.hay_a } else { &c.hay_b };
            let y = bale(
                kids,
                m,
                [x, y + k as f32 * (bh - l * 0.003), z],
                [bw, bh, bl],
            );
            if u <= 0.55 {
                top = top.max(y);
            }
        }
    }
    top
}

/// The wasteland's scrap heap: drums, a stack of tyres and sheet iron
/// propped against the side of the hold; piled higher with the tiers, a
/// rusted panel and a car's bonnet leant against the heap.
fn scrap(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &ScowColours,
    hold: &Hold,
    orn: OrnatenessTier,
) -> f32 {
    let l = hull.loa;
    let (r, h) = (l * 0.036, l * 0.095);
    let (tyre_r, tyre_w) = (l * 0.048, l * 0.024);
    let sheet_t = 0.012 + l * 0.004;
    let mut top = drum(kids, &c.drums[0], hold.at(0.18, -0.50), r, h);
    let [tx, _, tz] = hold.at(0.50, 0.45);
    let mut y = hold.y;
    for _ in 0..2 {
        y = tyre(kids, c, [tx, y, tz], tyre_r, tyre_w) - l * 0.001;
    }
    top = top.max(leaning(
        kids,
        &c.sheet,
        hold.at(0.74, -0.80),
        [l * 0.15, l * 0.12, sheet_t],
        -0.45,
        Lean::Port,
    ));
    if piled(orn) {
        drum(kids, &c.drums[1], hold.at(0.86, 0.50), r, h);
        drum(kids, &c.drums[2], hold.at(0.18, 0.30), r, h);
        y = tyre(kids, c, [tx, y, tz], tyre_r, tyre_w) - l * 0.001;
    }
    if orn == OrnatenessTier::Ornate {
        drum(kids, &c.drums[0], [tx, y, tz], r * 0.9, h * 0.85);
        top = top.max(leaning(
            kids,
            &c.rust,
            hold.at(0.02, 0.0),
            [l * 0.13, l * 0.13, sheet_t],
            0.55,
            Lean::Fore,
        ));
        leaning(
            kids,
            &c.car,
            hold.at(0.66, 0.05),
            [l * 0.11, l * 0.12, 0.012 + l * 0.006],
            0.60,
            Lean::Port,
        );
    }
    top
}
