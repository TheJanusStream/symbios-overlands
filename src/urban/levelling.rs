use bevy_symbios_ground::HeightMap;

use crate::urban::{
    Chain, Dims, RIBBON_STEP_M, ROAD_DEPTH_BIAS_M, densify, frame_right, trim_polyline,
};

/// Lateral samples across the deck width for the upward-only height: the flat
/// deck is lifted to clear the MAX of these, so no part of the drivable surface
/// ever buries (the uphill edge sits flush, the downhill edge rides proud).
const DECK_SAMPLES: usize = 5;
/// Cap (rise/run) on the deck's *downhill* drop between frames — keeps the grade
/// gentle and lets the deck bridge dips as an embankment instead of diving in.
/// Up/down inclines are tolerable; only the lateral roll is engineered out.
pub(crate) const MAX_LONGITUDINAL_GRADE: f32 = 0.18;
/// Gentler grade (rise/run) for a junction APPROACH ramp (#584): when a road is
/// pinned up to meet a flat junction, it ramps back to its natural draped height
/// over `rise / this` metres — long enough to read as a smooth transition, not a
/// short kick right at the intersection. Separate from (and below) the global
/// drainage grade so the rest of the road is unaffected.
pub(crate) const JUNCTION_APPROACH_GRADE: f32 = 0.09;
/// Safety cap on the junction-levelling relaxation passes (#584). Each pass raises
/// junction heights to the max incident mouth and re-ramps; heights only ever rise
/// and are bounded by the highest terrain floor, so it converges. One pass
/// propagates a height change across one chain, so a connected run of N junctions
/// needs ~N passes — set well above the longest junction-path real networks reach.
const MAX_LEVEL_ITERS: usize = 64;
/// Junction-levelling has converged once no junction height moved more than this
/// (m) in a pass (#584).
const LEVEL_CONVERGE_EPS_M: f32 = 1.0e-3;
/// How far (m) the skirt bottom sinks below the lower outer-edge terrain, so an
/// elevated (downhill) side always reads as a retaining wall meeting the ground.
pub(crate) const SKIRT_BURY_MARGIN_M: f32 = 0.3;

/// One frame's terrain-sampled geometry, independent of the final deck height —
/// the heightmap-sampling output of [`sample_chain`], reused by both the levelling
/// pre-pass and the mesh pass so the deck floor is bit-identical between them
/// (#584; the float-drift seam closes by having ONE sampling site).
pub(crate) struct RawFrame {
    pub(crate) cx: f32,
    pub(crate) cz: f32,
    pub(crate) rx: f32,
    pub(crate) rz: f32,
    pub(crate) scale: f32,
    pub(crate) arc: f32,
    /// Upward-only deck floor: max terrain across the deck width + the depth bias.
    pub(crate) floor: f32,
    /// Lowest outer-edge terrain, for the skirt drop on an elevated side.
    pub(crate) ground: f32,
}

/// A chain's trimmed, densified, terrain-sampled frames plus inter-frame segment
/// lengths — everything the deck height and mesh need that does NOT depend on the
/// chosen deck height. Sampled ONCE per chain (the only heightmap-sampling site).
pub(crate) struct ChainSample {
    pub(crate) frames: Vec<RawFrame>,
    pub(crate) seg: Vec<f32>,
}

/// Trim a chain at its junction ends (#575), densify it, and sample the terrain
/// floor / outer ground per frame (Pass A) — `None` if nothing meshable survives.
/// The single heightmap-sampling site for a chain (#584).
pub(crate) fn sample_chain(
    chain: &Chain,
    start_trim: f32,
    end_trim: f32,
    hm: &HeightMap,
    dims: &Dims,
) -> Option<ChainSample> {
    let trimmed = trim_polyline(&chain.pts, start_trim, end_trim);
    let pts = densify(&trimmed, RIBBON_STEP_M);
    if pts.len() < 2 {
        return None;
    }
    let half_w = chain.half_w;
    let wo = half_w + dims.curb_top_width + dims.chamfer_width;
    let mut frames: Vec<RawFrame> = Vec::with_capacity(pts.len());
    let mut arc = 0.0;
    for i in 0..pts.len() {
        let (cx, cz) = pts[i];
        if i > 0 {
            arc += (cx - pts[i - 1].0).hypot(cz - pts[i - 1].1);
        }
        let (rx, rz, scale) = frame_right(&pts, i);
        let mut maxh = f32::MIN;
        for s in 0..DECK_SAMPLES {
            let t = s as f32 / (DECK_SAMPLES - 1) as f32;
            let off = (-half_w + 2.0 * half_w * t) * scale;
            maxh = maxh.max(hm.get_height_at(cx + rx * off, cz + rz * off));
        }
        let g_r = hm.get_height_at(cx + rx * wo * scale, cz + rz * wo * scale);
        let g_l = hm.get_height_at(cx - rx * wo * scale, cz - rz * wo * scale);
        frames.push(RawFrame {
            cx,
            cz,
            rx,
            rz,
            scale,
            arc,
            floor: maxh + ROAD_DEPTH_BIAS_M,
            ground: g_r.min(g_l),
        });
    }
    let seg: Vec<f32> = frames
        .windows(2)
        .map(|w| (w[1].cx - w[0].cx).hypot(w[1].cz - w[0].cz).max(1.0e-3))
        .collect();
    Some(ChainSample { frames, seg })
}

/// Resolve the deck base height per frame from the terrain `floor` (#573/#584).
/// Two parts, each an upward-only lower bound (the deck only ever rises — it never
/// buries): first the longitudinal grade-limit at [`MAX_LONGITUDINAL_GRADE`] (a
/// gentle drainage grade that bridges dips), then — for each junction end carrying
/// a height `pin` — a ramp cone down from that pinned height at the gentler
/// [`JUNCTION_APPROACH_GRADE`], so a road pinned up to a flat junction ramps back
/// to its natural height over enough of its length to read smooth. With both pins
/// `None` this is exactly the #573 two-pass levelling (a strict refactor).
pub(crate) fn level_chain(floor: &[f32], seg: &[f32], pin: [Option<f32>; 2]) -> Vec<f32> {
    let mut base_y = floor.to_vec();
    for i in 1..base_y.len() {
        base_y[i] = base_y[i].max(base_y[i - 1] - MAX_LONGITUDINAL_GRADE * seg[i - 1]);
    }
    for i in (0..base_y.len().saturating_sub(1)).rev() {
        base_y[i] = base_y[i].max(base_y[i + 1] - MAX_LONGITUDINAL_GRADE * seg[i]);
    }
    // Junction-approach ramp cones: base_y[i] >= H - grade * arc-distance-to-the-
    // pinned end. Cones only RAISE the deck and ramp at a gentler grade than the
    // floor pass, so they never violate the longitudinal limit.
    if let Some(h) = pin[0] {
        let mut dist = 0.0;
        for i in 0..base_y.len() {
            base_y[i] = base_y[i].max(h - JUNCTION_APPROACH_GRADE * dist);
            if i + 1 < base_y.len() {
                dist += seg[i];
            }
        }
    }
    if let Some(h) = pin[1] {
        let mut dist = 0.0;
        for i in (0..base_y.len()).rev() {
            base_y[i] = base_y[i].max(h - JUNCTION_APPROACH_GRADE * dist);
            if i > 0 {
                dist += seg[i - 1];
            }
        }
    }
    base_y
}

/// Resolve a FLAT height per junction and the final deck height per chain across
/// the whole network (#584). Each junction is lifted to the max of (a) the terrain
/// under its mouth-centroid + the depth bias — so a junction on a local rise stays
/// flat by lifting its mouths to clear it rather than doming the hub — and (b) the
/// highest road mouth meeting it; every incident road is then ramped up to that
/// height by [`level_chain`]'s pin cones. A chain joins two junctions, and raising
/// one can raise the next, so junction heights are RELAXED to a monotone-upward
/// fixed point. Heights only ever rise and are bounded by the highest terrain
/// floor, so it converges; capped at [`MAX_LEVEL_ITERS`] (graceful degradation —
/// every chain still levels watertight, just under-pinned). Returns `base_y` per
/// chain (empty where the chain had no meshable sample).
pub(crate) fn level_network(
    chains: &[Chain],
    samples: &[Option<ChainSample>],
    degree: &[u32],
    hm: &HeightMap,
) -> Vec<Vec<f32>> {
    use std::collections::BTreeMap;
    let is_junction = |nd: usize| degree.get(nd).copied().unwrap_or(0) >= 3;
    let floor_of = |s: &ChainSample| -> Vec<f32> { s.frames.iter().map(|r| r.floor).collect() };

    // Seed each junction at the terrain under its incident mouths' centroid + bias.
    let mut centroid: BTreeMap<usize, (f32, f32, u32)> = BTreeMap::new();
    for (ci, chain) in chains.iter().enumerate() {
        let Some(s) = &samples[ci] else { continue };
        for (slot, &nd) in chain.end_nodes.iter().enumerate() {
            if !is_junction(nd) {
                continue;
            }
            let f = if slot == 0 {
                &s.frames[0]
            } else {
                s.frames.last().expect("sample has >= 2 frames")
            };
            let acc = centroid.entry(nd).or_insert((0.0, 0.0, 0));
            acc.0 += f.cx;
            acc.1 += f.cz;
            acc.2 += 1;
        }
    }
    let mut hub_h: BTreeMap<usize, f32> = BTreeMap::new();
    for (&nd, &(sx, sz, n)) in &centroid {
        let (cx, cz) = (sx / n as f32, sz / n as f32);
        hub_h.insert(nd, hm.get_height_at(cx, cz) + ROAD_DEPTH_BIAS_M);
    }

    let pin_for = |hub_h: &BTreeMap<usize, f32>, nd: usize| -> Option<f32> {
        if is_junction(nd) {
            hub_h.get(&nd).copied()
        } else {
            None
        }
    };
    let mut base_ys: Vec<Vec<f32>> = vec![Vec::new(); chains.len()];
    for _ in 0..MAX_LEVEL_ITERS {
        // Re-level every chain to its current junction pins.
        for (ci, chain) in chains.iter().enumerate() {
            let Some(s) = &samples[ci] else { continue };
            let pin = [
                pin_for(&hub_h, chain.end_nodes[0]),
                pin_for(&hub_h, chain.end_nodes[1]),
            ];
            base_ys[ci] = level_chain(&floor_of(s), &s.seg, pin);
        }
        // Lift each junction to the highest mouth now meeting it; track movement.
        let mut moved = 0.0_f32;
        for (ci, chain) in chains.iter().enumerate() {
            if base_ys[ci].is_empty() {
                continue;
            }
            for (slot, &nd) in chain.end_nodes.iter().enumerate() {
                if !is_junction(nd) {
                    continue;
                }
                let m = if slot == 0 {
                    base_ys[ci][0]
                } else {
                    base_ys[ci][base_ys[ci].len() - 1]
                };
                // Every junction reaching the lift was seeded above (same Some-sample
                // gate), so the entry exists; `or_insert(m)` is a sane fallback (the
                // mouth itself), never a garbage `f32::MIN`, if that ever changes.
                let h = hub_h.entry(nd).or_insert(m);
                if m > *h + LEVEL_CONVERGE_EPS_M {
                    moved = moved.max(m - *h);
                    *h = m;
                }
            }
        }
        if moved < LEVEL_CONVERGE_EPS_M {
            break;
        }
    }
    // Final re-level to the converged junction heights: the loop breaks just after a
    // lift, so without this the deck would lag the last (sub-eps) lift. This pins
    // every mouth EXACTLY to its junction's resolved height → spread is exactly 0.
    for (ci, chain) in chains.iter().enumerate() {
        let Some(s) = &samples[ci] else { continue };
        let pin = [
            pin_for(&hub_h, chain.end_nodes[0]),
            pin_for(&hub_h, chain.end_nodes[1]),
        ];
        base_ys[ci] = level_chain(&floor_of(s), &s.seg, pin);
    }
    base_ys
}

/// Per-junction incident-mouth height SPREAD (max − min over the roads meeting it)
/// for junctions with ≥ 2 meshed incident roads — 0 once the network levelling has
/// pinned every incident mouth to one height (#584 diagnostic).
pub(crate) fn junction_mouth_spreads(
    chains: &[Chain],
    base_ys: &[Vec<f32>],
    is_junction: &impl Fn(usize) -> bool,
) -> Vec<f32> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<usize, (f32, f32, u32)> = BTreeMap::new(); // (min, max, count)
    for (ci, chain) in chains.iter().enumerate() {
        if base_ys[ci].is_empty() {
            continue;
        }
        for (slot, &nd) in chain.end_nodes.iter().enumerate() {
            if !is_junction(nd) {
                continue;
            }
            let m = if slot == 0 {
                base_ys[ci][0]
            } else {
                base_ys[ci][base_ys[ci].len() - 1]
            };
            let e = acc.entry(nd).or_insert((f32::MAX, f32::MIN, 0));
            e.0 = e.0.min(m);
            e.1 = e.1.max(m);
            e.2 += 1;
        }
    }
    acc.values()
        .filter(|&&(_, _, n)| n >= 2)
        .map(|&(mn, mx, _)| mx - mn)
        .collect()
}

#[cfg(test)]
mod tests;
