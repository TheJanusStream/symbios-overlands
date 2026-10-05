//! Ribbon extrusion: one chain's closed cross-section swept along its centreline.
//! The profile is a chamfered curb framing a flat deck over a skirt of FIXED
//! depth, so a deck riding high over a dip floats clear as a bridge rather than
//! filling it. Frames miter through bends to hold a constant width - a bend
//! sharper than one arc step arrives already rounded, and each line a profile
//! point traces is collapsed wherever a bend still runs it backwards, so no
//! face folds ([`crate::urban::bends`], #1567). UVs run on arc length so the
//! texture flows down the street, and each profile face is its own strip -
//! normals average ALONG the road while the creases across it stay sharp; at a
//! hub's mouth the deck row shades halfway to the hub's flat deck, as the hub's
//! mouth corners do, so the seam shows no crease. Deck, structure and neon go
//! to separate buffers; an end no hub closes (a dead-end #579, a perimeter clip
//! #582) gets an explicit cross-section cap.

use crate::urban::bends::unfold_rail;
use crate::urban::math::{cross, dot, normalize, sub3};
use crate::urban::truncation::ChainEnds;
use crate::urban::{Chain, ChainSample, Dims, RoadEnd, RoadParts, splits_along_ad};

/// Spacing (m) of ribbon cross-sections along a road. Straight edges are
/// subdivided to this so the deck still drapes over relief between graph nodes.
pub(crate) const RIBBON_STEP_M: f32 = 3.0;
/// World metres per UV tile, both along the road and around the cross-section.
pub(crate) const UV_TILE_M: f32 = 6.0;
/// Width (m) of the emissive neon edge-line strip riding the inner curb top.
pub(crate) const NEON_LINE_WIDTH_M: f32 = 0.07;
/// Lift (m) of that strip above the curb top so it sits proud and never
/// z-fights the curb face it rides (see the coplanar-z-fight rule).
pub(crate) const NEON_LINE_LIFT_M: f32 = 0.04;

/// The closed cross-section (lateral offset `u`, height `h` relative to the
/// deck top) for a deck of half-width `w`: flat deck, chamfered curb framing
/// each edge, and a deep skirt capped by a bottom face. Ten points, traced
/// around the solid; consecutive points (wrapping) are the profile's faces.
pub(crate) fn profile(w: f32, dims: &Dims) -> [(f32, f32); 10] {
    let (ch, ct, cf, sd) = (
        dims.curb_height,
        dims.curb_top_width,
        dims.chamfer_width,
        dims.skirt_depth,
    );
    let wo = w + ct + cf;
    [
        (-w, 0.0),     // 0 deck top left
        (w, 0.0),      // 1 deck top right
        (w, ch),       // 2 right curb inner top
        (w + ct, ch),  // 3 right curb outer top
        (wo, 0.0),     // 4 right chamfer base
        (wo, -sd),     // 5 right skirt bottom
        (-wo, -sd),    // 6 left skirt bottom
        (-wo, 0.0),    // 7 left chamfer base
        (-w - ct, ch), // 8 left curb outer top
        (-w, ch),      // 9 left curb inner top
    ]
}

/// Per-vertex lateral (right) axis and miter scale. Endpoints use the segment
/// perpendicular; interior vertices use the bisector, scaled by `1/cos(½θ)` to
/// hold a constant width through the bend (clamped so sharp corners don't
/// spike).
pub(crate) fn frame_right(pts: &[(f32, f32)], i: usize) -> (f32, f32, f32) {
    let perp = |d: (f32, f32)| (-d.1, d.0);
    let norm = |d: (f32, f32)| {
        let l = (d.0 * d.0 + d.1 * d.1).sqrt().max(1.0e-6);
        (d.0 / l, d.1 / l)
    };
    let n = pts.len();
    if i == 0 {
        let r = perp(norm((pts[1].0 - pts[0].0, pts[1].1 - pts[0].1)));
        return (r.0, r.1, 1.0);
    }
    if i == n - 1 {
        let r = perp(norm((pts[i].0 - pts[i - 1].0, pts[i].1 - pts[i - 1].1)));
        return (r.0, r.1, 1.0);
    }
    let rin = perp(norm((pts[i].0 - pts[i - 1].0, pts[i].1 - pts[i - 1].1)));
    let rout = perp(norm((pts[i + 1].0 - pts[i].0, pts[i + 1].1 - pts[i].1)));
    let mr = norm((rin.0 + rout.0, rin.1 + rout.1));
    let cos_half = (mr.0 * rin.0 + mr.1 * rin.1).max(0.34);
    (mr.0, mr.1, (1.0 / cos_half).min(3.0))
}

/// Subdivide a polyline so no segment exceeds `step`, for smooth vertical drape.
pub(crate) fn densify(pts: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let Some(&first) = pts.first() else {
        return out;
    };
    out.push(first);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (lx, lz) = (b.0 - a.0, b.1 - a.1);
        let len = (lx * lx + lz * lz).sqrt();
        let segs = (len / step).ceil().max(1.0) as usize;
        for s in 1..=segs {
            let t = s as f32 / segs as f32;
            out.push((a.0 + lx * t, a.1 + lz * t));
        }
    }
    out
}

/// Per-vertex extrusion frame. The deck is **flat across** (no lateral banking,
/// so vehicles don't roll side-to-side) and drainage-correct: `base_y` is the
/// flat deck height - lifted to clear the highest terrain under the road and
/// longitudinally grade-limited - and `skirt_bottom_y` is a FIXED `skirt_depth`
/// below it (no terrain reach), so a deck riding high over a dip floats clear as
/// a bridge. `arc` is the running arc length (for V UVs).
struct Frame {
    cx: f32,
    cz: f32,
    rx: f32,
    rz: f32,
    scale: f32,
    base_y: f32,
    skirt_bottom_y: f32,
    arc: f32,
}

/// Interior reference point of a chain segment (the centreline at mid-height
/// between the deck and the skirt bottom), used to orient each face's normal
/// outward.
fn beam_axis(f0: &Frame, f1: &Frame, world_offset: [f32; 2]) -> [f32; 3] {
    [
        (f0.cx + f1.cx) * 0.5 + world_offset[0],
        (f0.base_y + f1.base_y + f0.skirt_bottom_y + f1.skirt_bottom_y) * 0.25,
        (f0.cz + f1.cz) * 0.5 + world_offset[1],
    ]
}

/// Flat per-face normal for a road quad, flipped to point away from the segment's
/// interior `axis` so every surface faces outward (deck up, skirt out, etc.).
pub(crate) fn quad_normal(
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    axis: [f32; 3],
) -> [f32; 3] {
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    orient_out(cross(e1, e2), [a, b, c, d], axis)
}

/// `nrm`, flipped if it points into the quad `q`'s segment (towards `axis`),
/// and made unit length.
fn orient_out(nrm: [f32; 3], q: [[f32; 3]; 4], axis: [f32; 3]) -> [f32; 3] {
    let fc = [
        (q[0][0] + q[1][0] + q[2][0] + q[3][0]) * 0.25,
        (q[0][1] + q[1][1] + q[2][1] + q[3][1]) * 0.25,
        (q[0][2] + q[1][2] + q[2][2] + q[3][2]) * 0.25,
    ];
    let outward = sub3(fc, axis);
    if dot(nrm, outward) < 0.0 {
        normalize([-nrm[0], -nrm[1], -nrm[2]])
    } else {
        normalize(nrm)
    }
}

/// The outward normal of one strip quad - `q` holds its left and right edge
/// points at one frame, then at the next - or `None` for a quad of no width,
/// which no vertex should take its shading from. Where both edges move it is
/// the quad's own area vector, so a quad a graded bend twists shades the same
/// whichever way the street turns. A collapsed fold (#1567) leaves an edge
/// standing still in plan, or crawling beside the other: a corner every
/// frame reaches at its own height, a vertical riser on a graded street that
/// would tip the area vector onto its side. Such a quad takes the normal of
/// the triangle it is drawn with on the edge that moves.
fn strip_normal(q: [[f32; 3]; 4], axis: [f32; 3]) -> Option<[f32; 3]> {
    let [a, b, c, d] = q;
    let plan = |p: [f32; 3], r: [f32; 3]| (r[0] - p[0]).hypot(r[2] - p[2]);
    let (left, right) = (plan(a, c), plan(b, d));
    let along_ad = splits_along_ad(&q);
    let nrm = match (edge_moves(left, right), edge_moves(right, left)) {
        (true, true) => cross(sub3(d, a), sub3(c, b)),
        // The left edge stands still: the triangle on the right edge.
        (false, true) if along_ad => cross(sub3(b, a), sub3(d, a)),
        (false, true) => cross(sub3(d, b), sub3(c, b)),
        // The right edge stands still: the triangle on the left edge.
        (true, false) if along_ad => cross(sub3(d, a), sub3(c, a)),
        (true, false) => cross(sub3(b, a), sub3(c, a)),
        (false, false) => return None,
    };
    (dot(nrm, nrm) > PINCHED_NORMAL2).then(|| orient_out(nrm, q, axis))
}

/// Whether a strip edge travelling `own` metres in plan beside one
/// travelling `other` moves, for its quad's shading: at least
/// [`STILL_EDGE_M`], and either [`SLOW_EDGE_M`] or [`SLOW_EDGE_RATIO`] of
/// the other edge's travel.
fn edge_moves(own: f32, other: f32) -> bool {
    own >= STILL_EDGE_M && (own >= SLOW_EDGE_M || own >= SLOW_EDGE_RATIO * other)
}

/// An edge travelling less than this (m) in plan stands still: a collapsed
/// corner, to within float noise.
const STILL_EDGE_M: f32 = 1.0e-3;
/// An edge travelling at least this (m) in plan always moves; a slower one
/// moves only beside an edge no more than four times faster.
const SLOW_EDGE_M: f32 = 0.05;
/// See [`SLOW_EDGE_M`].
const SLOW_EDGE_RATIO: f32 = 0.25;
/// Below this squared cross product (m⁴) a strip quad's edges span nothing.
const PINCHED_NORMAL2: f32 = 1.0e-12;

/// Per-row normals of a strip from its segments' normals: each row the
/// average of the (up to two) segments meeting at it - so the strip shades
/// smoothly along its length - leaving out a segment with no width.
fn smoothed_rows(seg_normals: &[Option<[f32; 3]>]) -> Vec<[f32; 3]> {
    let rows = seg_normals.len() + 1;
    (0..rows)
        .map(|i| {
            let mut acc = [0.0_f32; 3];
            for s in [i.checked_sub(1), (i < seg_normals.len()).then_some(i)]
                .into_iter()
                .flatten()
            {
                if let Some(nrm) = seg_normals[s] {
                    acc = [acc[0] + nrm[0], acc[1] + nrm[1], acc[2] + nrm[2]];
                }
            }
            normalize(acc)
        })
        .collect()
}

/// The line the profile point `lateral` metres right of the centreline
/// traces through `frames` (mitred like the frame), shifted into the
/// full-terrain frame by `world_offset`, with every stretch a bend runs
/// backwards collapsed (#1567, [`unfold_rail`]). Each frame's point keeps
/// its frame's height, so every cross-section stays the profile it was
/// drawn as; where a collapsed corner is reached at several heights, the
/// faces meet there in a vertical seam.
fn rail(frames: &[Frame], lateral: f32, world_offset: [f32; 2]) -> Vec<[f32; 2]> {
    let centre: Vec<[f32; 2]> = frames.iter().map(|f| [f.cx, f.cz]).collect();
    let mut line: Vec<[f32; 2]> = frames
        .iter()
        .map(|f| {
            let off = lateral * f.scale;
            [f.cx + f.rx * off, f.cz + f.rz * off]
        })
        .collect();
    unfold_rail(&mut line, &centre);
    line.iter()
        .map(|p| [p[0] + world_offset[0], p[1] + world_offset[1]])
        .collect()
}

/// The normal a deck row at a hub mouth takes (#1567): halfway between the
/// ribbon's own end segment `seg` and the flat hub deck it meets, which the
/// hub's mouth corners take too, so the seam does not shade as a crease on a
/// graded approach.
fn mouth_deck_normal(seg: Option<[f32; 3]>) -> [f32; 3] {
    let n = seg.unwrap_or(UP);
    normalize([n[0], n[1] + 1.0, n[2]])
}

const UP: [f32; 3] = [0.0, 1.0, 0.0];

/// Extrude the curb/skirt profile along one chain into `parts`. The deck drapes
/// over the terrain **flat-across and upward-only** (it never sinks below the
/// terrain - see [`Frame`]), shifted into the full-terrain frame by `world_offset`.
/// The drivable deck top, the structural curb/skirt and the emissive neon
/// edge-lines are routed to their respective [`RoadParts`] buffers.
/// `sample` is the chain's terrain-sampled frames ([`crate::urban::sample_chain`]) and `base_y`
/// the resolved per-frame deck height ([`crate::urban::level_chain`], with junction pins folded
/// in by the network pass) - both supplied by the caller so the heightmap is
/// sampled exactly once and the pre-pass and mesh agree to the bit (#584).
/// `ends` says how each end closes (#1558): the hub it opens into, where it
/// records its mouth, or an end cap.
#[allow(clippy::too_many_arguments)] // each arg is a distinct input/sink.
pub(crate) fn extrude_ribbon(
    chain: &Chain,
    sample: &ChainSample,
    base_y: &[f32],
    world_offset: [f32; 2],
    dims: &Dims,
    ends: ChainEnds,
    road_ends: &mut Vec<RoadEnd>,
    parts: &mut RoadParts,
) {
    let prof = profile(chain.half_w, dims);
    let half_w = chain.half_w;

    let frames: Vec<Frame> = sample
        .frames
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let by = base_y[i];
            // The skirt drops a FIXED `skirt_depth` below the deck - it no longer
            // reaches down to meet the terrain. Where the deck rides high over a
            // dip the underside stays shallow and floats clear, so a high road
            // reads as a bridge rather than a solid earth-filled embankment.
            let skirt_bottom_y = by - dims.skirt_depth;
            Frame {
                cx: r.cx,
                cz: r.cz,
                rx: r.rx,
                rz: r.rz,
                scale: r.scale,
                base_y: by,
                skirt_bottom_y,
                arc: r.arc,
            }
        })
        .collect();
    let last = frames.len() - 1;

    // Each profile point's line along the chain, unfolded where a bend would
    // run it backwards (#1567): every face and cap is built on these.
    let rails: Vec<Vec<[f32; 2]>> = prof
        .iter()
        .map(|&(pu, _)| rail(&frames, pu, world_offset))
        .collect();
    // World position of profile point `pi` at frame `i`: flat deck (no lateral
    // banking); the skirt-bottom points (5, 6) drop to `skirt_bottom_y`.
    let world = |i: usize, pi: usize| {
        let f = &frames[i];
        let y = if pi == 5 || pi == 6 {
            f.skirt_bottom_y
        } else {
            f.base_y + prof[pi].1
        };
        [rails[pi][i][0], y, rails[pi][i][1]]
    };

    // Cumulative cross-section perimeter, for the U coordinate.
    let mut u = [0.0_f32; 10];
    for j in 1..10 {
        let (a, b) = (prof[j - 1], prof[j]);
        u[j] = u[j - 1] + (b.0 - a.0).hypot(b.1 - a.1);
    }
    // Per-frame along-road V, shared by every profile face.
    let v: Vec<f32> = frames.iter().map(|f| f.arc / UV_TILE_M).collect();

    // One strip per face: normals are averaged ALONG the chain (smooth
    // ribbon) but each face is its own strip, so the crease ACROSS the
    // profile stays sharp.
    let faces: Vec<FaceStrip> = (0..10)
        .map(|j| {
            let k = (j + 1) % 10;
            let left: Vec<[f32; 3]> = (0..frames.len()).map(|i| world(i, j)).collect();
            let right: Vec<[f32; 3]> = (0..frames.len()).map(|i| world(i, k)).collect();
            let seg_n = (0..last)
                .map(|i| {
                    let axis = beam_axis(&frames[i], &frames[i + 1], world_offset);
                    strip_normal([left[i], right[i], left[i + 1], right[i + 1]], axis)
                })
                .collect();
            FaceStrip { left, right, seg_n }
        })
        .collect();

    // Record this chain's ends that open into a hub so the hub builder can
    // meet each road at its exact deck mouth, heading, height and shading.
    let mut deck_rows = smoothed_rows(&faces[0].seg_n);
    for (slot, hub) in ends.hub.iter().enumerate() {
        let Some(hub) = *hub else {
            continue;
        };
        let (fi, gi, seg) = if slot == 0 {
            (0, 1, 0)
        } else {
            (last, last - 1, last - 1)
        };
        let (f, g) = (&frames[fi], &frames[gi]);
        let deck_normal = mouth_deck_normal(faces[0].seg_n[seg]);
        deck_rows[fi] = deck_normal;
        road_ends.push(mouth(
            chain,
            slot,
            hub,
            ends.trim[slot],
            [(f.cx, f.cz), (g.cx, g.cz)],
            [f.base_y, f.skirt_bottom_y],
            deck_normal,
        ));
    }

    for (j, face) in faces.iter().enumerate() {
        let k = (j + 1) % 10;
        let (uj, uk) = (u[j] / UV_TILE_M, u[k] / UV_TILE_M);
        // Profile face 0→1 is the flat drivable deck top; every other face is
        // structural (curb walls, chamfers, the deep skirt and its bottom cap).
        if j == 0 {
            parts
                .deck
                .push_strip(&face.left, &face.right, &deck_rows, (uj, uk), &v);
        } else {
            let rows = smoothed_rows(&face.seg_n);
            parts
                .structure
                .push_strip(&face.left, &face.right, &rows, (uj, uk), &v);
        }
    }

    push_neon(&frames, &rails, half_w, dims, world_offset, parts);

    // End caps: an open chain end leaves the extruded cross-section open - a
    // visible hollow tube into the road's underside. Close it with a flat
    // cross-section cap facing outward (away from the ribbon). The junction
    // plan says which ends need it: a dead-end / cul-de-sac (#579), a
    // district-edge clip running off the network perimeter (#582), and a
    // junction whose hub kept only this arm (#1558). An end opening into a
    // hub is closed by it; a loop closure / used-edge break stays open.
    for (slot, &cap) in ends.cap.iter().enumerate() {
        if !cap {
            continue;
        }
        let (ei, ii) = if slot == 0 {
            (0, 1.min(last))
        } else {
            (last, last.saturating_sub(1))
        };
        let (fe, fi) = (&frames[ei], &frames[ii]);
        // The cap is the (vertical) end cross-section, so its true normal is the
        // HORIZONTAL lateral-perp `(rx,rz)⊥` - independent of the deck/skirt grade
        // - oriented away from the ribbon. Using the road tangent would tilt the
        // normal by the longitudinal slope and mis-shade the cap (review
        // wf_aabe1626).
        let perp = [-fe.rz, fe.rx];
        let away = [fe.cx - fi.cx, fe.cz - fi.cz];
        let s = if perp[0] * away[0] + perp[1] * away[1] >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let outward = [perp[0] * s, 0.0, perp[1] * s];
        let pts: [[f32; 3]; 10] = std::array::from_fn(|pi| world(ei, pi));
        push_end_cap(parts, &pts, &prof, outward);
    }
}

/// One profile face's strip along a chain: its two edges per frame and each
/// segment's outward normal (`None` where a collapsed fold left no area).
struct FaceStrip {
    left: Vec<[f32; 3]>,
    right: Vec<[f32; 3]>,
    seg_n: Vec<Option<[f32; 3]>>,
}

/// Emissive neon edge-lines: a thin strip riding proud of each curb's inner
/// top crease (lateral ±half_w, just above the curb top), lifted clear so it
/// never z-fights the curb. Kept on its own surface for the hot emissive
/// material. Its inner edge is the deck edge's own rail (`rails[1]`,
/// `rails[0]`), its outer edge a rail of its own, so it folds nowhere the
/// deck does not.
fn push_neon(
    frames: &[Frame],
    rails: &[Vec<[f32; 2]>],
    half_w: f32,
    dims: &Dims,
    world_offset: [f32; 2],
    parts: &mut RoadParts,
) {
    let lift = dims.curb_height + NEON_LINE_LIFT_M;
    let outer_w = half_w + NEON_LINE_WIDTH_M;
    let lines = [
        (&rails[1], rail(frames, outer_w, world_offset)),
        (&rails[0], rail(frames, -outer_w, world_offset)),
    ];
    for (inner, outer) in &lines {
        let at = |line: &[[f32; 2]], i: usize| [line[i][0], frames[i].base_y + lift, line[i][1]];
        for i in 0..frames.len() - 1 {
            let (f0, f1) = (&frames[i], &frames[i + 1]);
            let q = [
                at(inner, i),
                at(outer, i),
                at(inner, i + 1),
                at(outer, i + 1),
            ];
            let Some(nrm) = strip_normal(q, beam_axis(f0, f1, world_offset)) else {
                continue; // folded onto a point: nothing to draw
            };
            let (vi, vi1) = (f0.arc / UV_TILE_M, f1.arc / UV_TILE_M);
            parts.neon.push_quad(
                q[0],
                q[1],
                q[2],
                q[3],
                [[0.0, vi], [1.0, vi], [0.0, vi1], [1.0, vi1]],
                nrm,
            );
        }
    }
}

/// The mouth chain end `slot`, pulled back `trim` metres, opens into `hub`
/// with: its end frame centre `ends[0]`, the heading away from the hub along
/// the end segment to the next frame `ends[1]` (which the mouth frame's
/// right axis is perpendicular to), its deck and skirt-bottom heights
/// `heights`, the normal its deck row is shaded with, and the stub of chain
/// from its node to the mouth that the hub draws.
fn mouth(
    chain: &Chain,
    slot: usize,
    hub: usize,
    trim: f32,
    ends: [(f32, f32); 2],
    heights: [f32; 2],
    deck_normal: [f32; 3],
) -> RoadEnd {
    let [(cx, cz), (nx, nz)] = ends;
    let (hx, hz) = (nx - cx, nz - cz);
    let len = hx.hypot(hz).max(1.0e-6);
    // The first `trim` metres of the chain from its node end, ending exactly
    // on the mouth centre.
    let mut pts = chain.pts.clone();
    if slot == 1 {
        pts.reverse();
    }
    let mut spine = vec![pts[0]];
    let mut walked = 0.0_f32;
    for w in pts.windows(2) {
        let seg = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        if walked + seg >= trim - 1.0e-4 {
            break;
        }
        walked += seg;
        spine.push(w[1]);
    }
    spine.push((cx, cz));
    RoadEnd {
        hub,
        node: chain.end_nodes[slot],
        cx,
        cz,
        dx: hx / len,
        dz: hz / len,
        half_w: chain.half_w,
        deck_y: heights[0],
        skirt_y: heights[1],
        deck_normal,
        spine,
    }
}

/// The mouths a chain's sampled frames open into hubs with, before any deck
/// height is resolved (heights 0, shading straight up) - what the levelling
/// reads the hubs' outlines from (#1558).
pub(crate) fn sample_mouths(chain: &Chain, sample: &ChainSample, ends: ChainEnds) -> Vec<RoadEnd> {
    let f = &sample.frames;
    let last = f.len() - 1;
    (0..2)
        .filter_map(|slot| {
            let hub = ends.hub[slot]?;
            let (a, b) = if slot == 0 {
                (&f[0], &f[1])
            } else {
                (&f[last], &f[last - 1])
            };
            Some(mouth(
                chain,
                slot,
                hub,
                ends.trim[slot],
                [(a.cx, a.cz), (b.cx, b.cz)],
                [0.0, 0.0],
                UP,
            ))
        })
        .collect()
}

/// Cap a degree-1 dead-end's open cross-section (#579): a flat end wall filling
/// the profile's world points `pts`, every normal the (horizontal) outward
/// `outward` and each triangle wound to face it. UVs project the profile's
/// (lateral, height) so the cap textures continuously with the curb/skirt it
/// closes. Routed to `structure`.
///
/// The profile is CONCAVE (the deck dips between the two raised curbs), so it is
/// triangulated EXPLICITLY by its convex sub-regions - the skirt **body**
/// rectangle (full width, deck level down to the skirt floor) plus the two
/// **curb** wedges above deck level. A single fan from any centreline apex cannot
/// tile this: the vertical curb inner faces are back-facing from the centreline,
/// so fan triangles spill past the silhouette (review wf_aabe1626).
fn push_end_cap(
    parts: &mut RoadParts,
    pts: &[[f32; 3]; 10],
    prof: &[(f32, f32); 10],
    outward: [f32; 3],
) {
    let g = &mut parts.structure;
    let base = g.vertices.len() as u32;
    for (i, p) in pts.iter().enumerate() {
        g.vertices.push(*p);
        g.normals.push(outward);
        g.uvs.push([prof[i].0 / UV_TILE_M, prof[i].1 / UV_TILE_M]);
    }
    // Profile indices (see [`profile`]): 0/1 deck edges, 2/3 & 8/9 curb tops,
    // 4/7 chamfer bases, 5/6 skirt floor.
    const TRIS: [[usize; 3]; 6] = [
        [7, 4, 5],
        [7, 5, 6], // body rectangle: chamfer bases → skirt floor (full width)
        [1, 2, 3],
        [1, 3, 4], // right curb wedge
        [7, 8, 9],
        [7, 9, 0], // left curb wedge
    ];
    for t in TRIS {
        let geo = cross(sub3(pts[t[1]], pts[t[0]]), sub3(pts[t[2]], pts[t[0]]));
        let (i0, i1, i2) = (base + t[0] as u32, base + t[1] as u32, base + t[2] as u32);
        if dot(geo, outward) >= 0.0 {
            g.indices.extend_from_slice(&[i0, i1, i2]);
        } else {
            g.indices.extend_from_slice(&[i0, i2, i1]);
        }
    }
}

#[cfg(test)]
mod tests;
