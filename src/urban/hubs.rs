//! Intersection hubs: the deck that fills a junction between its incident roads.
//! Built from the truncated ribbon ends (#576), the deck is the polygon the
//! roads themselves outline: each arm's mouth, then between two neighbouring
//! arms the corner their curb lines make (#1558). Where two streets meet in a
//! corner of a block, the near curb line of each runs on to the point where
//! it meets the other's - one clean corner, at both the deck edge and the
//! outer curb - so a crossing of a wide and a narrow street keeps both curb
//! lines straight through it. The far side of a fork or a Y wraps round the
//! junction node. Each arm's edge follows the stub of street its pull-back
//! cut off, so a curving street's hub follows its curve, and a cluster of
//! junctions drawn as one hub walks round the short streets joining them.
//! The polygon is fanned from its node (a one-node hub's outline is
//! star-shaped from it) or else ear-clipped, so every deck triangle lies
//! inside it and faces up whatever the arms' angles, and it is held FLAT at
//! the max incident mouth height, which the network levelling has already
//! pinned every road up to (#584). The curb and skirt run on along the same
//! edges and corners, so they stay continuous with every ribbon's.
//!
//! The old hub (#576/#577/#894) fanned the mouth corners sorted by angle round
//! their centroid and closed each gap with an arc bulging out of the hub. At
//! a junction cluster (two junctions a few metres apart) the two decks'
//! corners interleaved and those arcs - curb, chamfer and a 5 m skirt - were
//! swept across the asphalt; where two pull-backs differed, a diagonal chord
//! stepped every corner sideways. Both were what the owner saw (#1558).

use crate::urban::math::{add2, cross, cross2, dot, dot2, normalize, scale2, sub2, sub3};
use crate::urban::truncation::Hub;
use crate::urban::{Dims, RoadParts, UV_TILE_M, quad_normal};

/// One ribbon end abutting a hub, recorded during chain extrusion so the hub
/// can meet each incident road at its exact mouth corners and deck height
/// (seamless, upward-only). All positions are in the sub-heightmap frame.
pub(crate) struct RoadEnd {
    /// The hub it opens into (an index into the junction plan's hubs).
    pub(crate) hub: usize,
    /// The junction node (graph id) the road leaves the hub from.
    pub(crate) node: usize,
    /// Truncated mouth centre (XZ): where the ribbon actually ends after #575.
    pub(crate) cx: f32,
    pub(crate) cz: f32,
    /// Unit heading of the road at its mouth, pointing away from the hub. Its
    /// perpendicular is the ribbon's end frame's right axis (up to sign), so
    /// the two mouth corners `(cx, cz) ± (−dz, dx)·half_w` coincide with the
    /// ribbon's end edge.
    pub(crate) dx: f32,
    pub(crate) dz: f32,
    /// Deck half-width of the road.
    pub(crate) half_w: f32,
    pub(crate) deck_y: f32,
    /// The ribbon's skirt-bottom height at this mouth (`Frame::skirt_bottom_y`),
    /// so the hub curb's skirt foot can drop to the *same* depth and weld to the
    /// ribbon skirt exactly - at any skirt depth or cross-slope - leaving no
    /// open band at the seam.
    pub(crate) skirt_y: f32,
    /// The normal the ribbon's deck row at this mouth is shaded with: halfway
    /// between its own end segment and the hub's flat deck (#1567). The hub's
    /// two mouth corners take it too, so on a graded approach the seam does
    /// not shade as a crease.
    pub(crate) deck_normal: [f32; 3],
    /// The stub of chain the pull-back cut off, from the junction node to the
    /// mouth centre (XZ): the hub draws it, its deck edges following it.
    pub(crate) spine: Vec<(f32, f32)>,
}

/// Within this |sine| of a straight line two neighbouring spokes are a
/// street running through its node: their edges meet in a corner on the
/// inside of the bend, and run on past the node on the outside, where a
/// wider turn would wrap round it.
const STRAIGHT_SIN: f32 = 0.17;
/// Angular step (radians) of the arc a hub's far side wraps round its node.
const WRAP_STEP_RAD: f32 = std::f32::consts::PI / 12.0;
/// Below this (m², twice an ear's area) an ear is no triangle at all.
const OUTLINE_EPS: f32 = 1.0e-6;
/// Below this sine an outline point turns nothing (a collinear run, or a
/// zero-width spike out and back) and is dropped before the deck is
/// triangulated.
const COLLINEAR_SIN: f32 = 1.0e-4;
/// An outline point closer than this (m) to the line through its neighbours
/// turns nothing the eye can see; the ear-clipper drops it rather than cut a
/// sliver.
const SNAP_M: f32 = 1.0e-2;

/// A point on a hub's deck edge, with the outward normal its curb is offset
/// along there.
#[derive(Clone, Copy)]
struct EdgePoint {
    p: [f32; 2],
    /// Outward normal (away from the deck), scaled so an offset along it by
    /// `x` lands `x` out from every curb line meeting at the point.
    n: [f32; 2],
}

/// One arm of a hub in world XZ.
struct HubArm<'a> {
    end: &'a RoadEnd,
    /// Mouth centre.
    m: [f32; 2],
    /// Unit left (counter-clockwise) perpendicular of the road's heading
    /// away from the hub.
    l: [f32; 2],
    /// The stub from the node to the mouth centre.
    spine: Vec<[f32; 2]>,
}

impl HubArm<'_> {
    /// The deck corner on the arm's clockwise (`-1`) or counter-clockwise
    /// (`+1`) side.
    fn corner(&self, side: f32) -> [f32; 2] {
        let w = self.end.half_w;
        [
            self.m[0] + self.l[0] * w * side,
            self.m[1] + self.l[1] * w * side,
        ]
    }
}

/// What leaves a hub node: an arm (its index), or a link - a swallowed chain
/// the hub draws - to another of the hub's nodes.
#[derive(Clone, Copy)]
enum Spoke {
    Arm(usize),
    Link {
        id: usize,
        to: usize,
        /// The link's own end node here (world XZ) - not the drawn node's
        /// centre, which may stand for several.
        start: [f32; 2],
        dir: [f32; 2],
        half_w: f32,
        len: f32,
    },
}

/// One node of a hub: where it stands and its spokes, counter-clockwise by
/// angle.
struct HubNode {
    p: [f32; 2],
    spokes: Vec<(f32, Spoke)>,
}

/// Union-find root with path-halving.
fn root(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Where the line through `a` along `da` meets the line through `b` along
/// `db`, as the two parameters `(s, u)` with `a + da·s = b + db·u`; `None`
/// when they are parallel.
fn line_meet(a: [f32; 2], da: [f32; 2], b: [f32; 2], db: [f32; 2]) -> Option<(f32, f32)> {
    let det = cross2(da, db);
    if det.abs() < 1.0e-6 {
        return None;
    }
    let ab = sub2(b, a);
    Some((cross2(ab, db) / det, cross2(ab, da) / det))
}

/// One side of a spoke's deck strip as a polyline from its node out to its
/// far end - an arm's stub offset to the side, ending exactly on its
/// mouth corner, or a link's edge out to its far node abeam - each point
/// with its outward normal (mitred through a bend). `side` is `+1` for the
/// counter-clockwise side, `-1` for the clockwise.
fn edge_poly(spoke: Spoke, arms: &[HubArm], side: f32) -> Vec<EdgePoint> {
    match spoke {
        Spoke::Arm(k) => {
            let a = &arms[k];
            let w = a.end.half_w;
            let mut spine: Vec<[f32; 2]> = Vec::with_capacity(a.spine.len());
            for &p in &a.spine {
                if spine
                    .last()
                    .is_none_or(|q: &[f32; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) > 1.0e-4)
                {
                    spine.push(p);
                }
            }
            if spine.len() < 2 {
                return vec![EdgePoint {
                    p: a.corner(side),
                    n: scale2(a.l, side),
                }];
            }
            let last = spine.len() - 1;
            let left = |i: usize| {
                let e = sub2(spine[i + 1], spine[i]);
                let l = dot2(e, e).sqrt().max(1.0e-6);
                [-e[1] / l, e[0] / l]
            };
            (0..=last)
                .map(|i| {
                    let n = if i == last {
                        a.l // the mouth frame's own axis: the corner is exact
                    } else if i == 0 {
                        left(0)
                    } else {
                        // The mitre: an offset along it lands on both
                        // segments' offset lines; clamped at three widths,
                        // as the ribbon clamps its bends.
                        let (a1, a2) = (left(i - 1), left(i));
                        let bis = add2(a1, a2);
                        let k = dot2(bis, a1).max(1.0e-3);
                        let mitre = scale2(bis, 1.0 / k);
                        let len = dot2(mitre, mitre).sqrt();
                        if len > 3.0 {
                            scale2(mitre, 3.0 / len)
                        } else {
                            mitre
                        }
                    };
                    let n = scale2(n, side);
                    let p = if i == last {
                        a.corner(side)
                    } else {
                        add2(spine[i], scale2(n, w))
                    };
                    EdgePoint { p, n }
                })
                .collect()
        }
        Spoke::Link {
            start,
            dir,
            half_w,
            len,
            ..
        } => {
            let n = scale2([-dir[1], dir[0]], side);
            let near = add2(start, scale2(n, half_w));
            vec![
                EdgePoint { p: near, n },
                EdgePoint {
                    p: add2(near, scale2(dir, len)),
                    n,
                },
            ]
        }
    }
}

/// Where segment `a0→a1` meets segment `b0→b1`, as their parameters, the
/// first segment of each allowed to run on back past its start (`extend`).
fn seg_meet(
    a0: [f32; 2],
    a1: [f32; 2],
    b0: [f32; 2],
    b1: [f32; 2],
    extend: [bool; 2],
) -> Option<(f32, f32)> {
    let (da, db) = (sub2(a1, a0), sub2(b1, b0));
    let (s, u) = line_meet(a0, da, b0, db)?;
    let ok = |t: f32, ext: bool| (ext || t >= -1.0e-5) && t <= 1.0 + 1.0e-5;
    (ok(s, extend[0]) && ok(u, extend[1])).then_some((s, u))
}

/// Where the counter-clockwise edge `pi` of one spoke and the clockwise
/// edge `pj` of the next meet (both from their node outward): the path back
/// along `pi` from its far end to the crossing nearest it, the corner, and
/// out along `pj` to its far end, both far ends left out. `None` if they do
/// not cross, or if the outer curb lines' corner would lie past either far
/// end's outer point (a capped fork, whose ribbons still overlap there).
fn reflex_corner(pi: &[EdgePoint], pj: &[EdgePoint], outer: f32) -> Option<Vec<EdgePoint>> {
    let (mi, mj) = (pi.len() - 1, pj.len() - 1);
    if mi == 0 || mj == 0 {
        return None;
    }
    // The crossing nearest `pi`'s far end.
    let mut hit: Option<(usize, f32, usize)> = None;
    for a in 0..mi {
        for b in 0..mj {
            let Some((s, _)) =
                seg_meet(pi[a].p, pi[a + 1].p, pj[b].p, pj[b + 1].p, [a == 0, b == 0])
            else {
                continue;
            };
            if hit.is_none_or(|(ha, hs, _)| (a as f32 + s) > (ha as f32 + hs)) {
                hit = Some((a, s, b));
            }
        }
    }
    let (a, s, b) = hit?;
    let q = add2(pi[a].p, scale2(sub2(pi[a + 1].p, pi[a].p), s));
    let seg_left = |p: &[EdgePoint], k: usize| {
        let e = sub2(p[k + 1].p, p[k].p);
        let l = dot2(e, e).sqrt().max(1.0e-6);
        [-e[1] / l, e[0] / l]
    };
    let (n1, n2) = (seg_left(pi, a), scale2(seg_left(pj, b), -1.0));
    let bis = add2(n1, n2);
    let k = dot2(bis, n1);
    if k < 1.0e-3 {
        return None;
    }
    let n = scale2(bis, 1.0 / k);
    // The outer curb corner must lie behind both far ends' outer points.
    let o = add2(q, scale2(n, outer));
    let behind = |p: &[EdgePoint], m: usize| {
        let far = add2(p[m].p, scale2(p[m].n, outer));
        let e = sub2(p[m].p, p[m - 1].p);
        let l = dot2(e, e).sqrt().max(1.0e-6);
        dot2(sub2(o, far), scale2(e, 1.0 / l)) <= 1.0e-3
    };
    if !behind(pi, mi) || !behind(pj, mj) {
        return None;
    }
    let mut path: Vec<EdgePoint> = pi[a + 1..mi].iter().rev().copied().collect();
    path.push(EdgePoint { p: q, n });
    path.extend(pj[b + 1..mj].iter().copied());
    Some(path)
}

/// The deck-edge points where the strip of spoke `si` (its counter-clockwise
/// side) turns into the strip of spoke `sj` (its clockwise side) at node `v`,
/// `gap` radians counter-clockwise from `si` to `sj`: back along `si`'s edge
/// from its far end, then out along `sj`'s to its far end, the far ends
/// themselves left out. In a block's corner (`gap` under half a turn), and
/// on the inside of a gentle bend, the two edges meet in a corner - kept
/// only if the outer curb lines' corner lies behind both far ends too, or
/// else a block's corner is a straight chord; round the far side (over half
/// a turn) the edge wraps round the node; straight on, it runs through.
fn corner_points(
    v: [f32; 2],
    si: Spoke,
    sj: Spoke,
    gap: f32,
    arms: &[HubArm],
    outer: f32,
) -> Vec<EdgePoint> {
    let pi = edge_poly(si, arms, 1.0);
    let pj = edge_poly(sj, arms, -1.0);
    let (mi, mj) = (pi.len() - 1, pj.len() - 1);
    let straight = STRAIGHT_SIN.asin();
    let half = std::f32::consts::PI;
    // Short of half a turn - a block's corner, or the inside of a gentle
    // bend in a street running through - the two edges meet in a corner.
    if gap < half + straight {
        if let Some(path) = reflex_corner(&pi, &pj, outer) {
            return path;
        }
        if gap < half - straight {
            return Vec::new(); // the corner would lie past a far end: a chord
        }
    }
    let mut path: Vec<EdgePoint> = pi[..mi].iter().rev().copied().collect();
    if gap > half + straight {
        // Round the node between the two edges' starts abeam it (radius
        // blended between them).
        let (r1v, r2v) = (sub2(pi[0].p, v), sub2(pj[0].p, v));
        let (r1, r2) = (r1v[0].hypot(r1v[1]), r2v[0].hypot(r2v[1]));
        if r1 > 1.0e-3 && r2 > 1.0e-3 {
            let a1 = r1v[1].atan2(r1v[0]);
            let mut sweep = r2v[1].atan2(r2v[0]) - a1;
            while sweep <= 0.0 {
                sweep += std::f32::consts::TAU;
            }
            while sweep > std::f32::consts::TAU {
                sweep -= std::f32::consts::TAU;
            }
            let steps = (sweep / WRAP_STEP_RAD).ceil().max(1.0) as usize;
            for k in 1..steps {
                let f = k as f32 / steps as f32;
                let (ang, r) = (a1 + sweep * f, r1 + (r2 - r1) * f);
                let n = [ang.cos(), ang.sin()];
                path.push(EdgePoint {
                    p: add2(v, scale2(n, r)),
                    n,
                });
            }
        }
    }
    path.extend(pj[..mj].iter().copied());
    path
}

/// The hub's outline as a walk round its nodes and links (#1558): from each
/// arm's mouth, round the node it leaves from to the next spoke
/// counter-clockwise - out across the next arm's mouth, or along a link to
/// the node at its far end and on round that - until the walk is back where
/// it started. Returns the arms in the order the walk meets them and, after
/// each, the deck-edge path from its counter-clockwise mouth corner to the
/// next arm's clockwise corner. `None` if the walk misses an arm (an arm
/// inside a ring of links) or does not close.
fn walk_outline(
    nodes: &[HubNode],
    arms: &[HubArm],
    arm_node: &[usize],
    outer: f32,
) -> Option<(Vec<usize>, Vec<Vec<EdgePoint>>)> {
    let spoke_of = |v: usize, k: usize| {
        nodes[v]
            .spokes
            .iter()
            .position(|(_, s)| matches!(s, Spoke::Arm(j) if *j == k))
    };
    let (v0, i0) = (arm_node[0], spoke_of(arm_node[0], 0)?);
    let (mut v, mut i) = (v0, i0);
    let mut order = vec![0];
    let mut gaps = Vec::new();
    let mut gap = vec![EdgePoint {
        p: arms[0].corner(1.0),
        n: arms[0].l,
    }];
    let limit = 4 * nodes.iter().map(|n| n.spokes.len()).sum::<usize>() + 4;
    for _ in 0..limit {
        let spokes = &nodes[v].spokes;
        let j = (i + 1) % spokes.len();
        let mut turn = spokes[j].0 - spokes[i].0;
        while turn <= 0.0 {
            turn += std::f32::consts::TAU;
        }
        gap.extend(corner_points(
            nodes[v].p,
            spokes[i].1,
            spokes[j].1,
            turn,
            arms,
            outer,
        ));
        match spokes[j].1 {
            Spoke::Arm(k) => {
                gap.push(EdgePoint {
                    p: arms[k].corner(-1.0),
                    n: scale2(arms[k].l, -1.0),
                });
                gaps.push(std::mem::take(&mut gap));
                if (v, j) == (v0, i0) {
                    let mut seen = order.clone();
                    seen.sort_unstable();
                    seen.dedup();
                    return (seen.len() == arms.len() && order.len() == arms.len())
                        .then_some((order, gaps));
                }
                order.push(k);
                gap.push(EdgePoint {
                    p: arms[k].corner(1.0),
                    n: arms[k].l,
                });
                i = j;
            }
            Spoke::Link { id, to, .. } => {
                let back = nodes[to]
                    .spokes
                    .iter()
                    .position(|(_, s)| matches!(s, Spoke::Link { id: b, .. } if *b == id))?;
                v = to;
                i = back;
            }
        }
    }
    None
}

/// Whether the closed polygon `poly` is simple: no two non-adjacent edges
/// cross.
fn is_simple(poly: &[[f32; 2]]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let seg_hit = |p1: [f32; 2], p2: [f32; 2], q1: [f32; 2], q2: [f32; 2]| {
        let d1 = cross2(sub2(p2, p1), sub2(q1, p1));
        let d2 = cross2(sub2(p2, p1), sub2(q2, p1));
        let d3 = cross2(sub2(q2, q1), sub2(p1, q1));
        let d4 = cross2(sub2(q2, q1), sub2(p2, q1));
        (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
    };
    for i in 0..n {
        let (a1, a2) = (poly[i], poly[(i + 1) % n]);
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue; // adjacent through the wrap
            }
            if seg_hit(a1, a2, poly[j], poly[(j + 1) % n]) {
                return false;
            }
        }
    }
    true
}

/// Twice the signed area of `poly` (positive counter-clockwise in `(x, z)`).
fn signed_area2(poly: &[[f32; 2]]) -> f32 {
    let n = poly.len();
    (0..n)
        .map(|i| cross2(poly[i], poly[(i + 1) % n]))
        .sum::<f32>()
}

/// Triangulate a simple polygon by ear clipping, counter-clockwise in
/// `(x, z)`: indices into `poly`. `None` if no ear can be found (only for a
/// polygon that is not simple). Each round clips the fattest ear - the one
/// whose area is largest against its longest side squared - so a long thin
/// outline is not cut into slivers; ties go to the lowest index, so it is
/// deterministic.
pub(crate) fn ear_clip(poly: &[[f32; 2]]) -> Option<Vec<[usize; 3]>> {
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    if signed_area2(poly) < 0.0 {
        idx.reverse();
    }
    let inside = |p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        cross2(sub2(b, a), sub2(p, a)) >= 0.0
            && cross2(sub2(c, b), sub2(p, b)) >= 0.0
            && cross2(sub2(a, c), sub2(p, c)) >= 0.0
    };
    // Twice the area of the corner at `b`, and the least that counts as a
    // turn: a sine of `COLLINEAR_SIN` against the two sides' lengths.
    let turn = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        let (ab, bc) = (sub2(b, a), sub2(c, b));
        (
            cross2(ab, bc),
            COLLINEAR_SIN * (dot2(ab, ab) * dot2(bc, bc)).sqrt() + OUTLINE_EPS,
        )
    };
    let mut tris = Vec::with_capacity(poly.len().saturating_sub(2));
    while idx.len() > 3 {
        let m = idx.len();
        // A vertex that turns nothing - collinear, a convex corner within a
        // centimetre of the line through its neighbours, or a zero-width
        // spike - goes without a triangle: it changes the deck by a sliver
        // (never outward, over an ear already cut), and would leave one.
        if let Some(i) = (0..m).find(|&i| {
            let (a, b, c) = (
                poly[idx[(i + m - 1) % m]],
                poly[idx[i]],
                poly[idx[(i + 1) % m]],
            );
            let ac = sub2(c, a);
            let lac = dot2(ac, ac).sqrt();
            let (area2, least) = turn(a, b, c);
            let near = lac > 1.0e-6 && (cross2(ac, sub2(b, a)) / lac).abs() < SNAP_M;
            let spike = dot2(sub2(b, a), sub2(c, b)) < 0.0;
            area2.abs() <= least || (near && (area2 > 0.0 || spike))
        }) {
            idx.remove(i);
            continue;
        }
        let mut best: Option<(f32, usize)> = None;
        for i in 0..m {
            let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (pa, pb, pc) = (poly[a], poly[b], poly[c]);
            let (area2, least) = turn(pa, pb, pc);
            if area2 <= least
                || idx
                    .iter()
                    .any(|&j| j != a && j != b && j != c && inside(poly[j], pa, pb, pc))
            {
                continue;
            }
            let long2 = [sub2(pb, pa), sub2(pc, pb), sub2(pa, pc)]
                .iter()
                .map(|e| dot2(*e, *e))
                .fold(0.0_f32, f32::max);
            let fat = area2 / long2.max(1.0e-12);
            if best.is_none_or(|(f, _)| fat > f) {
                best = Some((fat, i));
            }
        }
        let (_, ear) = best?;
        let (a, b, c) = (idx[(ear + m - 1) % m], idx[ear], idx[(ear + 1) % m]);
        tris.push([a, b, c]);
        idx.remove(ear);
    }
    if idx.len() == 3 {
        let (area2, least) = turn(poly[idx[0]], poly[idx[1]], poly[idx[2]]);
        if area2.abs() > least {
            tris.push([idx[0], idx[1], idx[2]]);
        }
    }
    Some(tris)
}

/// Drop consecutive duplicate points and the points that turn nothing - a
/// collinear run, or a zero-width spike out and back - from a closed
/// outline: an ear-clipper would cut a zero-area ear there.
fn clean_outline(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out
            .last()
            .is_none_or(|q: &[f32; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) > 1.0e-4)
        {
            out.push(p);
        }
    }
    while out.len() > 1 {
        let (f, l) = (out[0], out[out.len() - 1]);
        if (f[0] - l[0]).hypot(f[1] - l[1]) > 1.0e-4 {
            break;
        }
        out.pop();
    }
    loop {
        let n = out.len();
        if n < 4 {
            break;
        }
        let Some(k) = (0..n).find(|&k| {
            let (a, b, c) = (out[(k + n - 1) % n], out[k], out[(k + 1) % n]);
            let (ab, bc) = (sub2(b, a), sub2(c, b));
            let scale = (dot2(ab, ab) * dot2(bc, bc)).sqrt();
            cross2(ab, bc).abs() <= COLLINEAR_SIN * scale
        }) else {
            break;
        };
        out.remove(k);
    }
    out
}

/// Build a real hub at every junction plan hub (two or more meshed arms)
/// from the truncated ribbon ends (#576, #1558): a deck polygon whose mouth
/// edges coincide with each road's end cross-section (the deck flows in
/// seamlessly at the road's own height), joined between neighbouring arms by
/// the corner of their curb lines - or round the node on the far side of a
/// fork - walking round a cluster's links, and ear-clipped, FLAT at the
/// **max** incident mouth height, which the #584 network levelling pins every
/// incident road up to. The curb profile (curb wall, top, chamfer) and a
/// skirt dropping to the incident ribbons' fixed depth run along every gap
/// between arms, continuous with each ribbon's own curb. Every deck triangle
/// is wound front-up.
pub(crate) fn extrude_hubs(
    road_ends: &[RoadEnd],
    hubs: &[Hub],
    world_offset: [f32; 2],
    dims: &Dims,
    parts: &mut RoadParts,
) {
    use std::collections::BTreeMap;
    let mut by_hub: BTreeMap<usize, Vec<&RoadEnd>> = BTreeMap::new();
    for e in road_ends {
        by_hub.entry(e.hub).or_default().push(e);
    }
    for (h, ends) in by_hub {
        if let Some(hub) = hubs.get(h)
            && ends.len() >= 2
        {
            extrude_hub(&ends, hub, world_offset, dims, parts);
        }
    }
}

/// A hub's planar shape: the arms in outline order, the deck-edge path after
/// each, the outline polygon, and whether its curbs can be drawn.
struct HubShape<'a> {
    arms: Vec<HubArm<'a>>,
    order: Vec<usize>,
    gaps: Vec<Vec<EdgePoint>>,
    poly: Vec<[f32; 2]>,
    curbs: bool,
}

/// The planar shape of the hub `ends` open into, in world XZ (window frame
/// plus `world_offset`): the walked outline, or - if it does not close round
/// every arm, or is not simple (a pathological graph) - straight chords
/// between the arms in order round the hub's centre, or at the last the
/// corners' convex hull with no curbs. `None` for an arm of a node the hub
/// does not hold.
fn hub_shape<'a>(
    ends: &[&'a RoadEnd],
    hub: &Hub,
    world_offset: [f32; 2],
    dims: &Dims,
) -> Option<HubShape<'a>> {
    let world = |p: (f32, f32)| [p.0 + world_offset[0], p.1 + world_offset[1]];
    let arms: Vec<HubArm> = ends
        .iter()
        .map(|e| HubArm {
            end: e,
            m: [e.cx + world_offset[0], e.cz + world_offset[1]],
            l: [-e.dz, e.dx],
            spine: e.spine.iter().map(|&p| world(p)).collect(),
        })
        .collect();
    // The hub's nodes, those a link too short for both its ends' corners
    // joins drawn as one (their corners would cross), and every spoke
    // leaving them, counter-clockwise.
    let index = |nd: usize| hub.nodes.iter().position(|&x| x == nd);
    let pos: Vec<[f32; 2]> = hub.points.iter().map(|&p| world(p)).collect();
    let mut arm_at = Vec::with_capacity(arms.len());
    for a in &arms {
        arm_at.push(index(a.end.node)?);
    }
    let links: Vec<(usize, usize, f32)> = hub
        .links
        .iter()
        .filter_map(|&(a, b, w)| Some((index(a)?, index(b)?, w)))
        .collect();
    let mut group: Vec<usize> = (0..pos.len()).collect();
    for &(a, b, _) in &links {
        let ab = sub2(pos[b], pos[a]);
        let len = ab[0].hypot(ab[1]);
        // The widest street leaving each end across the link (not along it):
        // its corner reaches that far along the link.
        let across = |v: usize, dir: [f32; 2]| {
            let arm_w = arms
                .iter()
                .zip(&arm_at)
                .filter(|(_, at)| **at == v)
                .filter_map(|(a, _)| {
                    let to = sub2(a.m, pos[v]);
                    let l = to[0].hypot(to[1]).max(1.0e-6);
                    (dot2(to, dir).abs() / l < 0.87).then_some(a.end.half_w)
                });
            let link_w = links.iter().filter_map(|&(p, q, w)| {
                let other = if p == v {
                    q
                } else if q == v {
                    p
                } else {
                    return None;
                };
                let to = sub2(pos[other], pos[v]);
                let l = to[0].hypot(to[1]).max(1.0e-6);
                (dot2(to, dir).abs() / l < 0.87).then_some(w)
            });
            arm_w.chain(link_w).fold(0.0_f32, f32::max)
        };
        let dir = scale2(ab, 1.0 / len.max(1.0e-6));
        if len < across(a, dir) + across(b, dir) {
            let (ra, rb) = (root(&mut group, a), root(&mut group, b));
            group[ra.max(rb)] = ra.min(rb);
        }
    }
    let roots: Vec<usize> = (0..pos.len()).map(|v| root(&mut group, v)).collect();
    let mut slot = vec![usize::MAX; pos.len()];
    let mut nodes: Vec<HubNode> = Vec::new();
    for v in 0..pos.len() {
        if slot[roots[v]] == usize::MAX {
            slot[roots[v]] = nodes.len();
            let members: Vec<[f32; 2]> = (0..pos.len())
                .filter(|&u| roots[u] == roots[v])
                .map(|u| pos[u])
                .collect();
            let k = members.len() as f32;
            nodes.push(HubNode {
                p: [
                    members.iter().map(|p| p[0]).sum::<f32>() / k,
                    members.iter().map(|p| p[1]).sum::<f32>() / k,
                ],
                spokes: Vec::new(),
            });
        }
    }
    let node_of = |v: usize| slot[roots[v]];
    let mut arm_node = Vec::with_capacity(arms.len());
    for (k, a) in arms.iter().enumerate() {
        let g = node_of(arm_at[k]);
        let to = sub2(a.m, nodes[g].p);
        nodes[g].spokes.push((to[1].atan2(to[0]), Spoke::Arm(k)));
        arm_node.push(g);
    }
    for (id, &(a, b, half_w)) in links.iter().enumerate() {
        let (ga, gb) = (node_of(a), node_of(b));
        let ab = sub2(pos[b], pos[a]);
        let len = ab[0].hypot(ab[1]);
        if ga == gb || len < 1.0e-4 {
            continue; // inside one drawn node
        }
        for (from, to, start, dir) in [
            (ga, gb, pos[a], scale2(ab, 1.0 / len)),
            (gb, ga, pos[b], scale2(ab, -1.0 / len)),
        ] {
            nodes[from].spokes.push((
                dir[1].atan2(dir[0]),
                Spoke::Link {
                    id,
                    to,
                    start,
                    dir,
                    half_w,
                    len,
                },
            ));
        }
    }
    for node in &mut nodes {
        node.spokes.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    let n = arms.len();
    let outer = dims.curb_top_width + dims.chamfer_width;
    let outline = |order: &[usize], gaps: &[Vec<EdgePoint>]| -> Vec<[f32; 2]> {
        let mut pts = Vec::new();
        for (k, gap) in order.iter().zip(gaps) {
            pts.push(arms[*k].corner(-1.0));
            pts.extend(gap[..gap.len() - 1].iter().map(|e| e.p));
        }
        pts
    };
    let (mut order, mut gaps) = walk_outline(&nodes, &arms, &arm_node, outer).unwrap_or_default();
    let mut poly = clean_outline(&outline(&order, &gaps));
    if order.len() != n || !is_simple(&poly) || signed_area2(&poly) <= 0.0 {
        let c = world(hub.centre);
        order = (0..n).collect();
        order.sort_by(|&a, &b| {
            let angle = |k: usize| (arms[k].m[1] - c[1]).atan2(arms[k].m[0] - c[0]);
            angle(a).total_cmp(&angle(b))
        });
        gaps = (0..n)
            .map(|i| {
                let (a, b) = (&arms[order[i]], &arms[order[(i + 1) % n]]);
                vec![
                    EdgePoint {
                        p: a.corner(1.0),
                        n: a.l,
                    },
                    EdgePoint {
                        p: b.corner(-1.0),
                        n: scale2(b.l, -1.0),
                    },
                ]
            })
            .collect();
        poly = clean_outline(&outline(&order, &gaps));
    }
    let curbs = is_simple(&poly) && signed_area2(&poly) > 0.0;
    if !curbs {
        let corners: Vec<[f32; 2]> = arms
            .iter()
            .flat_map(|a| [a.corner(-1.0), a.corner(1.0)])
            .collect();
        poly = convex_hull(&corners);
    }
    Some(HubShape {
        arms,
        order,
        gaps,
        poly,
        curbs,
    })
}

/// The deck outline (window frame) of the hub `ends` open into: the ground
/// its flat deck must clear (#584), read before the deck heights exist.
pub(crate) fn hub_outline(ends: &[&RoadEnd], hub: &Hub, dims: &Dims) -> Vec<[f32; 2]> {
    hub_shape(ends, hub, [0.0; 2], dims).map_or_else(Vec::new, |s| s.poly)
}

/// One hub's deck and curbs - see [`extrude_hubs`].
fn extrude_hub(
    ends: &[&RoadEnd],
    hub: &Hub,
    world_offset: [f32; 2],
    dims: &Dims,
    parts: &mut RoadParts,
) {
    let Some(HubShape {
        arms,
        order,
        gaps,
        poly,
        curbs,
    }) = hub_shape(ends, hub, world_offset, dims)
    else {
        return;
    };
    let n = arms.len();
    let outer = dims.curb_top_width + dims.chamfer_width;
    let hub_y = arms.iter().map(|a| a.end.deck_y).fold(f32::MIN, f32::max);
    // A one-node hub's outline is star-shaped from its node (every corner and
    // wrap lies in its own sector): fan from the node, so no ear is a
    // sliver. Anything else - a cluster, a fallback outline - is ear-clipped.
    let mut poly = poly;
    let node = [
        hub.points[0].0 + world_offset[0],
        hub.points[0].1 + world_offset[1],
    ];
    let star = hub.points.len() == 1
        && (0..poly.len()).all(|i| {
            let (a, b) = (sub2(poly[i], node), sub2(poly[(i + 1) % poly.len()], node));
            cross2(a, b) > COLLINEAR_SIN * (dot2(a, a) * dot2(b, b)).sqrt() + OUTLINE_EPS
        });
    let tris: Vec<[usize; 3]> = if star {
        let k = poly.len();
        poly.push(node);
        (0..k).map(|i| [k, i, (i + 1) % k]).collect()
    } else {
        let Some(tris) = ear_clip(&poly) else {
            return;
        };
        tris
    };

    // --- Deck: the ear-clipped outline, flat at the hub height; a mouth
    //     corner keeps its own deck height so the seam with its ribbon holds
    //     even when the levelling capped out under-pinned, and its ribbon's
    //     mouth shading (#1567). ---
    let mouth_at = |p: [f32; 2]| {
        arms.iter()
            .find(|a| {
                [a.corner(-1.0), a.corner(1.0)]
                    .iter()
                    .any(|q| (q[0] - p[0]).hypot(q[1] - p[1]) < 1.0e-4)
            })
            .map(|a| a.end)
    };
    let verts: Vec<[f32; 3]> = poly
        .iter()
        .map(|p| [p[0], mouth_at(*p).map_or(hub_y, |e| e.deck_y), p[1]])
        .collect();
    let mut vn = vec![[0.0_f32; 3]; verts.len()];
    let base = parts.deck.vertices.len() as u32;
    for t in &tris {
        let (a, b, cc) = (verts[t[0]], verts[t[1]], verts[t[2]]);
        let mut f = cross(sub3(b, a), sub3(cc, a));
        let wind_up = f[1] >= 0.0;
        if !wind_up {
            f = [-f[0], -f[1], -f[2]];
        }
        for &k in t {
            vn[k] = [vn[k][0] + f[0], vn[k][1] + f[1], vn[k][2] + f[2]];
        }
        let (i0, i1, i2) = (base + t[0] as u32, base + t[1] as u32, base + t[2] as u32);
        if wind_up {
            parts.deck.indices.extend_from_slice(&[i0, i1, i2]);
        } else {
            parts.deck.indices.extend_from_slice(&[i0, i2, i1]);
        }
    }
    for ((v, nrm), p) in verts.iter().zip(&vn).zip(&poly) {
        parts.deck.vertices.push(*v);
        parts
            .deck
            .normals
            .push(mouth_at(*p).map_or_else(|| normalize(*nrm), |e| e.deck_normal));
        parts.deck.uvs.push([v[0] / UV_TILE_M, v[2] / UV_TILE_M]);
    }

    if !curbs {
        return;
    }
    // --- Curbs: along every gap between neighbouring arms, the ribbon's own
    //     profile - curb wall, top, chamfer, skirt - swept along the deck
    //     edge, offset outward along each point's curb normal (so it starts
    //     and ends exactly on the ribbons' curbs). ---
    let (ct, cf, ch) = (dims.curb_top_width, dims.chamfer_width, dims.curb_height);
    let c = [
        hub.centre.0 + world_offset[0],
        hub.centre.1 + world_offset[1],
    ];
    let centre3 = [c[0], hub_y - dims.skirt_depth * 0.5, c[1]];
    for (i, gap) in gaps.iter().enumerate() {
        let (a, b) = (&arms[order[i]], &arms[order[(i + 1) % n]]);
        // Arc length along the deck edge, for the skirt-foot blend and V.
        let mut acc = vec![0.0_f32];
        for w in gap.windows(2) {
            let l = (w[1].p[0] - w[0].p[0]).hypot(w[1].p[1] - w[0].p[1]);
            acc.push(acc[acc.len() - 1] + l);
        }
        let total = acc[acc.len() - 1].max(1.0e-6);
        let last = gap.len() - 1;
        let mut rows: [Vec<[f32; 3]>; 5] = Default::default();
        let mut vlen = Vec::with_capacity(gap.len());
        for (k, e) in gap.iter().enumerate() {
            let f = acc[k] / total;
            let dy = match k {
                0 => a.end.deck_y,
                _ if k == last => b.end.deck_y,
                _ => hub_y,
            };
            let fy = a.end.skirt_y + (b.end.skirt_y - a.end.skirt_y) * f;
            let at = |off: f32, y: f32| [e.p[0] + e.n[0] * off, y, e.p[1] + e.n[1] * off];
            rows[0].push(at(0.0, dy));
            rows[1].push(at(0.0, dy + ch));
            rows[2].push(at(ct, dy + ch));
            rows[3].push(at(outer, dy));
            rows[4].push(at(outer, fy));
            vlen.push(acc[k] / UV_TILE_M);
        }
        let (u1, u2) = (ch / UV_TILE_M, (ch + ct) / UV_TILE_M);
        let u3 = (ch + ct + cf) / UV_TILE_M;
        let u4 = u3 + dims.skirt_depth / UV_TILE_M;
        push_curb_face(parts, centre3, &rows[0], &rows[1], (0.0, u1), &vlen); // curb wall
        push_curb_face(parts, centre3, &rows[1], &rows[2], (u1, u2), &vlen); // curb top
        push_curb_face(parts, centre3, &rows[2], &rows[3], (u2, u3), &vlen); // chamfer
        push_curb_face(parts, centre3, &rows[3], &rows[4], (u3, u4), &vlen); // skirt
    }
}

/// The convex hull of `pts`, counter-clockwise (Andrew's monotone chain) -
/// the deck of last resort for a junction whose mouths admit no simple
/// outline.
fn convex_hull(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut p: Vec<[f32; 2]> = pts.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1.0e-4);
    if p.len() < 3 {
        return p;
    }
    let mut hull: Vec<[f32; 2]> = Vec::with_capacity(p.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f32; 2]>> = if pass == 0 {
            Box::new(p.iter())
        } else {
            Box::new(p.iter().rev())
        };
        for &q in iter {
            while hull.len() >= start + 2 {
                let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
                if cross2(sub2(b, a), sub2(q, b)) <= 0.0 {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(q);
        }
        hull.pop();
    }
    hull
}

/// Push one hub curb face strip into `parts.structure`: `inner`/`outer` are
/// the face's two edges at each sample along the gap, smooth-shaded ALONG it
/// (welded vertices carrying averaged segment normals) with a hard crease
/// ACROSS the profile (one strip per face). Each triangle is wound so its
/// front matches its own averaged shading normal, which points away from the
/// hub's interior reference point `center`.
fn push_curb_face(
    parts: &mut RoadParts,
    center: [f32; 3],
    inner: &[[f32; 3]],
    outer: &[[f32; 3]],
    uv_u: (f32, f32),
    v: &[f32],
) {
    let n = inner.len();
    if n < 2 {
        return;
    }
    let seg: Vec<[f32; 3]> = (0..n - 1)
        .map(|k| quad_normal(inner[k], outer[k], inner[k + 1], outer[k + 1], center))
        .collect();
    let vn: Vec<[f32; 3]> = (0..n)
        .map(|i| {
            let mut acc = [0.0_f32; 3];
            for s in [i.checked_sub(1), (i < seg.len()).then_some(i)]
                .into_iter()
                .flatten()
            {
                acc = [acc[0] + seg[s][0], acc[1] + seg[s][1], acc[2] + seg[s][2]];
            }
            normalize(acc)
        })
        .collect();
    let g = &mut parts.structure;
    let base = g.vertices.len() as u32;
    for i in 0..n {
        g.vertices.push(inner[i]);
        g.vertices.push(outer[i]);
        g.normals.push(vn[i]);
        g.normals.push(vn[i]);
        g.uvs.push([uv_u.0, v[i]]);
        g.uvs.push([uv_u.1, v[i]]);
    }
    let mut tri = |a: u32, b: u32, c: u32, na: [f32; 3], nb: [f32; 3], nc: [f32; 3]| {
        let (qa, qb, qc) = (
            g.vertices[a as usize],
            g.vertices[b as usize],
            g.vertices[c as usize],
        );
        let geo = cross(sub3(qb, qa), sub3(qc, qa));
        let nsum = [
            na[0] + nb[0] + nc[0],
            na[1] + nb[1] + nc[1],
            na[2] + nb[2] + nc[2],
        ];
        if dot(geo, geo) < 1.0e-12 {
            return; // a degenerate sliver (a wrap point's zero-width inner side)
        }
        if dot(geo, nsum) >= 0.0 {
            g.indices.extend_from_slice(&[a, b, c]);
        } else {
            g.indices.extend_from_slice(&[a, c, b]);
        }
    };
    for i in 0..n - 1 {
        let (li, ri, lj, rj) = (
            base + 2 * i as u32,
            base + 2 * i as u32 + 1,
            base + 2 * i as u32 + 2,
            base + 2 * i as u32 + 3,
        );
        tri(li, ri, rj, vn[i], vn[i], vn[i + 1]);
        tri(li, rj, lj, vn[i], vn[i + 1], vn[i + 1]);
    }
}

#[cfg(test)]
mod tests;
