use bevy_symbios_ground::HeightMap;

use crate::urban::math::{cross, dot, norm2, normalize, sub3, tri_up_normal};
use crate::urban::{Dims, ROAD_DEPTH_BIAS_M, RoadParts, UV_TILE_M, quad_normal};

/// One ribbon end abutting a junction node, recorded during chain extrusion so
/// the hub can meet each incident road at its exact mouth corners and deck
/// height (seamless, upward-only). All positions are in the sub-heightmap frame.
pub(crate) struct RoadEnd {
    pub(crate) node: usize,
    /// Truncated mouth centre (XZ): where the ribbon actually ends after #575.
    /// The hub fans from the centroid of all its mouth corners, so neither the
    /// node position nor the arm direction is needed here.
    pub(crate) cx: f32,
    pub(crate) cz: f32,
    /// Mouth-frame right axis and deck half-width — give the two mouth corners
    /// `(cx, cz) ± (rx, rz)·half_w`, which coincide with the ribbon's end edge.
    pub(crate) rx: f32,
    pub(crate) rz: f32,
    pub(crate) half_w: f32,
    pub(crate) deck_y: f32,
    /// The ribbon's skirt-bottom height at this mouth (`Frame::skirt_bottom_y`),
    /// so the hub fillet's skirt foot can drop to the *same* depth and weld to the
    /// ribbon skirt exactly — at any skirt depth or cross-slope, not just the deep
    /// default — leaving no open band at the seam.
    pub(crate) skirt_y: f32,
}

/// Curb-return fillet radius as a multiple of the deck half-width. Real curb
/// returns run ~1–1.5× a lane half-width (Minneapolis/AASHTO); this sets how
/// hard a hub corner rounds between two adjacent roads.
const CURB_RETURN_FACTOR: f32 = 1.5;
/// Cap on a fillet's outward bulge (sagitta) as a multiple of half-width, so a
/// wide gap can't balloon the corner apron well past the curb line.
const CURB_RETURN_MAX_SAG_FACTOR: f32 = 0.8;
/// Above this |cos| between two adjacent arms' headings the two are collinear —
/// a through road's two halves (anti-parallel) or an acute fork (parallel).
/// The through road's far edge stays a straight curb (sagitta 0); the acute
/// fork grows a smooth merge crotch instead (#578/#894), told apart by which
/// side of the hub the two mouths sit on (headings carry arbitrary sign, so
/// the dot's sign can't be trusted).
const FILLET_STRAIGHT_COS: f32 = 0.95;
/// Cap on the acute-merge crotch depth as a multiple of the deck half-width
/// (#894) — the ideal crotch (`gap/2 ÷ tan(θ/2)`) diverges as the fork
/// closes, and an unbounded teardrop would spear far down the roads.
const ACUTE_MERGE_MAX_FACTOR: f32 = 2.5;
/// Arc segments per curb-return fillet — sampled finely enough to read as a
/// smooth curve after the along-arc normal averaging.
const FILLET_SEG: usize = 6;

/// Build a real intersection hub at every junction (≥3 incident roads) from the
/// truncated ribbon ends (#576): a deck polygon whose mouth edges coincide with
/// each road's end cross-section (the deck flows in seamlessly at the road's own
/// height), its surface FLAT at the **max** incident mouth height — the #584
/// network levelling pins every incident road up to that one height, so the fan
/// is level, not domed (kept upward-only). Plus **curb-return arc fillets** (#577)
/// close the angular gaps: each corner between two adjacent roads rounds with an arc
/// joining their outer curbs, the curb profile swept along it (continuous with
/// the incident ribbon curbs) and a skirt dropping to the incident ribbons'
/// fixed depth. Smooth-shaded; every deck triangle wound front-up.
pub(crate) fn extrude_hubs(
    road_ends: &[RoadEnd],
    hm: &HeightMap,
    world_offset: [f32; 2],
    dims: &Dims,
    parts: &mut RoadParts,
) {
    use std::collections::BTreeMap;
    let mut by_node: BTreeMap<usize, Vec<&RoadEnd>> = BTreeMap::new();
    for e in road_ends {
        by_node.entry(e.node).or_default().push(e);
    }

    let uv = |q: [f32; 3]| [q[0] / UV_TILE_M, q[2] / UV_TILE_M];

    for (_node, arms) in by_node {
        if arms.len() < 3 {
            continue; // a real junction has ≥3 incident roads
        }

        // Mouth corners (world), two per arm at the road's own deck height so the
        // hub meets every ribbon seamlessly. Each is tagged with the arm it
        // belongs to, so a polygon edge *within* one arm is a mouth (left open for
        // the road) and an edge *between* arms is an exterior gap (gets a wall).
        let mut corners: Vec<([f32; 3], usize)> = Vec::with_capacity(arms.len() * 2);
        for (ai, a) in arms.iter().enumerate() {
            corners.push((
                [
                    a.cx - a.rx * a.half_w + world_offset[0],
                    a.deck_y,
                    a.cz - a.rz * a.half_w + world_offset[1],
                ],
                ai,
            ));
            corners.push((
                [
                    a.cx + a.rx * a.half_w + world_offset[0],
                    a.deck_y,
                    a.cz + a.rz * a.half_w + world_offset[1],
                ],
                ai,
            ));
        }

        // Fan centre = the mouth corners' centroid (always inside their hull). A
        // node-anchored fan over arm-grouped corners self-intersects whenever the
        // per-arm truncations differ and the deck half-width is comparable to the
        // pull-back (the common case) — adjacent mouths splay past each other.
        // Sweeping the corners by angle around the centroid and fanning from it
        // tiles a SIMPLE polygon regardless. Apex at the MAX incident deck height
        // (#584): the network levelling pins every incident mouth UP to that same
        // height, so apex == every corner → a genuinely FLAT junction plane (the
        // upward-only terrain clamp is a defensive floor that the pins already meet).
        // If the relaxation capped out under-pinned, the corners stay at their own
        // mouths (seamless) and the apex at the max keeps the fan from drooping.
        let (cx, cz) = (
            corners.iter().map(|(q, _)| q[0]).sum::<f32>() / corners.len() as f32,
            corners.iter().map(|(q, _)| q[2]).sum::<f32>() / corners.len() as f32,
        );
        let max_y = arms.iter().map(|a| a.deck_y).fold(f32::MIN, f32::max);
        let center_y = max_y
            .max(hm.get_height_at(cx - world_offset[0], cz - world_offset[1]) + ROAD_DEPTH_BIAS_M);
        let center = [cx, center_y, cz];

        // Angular sweep around the centroid → a simple polygon however the mouths
        // splay; the radius tiebreak keeps coincident-angle corners deterministic.
        corners.sort_by(|(q, _), (r, _)| {
            let aq = (q[2] - cz).atan2(q[0] - cx);
            let ar = (r[2] - cz).atan2(r[0] - cx);
            aq.partial_cmp(&ar)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    (q[0] - cx)
                        .hypot(q[2] - cz)
                        .partial_cmp(&(r[0] - cx).hypot(r[2] - cz))
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
        let p = corners.len();

        // --- Deck: a triangle fan from the levelled centroid, smooth-shaded. ---
        let mut vn = vec![[0.0_f32; 3]; p + 1]; // [0] = centre, [1+i] = corner i
        for i in 0..p {
            let f = tri_up_normal(center, corners[i].0, corners[(i + 1) % p].0);
            for idx in [0, 1 + i, 1 + (i + 1) % p] {
                vn[idx] = [vn[idx][0] + f[0], vn[idx][1] + f[1], vn[idx][2] + f[2]];
            }
        }
        let base = parts.deck.vertices.len() as u32;
        parts.deck.vertices.push(center);
        parts.deck.normals.push(normalize(vn[0]));
        parts.deck.uvs.push(uv(center));
        for (i, (c, _)) in corners.iter().enumerate() {
            parts.deck.vertices.push(*c);
            parts.deck.normals.push(normalize(vn[1 + i]));
            parts.deck.uvs.push(uv(*c));
        }
        for i in 0..p {
            let (a, b) = (1 + i as u32, 1 + ((i + 1) % p) as u32);
            // Wind every triangle front-up so back-face culling keeps it visible
            // from above regardless of the sweep's sense.
            let e1 = sub3(corners[i].0, center);
            let e2 = sub3(corners[(i + 1) % p].0, center);
            if cross(e1, e2)[1] >= 0.0 {
                parts
                    .deck
                    .indices
                    .extend_from_slice(&[base, base + a, base + b]);
            } else {
                parts
                    .deck
                    .indices
                    .extend_from_slice(&[base, base + b, base + a]);
            }
        }

        // --- Curb-return fillets (#577): a polygon edge between corners of
        //     DIFFERENT arms is an exterior corner → round it with a curb-return
        //     arc joining the two roads' outer curbs, the curb profile (curb wall
        //     + top + chamfer) swept along the arc and a skirt dropping to the
        //     ground. An edge within ONE arm is a mouth → left open for the road.
        //     The deck (#576) is left untouched: the curb's inner edge rides the
        //     deck's straight chord boundary, and only the curb top / chamfer /
        //     skirt round outward to the arc, so the deck stays the simple
        //     level-fit polygon. The arc starts/ends exactly on each ribbon's
        //     outer-curb point, so the curb is continuous across the seam. ---
        let (ct, cf, ch) = (dims.curb_top_width, dims.chamfer_width, dims.curb_height);
        for i in 0..p {
            let (l, la) = corners[i];
            let (r, ra) = corners[(i + 1) % p];
            if la == ra {
                continue; // a mouth edge: open for the road
            }
            let arm_l = arms[la];
            let arm_r = arms[ra];

            // Each gap corner's outward radial (unit, XZ) and its outer-curb point
            // — the deck-edge corner pushed out by curb_top + chamfer (= the outer
            // footprint `wo`), i.e. exactly where the incident ribbon's outer curb
            // ends — so the fillet arc joins one ribbon's outer curb to the next.
            let cl = [arm_l.cx + world_offset[0], arm_l.cz + world_offset[1]];
            let cr = [arm_r.cx + world_offset[0], arm_r.cz + world_offset[1]];
            let rad_l = norm2([l[0] - cl[0], l[2] - cl[1]]);
            let rad_r = norm2([r[0] - cr[0], r[2] - cr[1]]);
            let (wo_l, wo_r) = (arm_l.half_w + ct + cf, arm_r.half_w + ct + cf);
            let o_l = [cl[0] + rad_l[0] * wo_l, cl[1] + rad_l[1] * wo_l];
            let o_r = [cr[0] + rad_r[0] * wo_r, cr[1] + rad_r[1] * wo_r];
            let chord = (o_r[0] - o_l[0]).hypot(o_r[1] - o_l[1]);
            if chord < 1.0e-3 {
                continue; // coincident mouths — nothing to round
            }

            // Bulge outward (away from the hub centroid) by a sagitta derived from
            // a curb-return radius of ~CURB_RETURN_FACTOR · half_w, clamped gentle.
            // A near-straight gap (two anti-parallel arms — e.g. a through road's
            // far edge) keeps sagitta 0, so its curb stays a straight line.
            let half_w = (arm_l.half_w + arm_r.half_w) * 0.5;
            // Arm heading from the mouth-frame right (`(rz, −rx)` ⟂ right). Collinear
            // arms (a through road's two halves, or an acute fork) read as parallel
            // OR anti-parallel, so test |cosθ| — both keep the gap a straight curb
            // (a through road's far edge must not bump; acute forks are #578's job).
            let dir_l = [arm_l.rz, -arm_l.rx];
            let dir_r = [arm_r.rz, -arm_r.rx];
            let straight = fillet_gap_is_straight(dir_l, dir_r);
            let bd = norm2([
                (l[0] + r[0]) * 0.5 - center[0],
                (l[2] + r[2]) * 0.5 - center[2],
            ]);
            let radius = CURB_RETURN_FACTOR * half_w;
            let h = chord * 0.5;
            let arc = if straight {
                // Same-side mouths ⇒ an acute fork: grow the smooth-merge
                // crotch (#894) — a teardrop whose apex sits where the two
                // outer curbs would meet, capped so a razor-thin fork can't
                // spear off down the roads. Opposite-side mouths ⇒ a through
                // road's far edge: keep the straight curb.
                let same_side = (cl[0] - center[0]) * (cr[0] - center[0])
                    + (cl[1] - center[2]) * (cr[1] - center[2])
                    > 0.0;
                if same_side {
                    let cos_abs = (dir_l[0] * dir_r[0] + dir_l[1] * dir_r[1]).abs().min(1.0);
                    let tan_half = ((1.0 - cos_abs) / (1.0 + cos_abs)).sqrt().max(0.02);
                    let depth = (h / tan_half).min(ACUTE_MERGE_MAX_FACTOR * half_w);
                    merge_bezier(o_l, o_r, bd, depth, FILLET_SEG)
                } else {
                    fillet_arc(o_l, o_r, bd, 0.0, FILLET_SEG)
                }
            } else {
                let s = if radius > h {
                    radius - (radius * radius - h * h).sqrt()
                } else {
                    h
                };
                let sag = s.min(CURB_RETURN_MAX_SAG_FACTOR * half_w).min(0.45 * chord);
                fillet_arc(o_l, o_r, bd, sag, FILLET_SEG)
            };

            // Per-arc-sample curb cross-section, the inner edge riding the deck's
            // straight chord (deck untouched) and the chamfer/skirt rounding out to
            // the arc. P1 curb inner bottom .. P5 skirt foot.
            let n = arc.len();
            let mut p1: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p2: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p3: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p4: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p5: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut vlen: Vec<f32> = Vec::with_capacity(n);
            let mut acc = 0.0_f32;
            for (k, op) in arc.iter().enumerate() {
                let t = k as f32 / (n - 1) as f32;
                // Deck-level height interpolated between the two mouths — matching
                // the deck triangle edge l→r, which runs deck_y_l → deck_y_r.
                let dy = l[1] + (r[1] - l[1]) * t;
                let inner = [l[0] + (r[0] - l[0]) * t, dy, l[2] + (r[2] - l[2]) * t];
                let outer = [op[0], dy, op[1]];
                // Across-curb direction: from the deck-chord edge out to the arc.
                let rad = norm2([outer[0] - inner[0], outer[2] - inner[2]]);
                // Skirt foot: weld to the incident ribbons' skirt bottoms by
                // interpolating each arm's recorded `skirt_y` across the arc, so at
                // the two ends the foot equals the ribbon skirt exactly (at ANY
                // skirt depth or cross-slope, no open band at the seam) and tracks
                // the fixed-depth underside between them. Like the ribbon, it no
                // longer reaches down to the terrain — a high junction floats clear
                // as a bridge rather than filling the dip beneath it.
                let fy = arm_l.skirt_y + (arm_r.skirt_y - arm_l.skirt_y) * t;
                if k > 0 {
                    acc += (outer[0] - p4[k - 1][0]).hypot(outer[2] - p4[k - 1][2]);
                }
                p1.push(inner);
                p2.push([inner[0], inner[1] + ch, inner[2]]);
                p3.push([
                    inner[0] + rad[0] * ct,
                    inner[1] + ch,
                    inner[2] + rad[1] * ct,
                ]);
                p4.push(outer);
                p5.push([outer[0], fy, outer[2]]);
                vlen.push(acc / UV_TILE_M);
            }

            // One smoothed strip per profile face (smooth along the arc, hard
            // crease across — the WS2 idea), each wound so its front side faces out.
            let (u1, u2) = (ch / UV_TILE_M, (ch + ct) / UV_TILE_M);
            let u3 = (ch + ct + cf) / UV_TILE_M;
            let u4 = u3 + dims.skirt_depth / UV_TILE_M;
            push_fillet_face(parts, center, &p1, &p2, (0.0, u1), &vlen); // curb wall
            push_fillet_face(parts, center, &p2, &p3, (u1, u2), &vlen); // curb top
            push_fillet_face(parts, center, &p3, &p4, (u2, u3), &vlen); // chamfer
            push_fillet_face(parts, center, &p4, &p5, (u3, u4), &vlen); // skirt
        }
    }
}

/// Whether two adjacent hub arms are collinear — a through road's two halves
/// (anti-parallel) or an acute fork (parallel) — in which case the gap between
/// them is a near-straight curb that must NOT bulge (the fillet keeps sagitta 0).
/// Orientation-independent (tests |cosθ| of the unit headings), so it holds
/// whichever way each arm's recorded heading happens to point.
pub(crate) fn fillet_gap_is_straight(dir_l: [f32; 2], dir_r: [f32; 2]) -> bool {
    (dir_l[0] * dir_r[0] + dir_l[1] * dir_r[1]).abs() > FILLET_STRAIGHT_COS
}

/// Sample a quadratic Bézier from `a` to `b` whose control point sits `depth`
/// out along `bd` from the chord midpoint — the acute-fork merge crotch
/// (#894). Endpoints are exact (they must coincide with the incident
/// ribbons' outer-curb points); the apex reaches `depth/2` at `t = 0.5`.
/// Deterministic.
pub(crate) fn merge_bezier(
    a: [f32; 2],
    b: [f32; 2],
    bd: [f32; 2],
    depth: f32,
    segs: usize,
) -> Vec<[f32; 2]> {
    let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    let ctrl = [mid[0] + bd[0] * depth, mid[1] + bd[1] * depth];
    let mut pts = Vec::with_capacity(segs + 1);
    for i in 0..=segs.max(1) {
        let t = i as f32 / segs.max(1) as f32;
        let u = 1.0 - t;
        pts.push([
            u * u * a[0] + 2.0 * u * t * ctrl[0] + t * t * b[0],
            u * u * a[1] + 2.0 * u * t * ctrl[1] + t * t * b[1],
        ]);
    }
    // Endpoint exactness against float drift.
    *pts.first_mut().expect("segs+1 ≥ 2") = a;
    *pts.last_mut().expect("segs+1 ≥ 2") = b;
    pts
}

/// Sample a circular curb-return arc from `a` to `b` (XZ) bulging by sagitta
/// `sag` toward the outward direction `bd`. Returns `segs + 1` points, the first
/// exactly `a` and the last exactly `b` (force-assigned, so they always coincide
/// with the incident ribbon's curb point regardless of `bd`). A non-positive
/// sagitta (or a degenerate chord) returns the straight chord, so a near-collinear
/// gap stays flat. Deterministic; no `Date`/random.
pub(crate) fn fillet_arc(
    a: [f32; 2],
    b: [f32; 2],
    bd: [f32; 2],
    sag: f32,
    segs: usize,
) -> Vec<[f32; 2]> {
    let lerp = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let chord = [b[0] - a[0], b[1] - a[1]];
    let clen = chord[0].hypot(chord[1]);
    let half = clen * 0.5;
    if segs == 0 || half < 1.0e-4 || sag < 1.0e-4 {
        return (0..=segs)
            .map(|i| lerp(i as f32 / segs.max(1) as f32))
            .collect();
    }
    let r = (sag * sag + half * half) / (2.0 * sag);
    let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    // Place the centre on the chord's OWN perpendicular bisector — then |a−c| =
    // |b−c| = r exactly, so both endpoints land on the circle whatever `bd` is.
    // `bd` only picks which side the arc bulges (the corner side).
    let perp = [-chord[1] / clen, chord[0] / clen];
    let pn = if perp[0] * bd[0] + perp[1] * bd[1] >= 0.0 {
        perp
    } else {
        [-perp[0], -perp[1]]
    };
    let center = [mid[0] - pn[0] * (r - sag), mid[1] - pn[1] * (r - sag)];
    let ang = |p: [f32; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
    let (pi, tau, frac) = (
        std::f32::consts::PI,
        std::f32::consts::TAU,
        std::f32::consts::FRAC_PI_2,
    );
    let a0 = ang(a);
    let mut d = ang(b) - a0;
    while d > pi {
        d -= tau;
    }
    while d <= -pi {
        d += tau;
    }
    // Keep the sweep on the bulge (apex) side: the apex bears along `pn`. If the
    // short sweep's midpoint faces away from it, take the complementary arc.
    let apex = pn[1].atan2(pn[0]);
    let mut diff = (a0 + d * 0.5) - apex;
    while diff > pi {
        diff -= tau;
    }
    while diff <= -pi {
        diff += tau;
    }
    if diff.abs() > frac {
        d += if d > 0.0 { -tau } else { tau };
    }
    let mut pts: Vec<[f32; 2]> = (0..=segs)
        .map(|i| {
            let th = a0 + d * (i as f32 / segs as f32);
            [center[0] + r * th.cos(), center[1] + r * th.sin()]
        })
        .collect();
    // Pin the endpoints exactly (kill float residue) so the seam is watertight.
    pts[0] = a;
    pts[segs] = b;
    pts
}

/// Push one curb-return-fillet face strip into `parts.structure`: `inner`/`outer`
/// are the face's two edges at each arc sample, smooth-shaded ALONG the arc (welded
/// vertices carrying averaged segment normals) with a hard crease ACROSS the
/// profile (one strip per face). Each segment is wound INDIVIDUALLY so its front
/// face matches that segment's own outward normal — a single per-strip decision is
/// wrong on a curved or height-sloped strip (where the geometric facing flips
/// partway), which would back-wind some triangles and mis-shade them under the
/// road's double-sided material.
fn push_fillet_face(
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
    // Per-segment outward normals (oriented away from the hub centre), then the
    // welded per-vertex normal = average of the (up to two) segments meeting at i,
    // so the strip shades smoothly along the arc.
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
    // Emit each TRIANGLE wound so its geometric front matches its own averaged
    // shading normal. Per-triangle (not per-strip, nor even per-quad) is required:
    // a fillet quad is generally warped (the inner edge is a straight chord, the
    // outer edge a curved arc at varying height), so its two triangles can face
    // opposite ways — a single decision back-winds one of them and mis-shades it.
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
