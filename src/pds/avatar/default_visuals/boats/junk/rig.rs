//! The junk's battened lug rig: two masts on every seed and a mizzen at
//! Ornate, every sail ONE flattened sweep whose stations are its battens.
//!
//! # A sail is a sweep laid leapfrog
//!
//! A Spine's section is perpendicular to its Catmull-Rom tangent (bevy's
//! `CubicCardinalSpline`, mirrored ends: `world_builder/prim/sweeps.rs`), so
//! a midline curving forward FANS its own sections. Laid LEAPFROG - each
//! station two steps along the normal to the chord wanted at the station
//! between, `M[i+1] = M[i-1] + 2 ds n[i]`, and the first a single step,
//! `M[1] = M[0] + ds n[0]`, for the mirrored start - every interior drawn
//! tangent is exactly perpendicular to its chord. So each station's chord IS
//! a batten lying on the cloth, and the tube flattened by its node's x-scale
//! is the sail: no torture anywhere, and the connectivity guard, which
//! honours node scale, reads each sail as the lens it is.
//!
//! The step is solved in closed form, not searched: every chord's direction
//! is a difference of the midline, each `ds` times a fixed vector, so the
//! directions do not move with `ds` and every half-chord - and so the peak -
//! is linear in it. Two layouts give the line and the third lands the peak
//! on its height. They are laid at 1 and 2, never 0: at 0 every tangent is
//! a zero vector.
//!
//! # One point on the cloth
//!
//! The boom, the yard, the battens and the wear bands all go through
//! [`Sail::point`]. A raked sail is LAID OUT raked by its own frame, never
//! rotated - a flattened sweep takes no rotation.
//!
//! # Each rig resolved by its top
//!
//! A yard's peak is the highest thing a junk carries, so it is what the
//! air-draft cap is resolved against: the sloop's rule (#1366). A sail the
//! cap leaves short keeps its shape - it flattens to a lower aspect before
//! its foot shrinks - so a big hull under the cap carries a smaller rig
//! rather than a sail too flat for its own fan (the 4.4 m corner's mizzen
//! folded its yard under its tack before this).

use crate::pds::generator::Generator;
use crate::seeded_defaults::WearTier;

use super::super::JunkColours;
use super::super::profile::HullProfile;
use super::super::shape::{band, flat_sweep, line};
use super::super::{AIR_DRAFT_CAP, AIR_DRAFT_MARGIN, hover};
use super::hull::{deck_level, deck_y};

/// The main rig's top over the waterline as a fraction of the length,
/// before the air-draft cap has its say.
const TOP_PER_LOA: f32 = 0.86;

/// A mast's radius at its heel, the yard's and the boom's, and a batten's,
/// as fractions of the length.
const MAST_R: f32 = 0.013;
const SPAR_R: f32 = 0.0080;
const BATTEN_R: f32 = 0.0060;

/// A sail's node x-scale: it flattens the swept tube to a lens across the
/// sail's plane.
const SAIL_SCALE: f32 = 0.010;

/// A worn mainsail's replaced panel lies between battens 3 and 4, and a
/// battered one's torn-out panel between battens 2 and 3.
const PATCH: usize = 3;
const TORN: usize = 2;

/// One sail's design. Lengths are fractions of the hull's.
struct Spec {
    /// The mast's station.
    zf: f32,
    /// The foot - the boom's length - and the share of it forward of the
    /// mast: a balanced lug.
    foot: f32,
    fwd: f32,
    /// How far every point of the boom clears the sheer under it.
    clear: f32,
    /// The share of the rig's top this sail's peak may reach.
    share: f32,
    /// Height over foot wanted, and the least it flattens to under the cap
    /// before its foot shrinks.
    aspect: f32,
    min_aspect: f32,
    /// Stations - the boom, the battens and the yard - and the fan: the
    /// yard's angle over the boom's in degrees, and the power it is laid
    /// by, near parallel low and fanning in the head.
    n: usize,
    peak: f32,
    fan: f32,
    /// The mast's forward rake in degrees.
    rake: f32,
    /// Which side of its mast the sail hangs (`+1` starboard), the mast's
    /// station athwartships as a fraction of the half-beam, and its radius
    /// over a mainmast's.
    side: f32,
    mast_x: f32,
    r: f32,
}

/// The mainsail: seven stations, 30 degrees of fan, on a mast a little
/// abaft amidships.
const MAIN: Spec = Spec {
    zf: -0.020,
    foot: 0.52,
    fwd: 0.24,
    clear: 0.030,
    share: 1.00,
    aspect: 1.30,
    min_aspect: 0.70,
    n: 7,
    peak: 30.0,
    fan: 1.8,
    rake: 0.0,
    side: 1.0,
    mast_x: 0.0,
    r: 1.0,
};

/// The foresail: six stations, 26 degrees, on a mast raked forward over her
/// bow.
const FORE: Spec = Spec {
    zf: 0.340,
    foot: 0.33,
    fwd: 0.26,
    clear: 0.030,
    share: 0.80,
    aspect: 1.70,
    min_aspect: 0.80,
    n: 6,
    peak: 26.0,
    fan: 1.8,
    rake: 9.0,
    side: 1.0,
    mast_x: 0.0,
    r: 1.0,
};

/// Ornate's mizzen: a small third sail stepped on the poop to port of the
/// tiller, overhanging the transom, on a mast 0.7 of the others'.
const MIZZEN: Spec = Spec {
    zf: -0.395,
    foot: 0.20,
    fwd: 0.26,
    clear: 0.030,
    share: 0.60,
    aspect: 1.35,
    min_aspect: 0.80,
    n: 5,
    peak: 24.0,
    fan: 1.6,
    rake: 0.0,
    side: -1.0,
    mast_x: -0.50,
    r: 0.70,
};

/// One battened lug sail: `n` stations, each a CHORD from the luff to the
/// leech - station 0 the boom, `n - 1` the yard, the rest battens.
///
/// Laid out in its OWN frame - `z` forward, `y` up, the tack at the origin,
/// the luff the line `z = 0`, upright - and turned into the root frame by
/// its rake about the tack, which keeps every chord perpendicular to its
/// tangent.
pub(super) struct Sail {
    n: usize,
    foot: f32,
    /// The mast's rake (radians), and the x of the sail's plane.
    rake: f32,
    x: f32,
    /// The tack in the root frame, `(z, y)`.
    tack: (f32, f32),
    /// The share of the foot forward of the mast, the mast's x, and its
    /// radius over a mainmast's.
    fwd: f32,
    mast_x: f32,
    mast_r: f32,
    /// Each chord's angle over the boom's (radians).
    theta: Vec<f32>,
    /// Each half-chord, and each luff end in the sail's own frame.
    half: Vec<f32>,
    own_luff: Vec<[f32; 2]>,
    own_leech: Vec<[f32; 2]>,
    own_mid: Vec<[f32; 2]>,
    /// The midline and each chord's two ends, in the root frame.
    mid: Vec<[f32; 3]>,
    luff: Vec<[f32; 3]>,
    leech: Vec<[f32; 3]>,
}

impl Sail {
    /// The sail `spec` describes, set on its tack with its peak at `top`
    /// over the waterline.
    fn new(spec: &Spec, tack: (f32, f32), x: f32, foot: f32, top: f32, mast_x: f32) -> Self {
        let n = spec.n;
        let peak = spec.peak.to_radians();
        let theta = (0..n)
            .map(|i| peak * (i as f32 / (n - 1) as f32).powf(spec.fan))
            .collect();
        let mut s = Self {
            n,
            foot,
            rake: spec.rake.to_radians(),
            x,
            tack,
            fwd: spec.fwd,
            mast_x,
            mast_r: spec.r,
            theta,
            half: Vec::with_capacity(n),
            own_luff: Vec::with_capacity(n),
            own_leech: Vec::with_capacity(n),
            own_mid: Vec::with_capacity(n),
            mid: Vec::new(),
            luff: Vec::new(),
            leech: Vec::new(),
        };
        // The peak's height is linear in the step: lay it twice and solve.
        s.lay(1.0);
        let y1 = s.world(s.own_leech[n - 1])[1];
        s.lay(2.0);
        let y2 = s.world(s.own_leech[n - 1])[1];
        s.lay(1.0 + (top - y1) / (y2 - y1));
        debug_assert!(
            s.half.iter().all(|&r| r > 0.0),
            "the midline crossed the luff: the sail is too tall for its foot"
        );
        s.mid = s.own_mid.iter().map(|&p| s.world(p)).collect();
        s.luff = s.own_luff.iter().map(|&p| s.world(p)).collect();
        s.leech = s.own_leech.iter().map(|&p| s.world(p)).collect();
        s
    }

    /// Lay the midline leapfrog at step `ds` (see the module docs), and read
    /// each chord off it: perpendicular to the drawn tangent - at the ends
    /// the one-sided difference, which is what bevy's mirrored ends draw -
    /// with its forward end on the luff.
    fn lay(&mut self, ds: f32) {
        let n = self.n;
        let normal: Vec<[f32; 2]> = self.theta.iter().map(|a| [a.sin(), a.cos()]).collect();
        let mut m = Vec::with_capacity(n);
        m.push([-self.foot * 0.5, 0.0]);
        m.push([m[0][0] + ds * normal[0][0], m[0][1] + ds * normal[0][1]]);
        for i in 1..n - 1 {
            m.push([
                m[i - 1][0] + 2.0 * ds * normal[i][0],
                m[i - 1][1] + 2.0 * ds * normal[i][1],
            ]);
        }
        self.half.clear();
        self.own_luff.clear();
        self.own_leech.clear();
        for i in 0..n {
            let d = if i == 0 {
                [m[1][0] - m[0][0], m[1][1] - m[0][1]]
            } else if i == n - 1 {
                [m[n - 1][0] - m[n - 2][0], m[n - 1][1] - m[n - 2][1]]
            } else {
                [
                    (m[i + 1][0] - m[i - 1][0]) * 0.5,
                    (m[i + 1][1] - m[i - 1][1]) * 0.5,
                ]
            };
            let k = d[0].hypot(d[1]);
            // The chord, luff to leech: aft and up.
            let (cz, cy) = (-d[1] / k, d[0] / k);
            // Its forward end on the luff, the line z = 0.
            let r = m[i][0] / cz;
            self.half.push(r);
            self.own_luff.push([m[i][0] - r * cz, m[i][1] - r * cy]);
            self.own_leech.push([m[i][0] + r * cz, m[i][1] + r * cy]);
        }
        self.own_mid = m;
    }

    /// A point `[z, y]` of the sail's own frame in the root frame: turned by
    /// the rake about the tack (the luff leans forward), on the sail's plane.
    fn world(&self, [z, y]: [f32; 2]) -> [f32; 3] {
        let (s, c) = self.rake.sin_cos();
        [
            self.x,
            self.tack.1 - z * s + y * c,
            self.tack.0 + z * c + y * s,
        ]
    }

    /// The point `f` along chord `i`, luff (0) to leech (1), in the root
    /// frame - on the cloth's mid-plane. The ONE point-on-the-cloth function:
    /// every batten, the boom and the yard are placed through it.
    fn point(&self, i: usize, f: f32) -> [f32; 3] {
        let (a, b) = (self.luff[i], self.leech[i]);
        std::array::from_fn(|k| a[k] + (b[k] - a[k]) * f)
    }

    /// The mast's axis at height `y` of the sail's own frame: `fwd` of the
    /// foot abaft the luff and parallel to it, on its own station
    /// athwartships - the centreline but for the mizzen's. The sail hangs a
    /// mast radius and less than half a batten radius to one side, so every
    /// batten, the yard and the boom overlap the mast by construction.
    fn mast_at(&self, y: f32) -> [f32; 3] {
        let p = self.world([-self.fwd * self.foot, y]);
        [self.mast_x, p[1], p[2]]
    }

    /// The highest point of the rig on this sail: the yard's peak end.
    pub(super) fn top(&self) -> f32 {
        self.leech[self.n - 1][1].max(self.luff[self.n - 1][1])
    }
}

/// The sails for this hull - the main and the fore, and the mizzen when
/// asked - each resolved against the air-draft cap by the TOP of the rig,
/// the yard's peak, never a masthead (Rig::new's rule, #1366).
///
/// Each sail is a fixed point of SIX rounds over its tack's height, its foot
/// and its height, from a tack on the waterline and the full foot: the
/// tack stands where every point of the boom clears the sheer under it, the
/// room left under the top sets the height, and a sail short of room
/// flattens to its least aspect before its foot shrinks.
pub(super) fn design(hull: &HullProfile, mizzen: bool) -> Vec<Sail> {
    let l = hull.loa;
    let top = (l * TOP_PER_LOA).min(AIR_DRAFT_CAP - hover(hull.draft) - AIR_DRAFT_MARGIN);
    let specs: &[&Spec] = if mizzen {
        &[&MAIN, &FORE, &MIZZEN]
    } else {
        &[&MAIN, &FORE]
    };
    specs
        .iter()
        .map(|spec| {
            let rake = spec.rake.to_radians();
            let (s, c) = rake.sin_cos();
            let zm = spec.zf * l;
            let full = spec.foot * l;
            let (mut foot, mut height) = (full, 0.0);
            let (mut tack_z, mut tack_y) = (0.0, 0.0);
            for _ in 0..6 {
                // The mast crosses the boom this far along; the tack is `fwd`
                // of the foot ahead of it.
                let mast_boom_z = zm + (tack_y - deck_level(hull, zm)) * rake.tan();
                tack_z = mast_boom_z + spec.fwd * foot * c;
                tack_y = (0..9)
                    .map(|k| {
                        let zl = -foot * k as f32 / 8.0;
                        hull.sheer_z(tack_z + zl * c) + l * spec.clear + zl * s
                    })
                    .fold(f32::NEG_INFINITY, f32::max);
                let room = top * spec.share - tack_y;
                (foot, height) = if room >= spec.aspect * full {
                    (full, spec.aspect * full)
                } else if room >= spec.min_aspect * full {
                    (full, room)
                } else {
                    (room / spec.min_aspect, room)
                };
            }
            let mast_x = spec.mast_x * hull.half_beam_at(zm);
            let x = mast_x + spec.side * (l * MAST_R * spec.r + l * BATTEN_R * 0.4);
            Sail::new(spec, (tack_z, tack_y), x, foot, tack_y + height, mast_x)
        })
        .collect()
}

/// The highest point of the rig every junk stands - the main and the fore,
/// never the Ornate mizzen, so a mount read off it no tier moves.
pub(super) fn standing_top(hull: &HullProfile) -> f32 {
    design(hull, false)
        .iter()
        .map(Sail::top)
        .fold(f32::NEG_INFINITY, f32::max)
}

/// A mast: its heel bedded in the crowned deck under it - found along the
/// mast's own axis in four rounds, since a raked mast's station moves with
/// its height - and its head a hand over the luff's top, where the halyard's
/// block would be.
fn mast_line(hull: &HullProfile, s: &Sail) -> ([f32; 3], [f32; 3]) {
    let l = hull.loa;
    let mut y = 0.0;
    for _ in 0..4 {
        let p = s.mast_at(y);
        y += (deck_y(hull, p[2], p[0]) - l * 0.006) - p[1];
    }
    (s.mast_at(y), s.mast_at(s.own_luff[s.n - 1][1] + l * 0.045))
}

/// The rig, sail by sail: the mast, the cloth, the boom and the yard, then
/// the battens.
pub(super) fn rig(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    c: &JunkColours,
    sails: &[Sail],
    wear: WearTier,
) {
    let l = hull.loa;
    for (k, s) in sails.iter().enumerate() {
        let (heel, head) = mast_line(hull, s);
        kids.push(line(
            &[(heel, l * MAST_R * s.mast_r), (head, l * 0.008 * s.mast_r)],
            10,
            &c.spar,
        ));
        cloth(kids, c, s, k == 0, wear);
        // The boom and the yard: along the end chords, a hand past each end.
        for i in [0, s.n - 1] {
            kids.push(line(
                &[
                    (s.point(i, -0.04), l * SPAR_R),
                    (s.point(i, 1.02), l * SPAR_R * 0.85),
                ],
                8,
                &c.spar,
            ));
        }
        // The battens: every interior chord, luff to leech.
        for i in 1..s.n - 1 {
            kids.push(line(
                &[
                    (s.point(i, -0.012), l * BATTEN_R),
                    (s.point(i, 1.012), l * BATTEN_R),
                ],
                6,
                &c.batten,
            ));
        }
    }
}

/// The cloth: ONE flattened sweep, the midline at every station and its
/// radius the half-chord. The MAINSAIL alone carries the wear: a battered
/// one has lost a panel, and is drawn as two bands of its own sweep with the
/// gap open to the sky; a worn or battered one has a replaced panel in new
/// cloth, the same sweep kept between two battens a hair thicker, so it lies
/// proud of both faces.
fn cloth(kids: &mut Vec<Generator>, c: &JunkColours, s: &Sail, main: bool, wear: WearTier) {
    let at = [s.x, 0.0, 0.0];
    let pts: Vec<([f32; 3], f32)> = s.mid.iter().copied().zip(s.half.iter().copied()).collect();
    let whole = || flat_sweep(&pts, 16, 0, SAIL_SCALE, &c.sail, at);
    if main && wear == WearTier::Battered {
        kids.push(band(whole(), 0, TORN));
        kids.push(band(whole(), TORN + 1, s.n - 1));
    } else {
        kids.push(whole());
    }
    if main && wear != WearTier::Pristine {
        let new_cloth: Vec<_> = pts.iter().map(|&(p, r)| (p, r * 0.992)).collect();
        kids.push(band(
            flat_sweep(&new_cloth, 16, 0, SAIL_SCALE * 1.6, &c.patch, at),
            PATCH,
            PATCH + 1,
        ));
    }
}
