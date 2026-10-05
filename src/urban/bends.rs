//! How a ribbon gets round a bend without folding (#1567). The tracer can
//! leave a hairpin as one sharp vertex between two straight legs, and a
//! ribbon mitred there folds: on the inside of a bend turning `θ` the legs'
//! lines `u` metres out cross `u·tan(θ/2)` short of the vertex, further back
//! than the frames either side of it, so the inner deck edge, curb and skirt
//! ran backwards over the other leg's deck and the deck turned over at the
//! tip - the owner's two corners, a major and a minor street each turning
//! about 117 degrees at one vertex.
//!
//! Two steps keep a ribbon from folding. Before a chain is sampled, every
//! bend that turns more than one arc step is drawn as an arc tangent to both
//! of its legs ([`round_bends`]), of the street's outer half-width where the
//! legs leave room for it. At that radius the outer curb line on the inside
//! of the bend shrinks to the point where the straight legs' outer curb lines
//! meet, so the rounded street keeps to the footprint of the straight one -
//! which the lots and the street furniture are kept clear of - and only its
//! outside is cut. A leg two bends share is split between them in proportion
//! to what each wants. Where the legs are too short for that radius, or a
//! bend is too gentle to round, a line of the ribbon can still run
//! backwards, and the ribbon collapses each such stretch onto the point where
//! the line crosses itself ([`unfold_rail`]): the sharp inner corner a mitre
//! is meant to draw.

use std::f32::consts::PI;

use crate::urban::math::{cross2, dot2, sub2};

/// Angular step (radians) of the arc a bend is drawn as - the step a hub's
/// far side wraps round its node by. A bend turning less than one step
/// keeps its mitre: as an arc it would be its own chord.
pub(crate) const BEND_STEP_RAD: f32 = PI / 12.0;
/// The straight (m) a chain keeps at each end before a bend's arc, so its
/// end frame faces along its end leg - the heading its hub mouth or end cap
/// is planned on.
const END_STRAIGHT_M: f32 = 0.5;
/// A leg shorter than this (m) has no direction to turn from, so the bends
/// at its ends keep their mitres; two points of a rounded chain closer than
/// this are one.
const MIN_LEG_M: f32 = 1.0e-3;
/// A bend turning back on itself further than this (radians) keeps its
/// mitre: the arc it wants is longer than any leg, and its tangent length
/// runs to infinity.
const MAX_ROUNDED_TURN_RAD: f32 = PI * (178.0 / 180.0);
/// A rail step backwards shorter than this (m, along the chain) is rounding
/// noise, not a fold.
const FOLD_EPS_M: f32 = 1.0e-4;
/// Rail segments searched, both sides together, for where a folded rail
/// crosses itself: a fold reaches `u·tan(θ/2)` back along each leg - 11 m
/// for a 140-degree mitre's outer curb line, four frames 3 m apart or a
/// dozen of an arc's.
const UNFOLD_SEARCH: usize = 48;
/// Slack (as a fraction of a segment) in whether two rail segments cross,
/// so a crossing on a shared end point is still found.
const CROSS_EPS: f32 = 1.0e-5;

/// One interior vertex of a chain that is drawn as an arc.
struct Bend {
    /// Signed turn (radians), counter-clockwise in `(x, z)` positive.
    turn: f32,
    /// `tan(|turn| / 2)`: the tangent length per metre of radius.
    half_tan: f32,
    /// The tangent length the full radius takes from each leg.
    want: f32,
}

/// `pts` with every interior vertex that turns more than [`BEND_STEP_RAD`]
/// drawn as an arc tangent to both of its legs, of radius `radius` (the
/// street's outer half-width) where the legs leave room for it and tighter
/// where they do not. Each leg two bends share is split in proportion to
/// the length each wants, and an end leg keeps [`END_STRAIGHT_M`] of
/// straight. The arc's chords are mitred like any frame, so the radius is
/// raised by the chords' mitre at the arc's step: its outer curb line on the
/// inside then meets in one point. The ends never move, and a chain with no
/// such bend comes back as it went in.
pub(crate) fn round_bends(pts: &[(f32, f32)], radius: f32) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 3 || !radius.is_finite() || radius <= 0.0 {
        return pts.to_vec();
    }
    let legs: Vec<([f32; 2], f32)> = pts.windows(2).map(|w| unit_leg(w[0], w[1])).collect();
    let bends: Vec<Option<Bend>> = (0..n)
        .map(|k| {
            (k > 0 && k + 1 < n)
                .then(|| bend_at(&legs, k, radius))
                .flatten()
        })
        .collect();
    if bends.iter().all(Option::is_none) {
        return pts.to_vec();
    }
    let want = |k: usize| bends[k].as_ref().map_or(0.0, |b| b.want);
    let mut out = vec![pts[0]];
    for k in 1..n - 1 {
        let Some(bend) = &bends[k] else {
            push_point(&mut out, pts[k]);
            continue;
        };
        let back = leg_share(legs[k - 1].1, bend.want, want(k - 1), k == 1);
        let ahead = leg_share(legs[k].1, bend.want, want(k + 1), k + 2 == n);
        push_arc(
            &mut out,
            pts[k],
            [legs[k - 1].0, legs[k].0],
            bend,
            back.min(ahead),
        );
    }
    // The last point is the chain's end: it replaces an arc point it repeats.
    let end = pts[n - 1];
    if out.len() > 1 && out.last().is_some_and(|&last| dist(last, end) < MIN_LEG_M) {
        out.pop();
    }
    out.push(end);
    out
}

/// The unit direction and length of the leg from `a` to `b`.
fn unit_leg(a: (f32, f32), b: (f32, f32)) -> ([f32; 2], f32) {
    let d = [b.0 - a.0, b.1 - a.1];
    let len = d[0].hypot(d[1]);
    let l = len.max(MIN_LEG_M);
    ([d[0] / l, d[1] / l], len)
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// The bend at interior vertex `k` between `legs[k - 1]` and `legs[k]`, if
/// it is drawn as an arc of (at most) `radius`.
fn bend_at(legs: &[([f32; 2], f32)], k: usize, radius: f32) -> Option<Bend> {
    let ((u, lu), (v, lv)) = (legs[k - 1], legs[k]);
    if lu < MIN_LEG_M || lv < MIN_LEG_M {
        return None;
    }
    let turn = cross2(u, v).atan2(dot2(u, v));
    if turn.abs() <= BEND_STEP_RAD || turn.abs() > MAX_ROUNDED_TURN_RAD {
        return None;
    }
    let half_tan = (turn.abs() * 0.5).tan();
    let step = turn.abs() / arc_steps(turn) as f32;
    let full = radius / (step * 0.5).cos();
    Some(Bend {
        turn,
        half_tan,
        want: full * half_tan,
    })
}

/// The chords an arc turning `turn` is drawn with, each turning at most
/// [`BEND_STEP_RAD`].
fn arc_steps(turn: f32) -> usize {
    (turn.abs() / BEND_STEP_RAD).ceil().max(1.0) as usize
}

/// The tangent length a bend wanting `mine` takes from a leg `len` metres
/// long whose other end wants `theirs` (nothing for an end): all it wants if
/// both fit, else its share in proportion. An end leg (`end`) first keeps
/// [`END_STRAIGHT_M`] of straight.
fn leg_share(len: f32, mine: f32, theirs: f32, end: bool) -> f32 {
    let room = if end {
        (len - END_STRAIGHT_M).max(0.0)
    } else {
        len
    };
    if mine <= 0.0 || mine + theirs <= room {
        mine.min(room)
    } else {
        room * mine / (mine + theirs)
    }
}

/// Push the arc the bend at `p` is drawn as - from leg direction `dirs[0]`
/// to `dirs[1]`, tangent to both legs `t` metres from `p` - or `p` itself
/// where there is no room for one.
fn push_arc(out: &mut Vec<(f32, f32)>, p: (f32, f32), dirs: [[f32; 2]; 2], bend: &Bend, t: f32) {
    let [u, v] = dirs;
    if t < MIN_LEG_M {
        push_point(out, p);
        return;
    }
    let r = t / bend.half_tan;
    let side = bend.turn.signum();
    let start = [p.0 - u[0] * t, p.1 - u[1] * t];
    // The centre lies `r` to the side the chain turns to.
    let centre = [start[0] - u[1] * side * r, start[1] + u[0] * side * r];
    let spoke = sub2(start, centre);
    let steps = arc_steps(bend.turn);
    push_point(out, (start[0], start[1]));
    for j in 1..steps {
        let (sin, cos) = (bend.turn * j as f32 / steps as f32).sin_cos();
        push_point(
            out,
            (
                centre[0] + spoke[0] * cos - spoke[1] * sin,
                centre[1] + spoke[0] * sin + spoke[1] * cos,
            ),
        );
    }
    push_point(out, (p.0 + v[0] * t, p.1 + v[1] * t));
}

/// Push `q` unless it repeats the last point.
fn push_point(out: &mut Vec<(f32, f32)>, q: (f32, f32)) {
    if out.last().is_none_or(|&last| dist(last, q) >= MIN_LEG_M) {
        out.push(q);
    }
}

/// Collapse each stretch of one ribbon rail that runs backwards onto the
/// point where the rail crosses itself - the corner the lines either side
/// of the stretch meet in - or, where it does not cross itself, hold the
/// rail still until it moves on past the stretch. `rail` is the line one
/// profile point traces through the chain's frames, `centre` the frames'
/// centres, which say which way is forward. A rail's ends never move: a
/// hub's mouth and an end cap are built on them.
pub(crate) fn unfold_rail(rail: &mut [[f32; 2]], centre: &[[f32; 2]]) {
    let n = rail.len().min(centre.len());
    let backwards = |rail: &[[f32; 2]], s: usize| {
        runs_backwards(rail[s], rail[s + 1], centre[s], centre[s + 1])
    };
    let mut i = 0;
    while i + 1 < n {
        if !backwards(rail, i) {
            i += 1;
            continue;
        }
        let mut last = i;
        while last + 2 < n && backwards(rail, last + 1) {
            last += 1;
        }
        i = match self_crossing(&rail[..n], i, last) {
            Some((a, b, x)) => {
                rail[a + 1..=b].fill(x);
                b
            }
            None => hold_still(&mut rail[..n], centre, i),
        };
    }
}

/// Whether a rail step from `p` to `q` runs backwards against the chain's
/// step from `c0` to `c1`.
fn runs_backwards(p: [f32; 2], q: [f32; 2], c0: [f32; 2], c1: [f32; 2]) -> bool {
    let c = sub2(c1, c0);
    dot2(sub2(q, p), c) < -FOLD_EPS_M * c[0].hypot(c[1])
}

/// Where the rail before the backward stretch of segments `first..=last`
/// crosses the rail after it, nearest the stretch: the segment before that
/// crosses (`a`), the segment after (`b`) and the point.
fn self_crossing(rail: &[[f32; 2]], first: usize, last: usize) -> Option<(usize, usize, [f32; 2])> {
    for span in 0..UNFOLD_SEARCH {
        for back in 0..=span {
            let Some(a) = first.checked_sub(back + 1) else {
                continue;
            };
            let b = last + 1 + span - back;
            if b + 1 >= rail.len() {
                continue;
            }
            if let Some(x) = segment_crossing([rail[a], rail[a + 1]], [rail[b], rail[b + 1]]) {
                return Some((a, b, x));
            }
        }
    }
    None
}

/// Where segment `p` crosses segment `q`, if they cross.
fn segment_crossing(p: [[f32; 2]; 2], q: [[f32; 2]; 2]) -> Option<[f32; 2]> {
    let (r, s) = (sub2(p[1], p[0]), sub2(q[1], q[0]));
    let den = cross2(r, s);
    if den.abs() < 1.0e-12 {
        return None;
    }
    let w = sub2(q[0], p[0]);
    let (t, u) = (cross2(w, s) / den, cross2(w, r) / den);
    let within = |x: f32| (-CROSS_EPS..=1.0 + CROSS_EPS).contains(&x);
    (within(t) && within(u)).then(|| [p[0][0] + r[0] * t, p[0][1] + r[1] * t])
}

/// [`unfold_rail`]'s fallback for a backward stretch from segment `first`
/// the rail never crosses back over: hold the rail at the stretch's start
/// until its next point lies ahead - or, where that would move the rail's
/// last point, hold it back on the last point instead. Returns the segment
/// to scan on from.
fn hold_still(rail: &mut [[f32; 2]], centre: &[[f32; 2]], first: usize) -> usize {
    let n = rail.len();
    let p = rail[first];
    for s in first + 1..n - 1 {
        rail[s] = p;
        if !runs_backwards(p, rail[s + 1], centre[s], centre[s + 1]) {
            return s;
        }
    }
    // The stretch runs on to the rail's last point, which never moves.
    let q = rail[n - 1];
    for s in (1..n - 1).rev() {
        rail[s] = q;
        if !runs_backwards(rail[s - 1], q, centre[s - 1], centre[s]) {
            break;
        }
    }
    n - 1
}

#[cfg(test)]
mod tests;
