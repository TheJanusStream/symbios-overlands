//! The airship family's one profile (#1657): a hull's radius along its
//! length, which every mount reads.
//!
//! The boats' `HullProfile` and the helicopters' `PodPlan` for a hull of
//! revolution. A type states the SHAPE of its hull - where the full girth
//! begins and ends, how blunt the nose is, how the tail tapers, and the
//! stations its sweep is laid through - as a [`HullProfile`]; the seeded
//! blueprint sets the SIZE, its length and its fineness. [`HullPlan`] joins
//! the two, and everything a type hangs on its hull - a control car's roof,
//! an engine car's struts, a fin's root, a window band laid on the skin -
//! asks the plan where the skin is rather than carrying a number of its own,
//! so nothing can float off a hull the blueprint has grown or slimmed.
//!
//! Every type's prototype was drawn at the nominal blueprint, 3.6 m at
//! fineness 5.0, and a fitting's own size - a car's girth, a nacelle's, a
//! lamp's - is authored in that prototype's metres and drawn
//! [`HullPlan::len`] times larger: the length over the nominal's. The hull's
//! radius is not a fitting: it is the blueprint's own, times the type's
//! shares of its length and fineness, so a fuller hull carries the same car
//! on a deeper belly.

use crate::seeded_defaults::{AirshipBlueprint, NOMINAL_AIRSHIP_LEN};

use super::MIN_DIM;

/// A type's hull shape, as its prototype drew it.
///
/// The radius runs from the nose (share `x = 0`) to the tail (`x = 1`) in
/// three pieces: a superellipse quadrant from the nose's tip to the full
/// girth at `nose` (exponent 2 is an ellipse, higher a fuller, blunter
/// nose), the full girth held for `parallel`, and a power-law taper to a
/// point at the tail (2 is a parabola, under it a straighter cone).
///
/// A hull stood on end - the hot-air balloon's envelope, the floater's
/// bell - reads the same shape from its crown (`x = 0`) down, and its
/// builder lays the stations up its height ([`HullPlan::upright`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HullProfile {
    /// Share of the length from the nose's tip to the full girth.
    pub(crate) nose: f32,
    /// Share of the length held at the full girth.
    pub(crate) parallel: f32,
    /// The nose's superellipse exponent.
    pub(crate) nose_power: f32,
    /// The tail's taper power.
    pub(crate) tail_power: f32,
    /// The hull sweep's stations, as shares of the length from the nose -
    /// at most the sanitiser's sixteen, close at a blunt nose where the
    /// radius climbs fastest, and no adjacent gap more than twice its
    /// neighbour (a uniform Catmull-Rom loops past a station otherwise).
    pub(crate) stations: &'static [f32],
    /// The type's hull length over the blueprint's (the Zeppelin's 1): a
    /// blimp is shorter, a balloon's envelope stands half as tall.
    pub(crate) len_scale: f32,
    /// The type's fineness over the blueprint's (the Zeppelin's 1): under
    /// one is a fuller hull - a blimp's 0.8, a balloon's quarter.
    pub(crate) fine_scale: f32,
}

/// A seeded hull: a type's [`HullProfile`] at a blueprint's size.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HullPlan {
    profile: HullProfile,
    /// Nose tip to tail tip (m).
    length: f32,
    /// The greatest radius (m).
    girth: f32,
    /// The blueprint's length over the nominal's: what a fitting authored
    /// in the prototype's metres is drawn [`Self::len`] times larger by -
    /// the blueprint's, not the hull's, so a type's shorter hull keeps
    /// full-size fittings.
    scale: f32,
}

impl HullPlan {
    /// The hull `profile` draws at `bp`'s length and fineness, each at the
    /// type's own scale on them.
    pub(crate) fn new(bp: &AirshipBlueprint, profile: HullProfile) -> Self {
        let length = bp.length * profile.len_scale;
        Self {
            profile,
            length,
            girth: length / (2.0 * bp.fineness * profile.fine_scale),
            scale: bp.length / NOMINAL_AIRSHIP_LEN,
        }
    }

    /// Nose tip to tail tip (m).
    pub(crate) fn length(&self) -> f32 {
        self.length
    }

    /// The greatest radius (m).
    pub(crate) fn girth(&self) -> f32 {
        self.girth
    }

    /// A fitting's size `v`, authored in the prototype's metres, as this
    /// hull draws it: grown with the length over the nominal's.
    pub(crate) fn len(&self, v: f32) -> f32 {
        v * self.scale
    }

    /// The radius at share `x` from the nose, as a share of the greatest -
    /// zero at both tips.
    pub(crate) fn shape(&self, x: f32) -> f32 {
        let p = &self.profile;
        if x <= 0.0 || x >= 1.0 {
            return 0.0;
        }
        if x < p.nose {
            let u = 1.0 - x / p.nose;
            return (1.0 - u.powf(p.nose_power)).powf(1.0 / p.nose_power);
        }
        if x < p.nose + p.parallel {
            return 1.0;
        }
        let v = (x - p.nose - p.parallel) / (1.0 - p.nose - p.parallel);
        1.0 - v.powf(p.tail_power)
    }

    /// The skin's radius at share `x` (m), never under the sanitiser's floor
    /// - so the two tips, where the shape is zero, are a 1 cm disc.
    pub(crate) fn radius(&self, x: f32) -> f32 {
        (self.shape(x) * self.girth).max(MIN_DIM)
    }

    /// The `z` of share `x`: the nose's tip at `+length / 2`, the tail's at
    /// `-length / 2`.
    pub(crate) fn z(&self, x: f32) -> f32 {
        self.length * (0.5 - x)
    }

    /// The share of the length at `z` - [`Self::z`] undone.
    pub(crate) fn x(&self, z: f32) -> f32 {
        0.5 - z / self.length
    }

    /// The hull sweep's stations nose to tail, each `proud` (m) off the
    /// skin - a skin laid on the hull (a window band, a stripe, a cap)
    /// reuses the hull's OWN stations a few millimetres out, or the hull's
    /// spline pokes through it between them.
    pub(crate) fn stations(&self, proud: f32) -> Vec<([f32; 3], f32)> {
        self.profile
            .stations
            .iter()
            .map(|&x| ([0.0, 0.0, self.z(x)], self.radius(x) + proud))
            .collect()
    }

    /// The stations of a hull stood on end - crown up, at `+length / 2` -
    /// each `proud` (m) off the skin.
    pub(crate) fn upright(&self, proud: f32) -> Vec<([f32; 3], f32)> {
        self.profile
            .stations
            .iter()
            .map(|&x| ([0.0, self.z(x), 0.0], self.radius(x) + proud))
            .collect()
    }
}

/// `n` equal wedges round a sweep, every other one from `phase` turns: the
/// stripes of a two-colour gored envelope, as `path_cut` ranges. `n` is even,
/// and a wedge never crosses the `0`/`1` seam for a `phase` under `1 / n`.
pub(crate) fn gores(n: usize, phase: f32) -> Vec<[f32; 2]> {
    let w = 1.0 / n as f32;
    (0..n / 2)
        .map(|k| {
            let a = phase + 2.0 * k as f32 * w;
            [a, a + w]
        })
        .collect()
}

/// The `path_cut` wedges of a band down each flank, from `lo` to `hi`
/// degrees above her widest line (negative is under it), for a sweep drawn
/// NOSE TO TAIL.
///
/// A sweep transports its ring's frame along its path, and one drawn toward
/// `-Z` comes out mirrored: in `path_cut` turns `0.5` is `+X`, `0.25` the
/// crown, `0.75` the keel and `0` (`1`) `-X`. A band is a wedge either side,
/// mirrored about the crown and the keel so the flanks match - and a wedge
/// may not cross the `0`/`1` seam, so a band that crosses her widest line
/// is drawn on the `-X` flank as two, one either side of the seam.
pub(crate) fn flank_bands(lo_deg: f32, hi_deg: f32) -> Vec<[f32; 2]> {
    debug_assert!(lo_deg < hi_deg, "a flank band runs {lo_deg}..{hi_deg}");
    let (lo, hi) = (lo_deg / 360.0, hi_deg / 360.0);
    let mut out = vec![[0.5 - hi, 0.5 - lo]];
    if hi <= 0.0 {
        out.push([1.0 + lo, 1.0 + hi]);
    } else if lo >= 0.0 {
        out.push([lo, hi]);
    } else {
        out.push([1.0 + lo, 1.0]);
        out.push([0.0, hi]);
    }
    out
}
