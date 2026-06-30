use crate::urban::math::{cross, dot, normalize, sub3};
use crate::urban::{Chain, ChainSample, Dims, RoadEnd, RoadParts};

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
/// flat deck height — lifted to clear the highest terrain under the road and
/// longitudinally grade-limited — and `skirt_bottom_y` is a FIXED `skirt_depth`
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
fn beam_axis(f0: &Frame, f1: &Frame, world_offset: f32) -> [f32; 3] {
    [
        (f0.cx + f1.cx) * 0.5 + world_offset,
        (f0.base_y + f1.base_y + f0.skirt_bottom_y + f1.skirt_bottom_y) * 0.25,
        (f0.cz + f1.cz) * 0.5 + world_offset,
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
    let mut nrm = cross(e1, e2);
    let fc = [
        (a[0] + b[0] + c[0] + d[0]) * 0.25,
        (a[1] + b[1] + c[1] + d[1]) * 0.25,
        (a[2] + b[2] + c[2] + d[2]) * 0.25,
    ];
    let outward = [fc[0] - axis[0], fc[1] - axis[1], fc[2] - axis[2]];
    if dot(nrm, outward) < 0.0 {
        nrm = [-nrm[0], -nrm[1], -nrm[2]];
    }
    normalize(nrm)
}

/// Extrude the curb/skirt profile along one chain into `parts`. The deck drapes
/// over the terrain **flat-across and upward-only** (it never sinks below the
/// terrain — see [`Frame`]), shifted into the full-terrain frame by `world_offset`.
/// The drivable deck top, the structural curb/skirt and the emissive neon
/// edge-lines are routed to their respective [`RoadParts`] buffers.
/// `sample` is the chain's terrain-sampled frames ([`crate::urban::sample_chain`]) and `base_y`
/// the resolved per-frame deck height ([`crate::urban::level_chain`], with junction pins folded
/// in by the network pass) — both supplied by the caller so the heightmap is
/// sampled exactly once and the pre-pass and mesh agree to the bit (#584).
#[allow(clippy::too_many_arguments)] // each arg is a distinct input/sink.
pub(crate) fn extrude_ribbon(
    chain: &Chain,
    sample: &ChainSample,
    base_y: &[f32],
    world_offset: f32,
    dims: &Dims,
    degree: &[u32],
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
            // The skirt drops a FIXED `skirt_depth` below the deck — it no longer
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

    // Record this chain's ends that abut a junction (degree ≥ 3) so the hub
    // builder can meet each road at its exact deck mouth and height.
    let last = frames.len() - 1;
    for (slot, &nd) in chain.end_nodes.iter().enumerate() {
        if degree.get(nd).copied().unwrap_or(0) < 3 {
            continue;
        }
        let f = &frames[if slot == 0 { 0 } else { last }];
        road_ends.push(RoadEnd {
            node: nd,
            cx: f.cx,
            cz: f.cz,
            rx: f.rx,
            rz: f.rz,
            half_w,
            deck_y: f.base_y,
            skirt_y: f.skirt_bottom_y,
        });
    }

    // Cumulative cross-section perimeter, for the U coordinate.
    let mut u = [0.0_f32; 10];
    for j in 1..10 {
        let (a, b) = (prof[j - 1], prof[j]);
        u[j] = u[j - 1] + (b.0 - a.0).hypot(b.1 - a.1);
    }

    // World position of profile point `pi` at frame `f`: flat deck (no lateral
    // banking); the skirt-bottom points (5, 6) drop to `skirt_bottom_y`.
    let world = |f: &Frame, pi: usize| {
        let (pu, ph) = prof[pi];
        let lateral = pu * f.scale;
        let y = if pi == 5 || pi == 6 {
            f.skirt_bottom_y
        } else {
            f.base_y + ph
        };
        [
            f.cx + f.rx * lateral + world_offset,
            y,
            f.cz + f.rz * lateral + world_offset,
        ]
    };

    // Per-frame along-road V, shared by every profile face.
    let v: Vec<f32> = frames.iter().map(|f| f.arc / UV_TILE_M).collect();

    for j in 0..10 {
        let k = (j + 1) % 10;
        let (uj, uk) = (u[j] / UV_TILE_M, u[k] / UV_TILE_M);
        // Profile face 0→1 is the flat drivable deck top; every other face is
        // structural (curb walls, chamfers, the deep skirt and its bottom cap).
        let target = if j == 0 {
            &mut parts.deck
        } else {
            &mut parts.structure
        };
        // One strip per face: normals are averaged ALONG the chain (smooth
        // ribbon) but each face is its own strip, so the crease ACROSS the
        // profile stays sharp.
        let left: Vec<[f32; 3]> = frames.iter().map(|f| world(f, j)).collect();
        let right: Vec<[f32; 3]> = frames.iter().map(|f| world(f, k)).collect();
        let mut seg_n = Vec::with_capacity(frames.len().saturating_sub(1));
        for i in 0..frames.len() - 1 {
            let axis = beam_axis(&frames[i], &frames[i + 1], world_offset);
            seg_n.push(quad_normal(
                left[i],
                right[i],
                left[i + 1],
                right[i + 1],
                axis,
            ));
        }
        target.push_smoothed_strip(&left, &right, &seg_n, (uj, uk), &v);
    }

    // Emissive neon edge-line: a thin strip riding proud of each curb's inner top
    // crease (lateral ±half_w, just above the curb top), lifted clear so it never
    // z-fights the curb. Kept on its own surface for the hot emissive material.
    let lift = dims.curb_height + NEON_LINE_LIFT_M;
    let neon_at = |f: &Frame, lu: f32| {
        [
            f.cx + f.rx * (lu * f.scale) + world_offset,
            f.base_y + lift,
            f.cz + f.rz * (lu * f.scale) + world_offset,
        ]
    };
    for (u0, u1) in [
        (half_w, half_w + NEON_LINE_WIDTH_M),
        (-half_w, -half_w - NEON_LINE_WIDTH_M),
    ] {
        for i in 0..frames.len() - 1 {
            let (f0, f1) = (&frames[i], &frames[i + 1]);
            let (a, b) = (neon_at(f0, u0), neon_at(f0, u1));
            let (c, d) = (neon_at(f1, u0), neon_at(f1, u1));
            let nrm = quad_normal(a, b, c, d, beam_axis(f0, f1, world_offset));
            let (vi, vi1) = (f0.arc / UV_TILE_M, f1.arc / UV_TILE_M);
            parts.neon.push_quad(
                a,
                b,
                c,
                d,
                [[0.0, vi], [1.0, vi], [0.0, vi1], [1.0, vi1]],
                nrm,
            );
        }
    }

    // End caps: an open chain end leaves the extruded cross-section open — a
    // visible hollow tube into the road's underside. Close it with a flat
    // cross-section cap facing outward (away from the ribbon). Two ends need it:
    // a degree-1 dead-end / cul-de-sac (#579), and a district-edge clip running
    // off the network perimeter (#582, `chain.clip[slot]`). Junctions (degree ≥ 3)
    // are closed by their hub; a loop closure / used-edge break stays open.
    for (slot, &nd) in chain.end_nodes.iter().enumerate() {
        let is_dead_end = degree.get(nd).copied().unwrap_or(0) == 1;
        if !is_dead_end && !chain.clip[slot] {
            continue;
        }
        let (fe, fi) = if slot == 0 {
            (&frames[0], &frames[1.min(last)])
        } else {
            (&frames[last], &frames[last.saturating_sub(1)])
        };
        // The cap is the (vertical) end cross-section, so its true normal is the
        // HORIZONTAL lateral-perp `(rx,rz)⊥` — independent of the deck/skirt grade
        // — oriented away from the ribbon. Using the road tangent would tilt the
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
        let pts: [[f32; 3]; 10] = std::array::from_fn(|pi| world(fe, pi));
        push_end_cap(parts, &pts, &prof, outward);
    }
}

/// Cap a degree-1 dead-end's open cross-section (#579): a flat end wall filling
/// the profile's world points `pts`, every normal the (horizontal) outward
/// `outward` and each triangle wound to face it. UVs project the profile's
/// (lateral, height) so the cap textures continuously with the curb/skirt it
/// closes. Routed to `structure`.
///
/// The profile is CONCAVE (the deck dips between the two raised curbs), so it is
/// triangulated EXPLICITLY by its convex sub-regions — the skirt **body**
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
