//! Deck heights - the one place the heightmap is sampled, so nothing drifts.
//! Each chain is trimmed, its sharp bends rounded (#1567), densified and
//! sampled ONCE here (#584), and both the levelling pre-pass and the ribbon
//! mesher consume that cached sample, so the deck floor is bit-identical
//! between them and the mouths leave no seam.
//! Heights resolve upward-only - the deck rises to clear the terrain under it,
//! never sinks: a longitudinal grade limit that bridges dips (#573), then ramp
//! cones down from each junction's pinned height that shed its lift over the
//! road's own natural profile (#1737). Junctions are flat and lifting one can
//! lift the next, so the network relaxes to a monotone-upward fixed point.

use bevy_symbios_ground::HeightMap;

use crate::urban::bends::round_bends;
use crate::urban::truncation::JunctionPlan;
use crate::urban::{
    Chain, Dims, RIBBON_STEP_M, ROAD_DEPTH_BIAS_M, densify, frame_right, trim_polyline,
};

/// Lateral samples across the deck width for the upward-only height: the flat
/// deck is lifted to clear the MAX of these, so no part of the drivable surface
/// ever buries (the uphill edge sits flush, the downhill edge rides proud).
const DECK_SAMPLES: usize = 5;
/// Cap (rise/run) on the deck's *downhill* drop between frames - keeps the grade
/// gentle and lets the deck bridge dips as an embankment instead of diving in.
/// Up/down inclines are tolerable; only the lateral roll is engineered out.
pub(crate) const MAX_LONGITUDINAL_GRADE: f32 = 0.18;
/// Gentler grade (rise/run) for a junction APPROACH ramp (#584): when a road is
/// pinned up to meet a flat junction, it ramps back to its natural draped height
/// over `rise / this` metres - long enough to read as a smooth transition, not a
/// short kick right at the intersection - the rise shed over the road's own
/// profile, so a road falling away downhill comes back down too, as fast as the
/// longitudinal grade lets it (#1737, [`level_chain`]).
/// Separate from (and below) the global drainage grade so the rest of the road
/// is unaffected.
pub(crate) const JUNCTION_APPROACH_GRADE: f32 = 0.09;
/// Safety cap on the junction-levelling relaxation passes (#584). Each pass raises
/// junction heights to the max incident mouth and re-ramps; heights only ever rise
/// and are bounded by the highest terrain floor, so it converges. One pass
/// propagates a height change across one chain, so a connected run of N junctions
/// needs ~N passes - set well above the longest junction-path real networks reach.
const MAX_LEVEL_ITERS: usize = 64;
/// Junction-levelling has converged once no junction height moved more than this
/// (m) in a pass (#584).
const LEVEL_CONVERGE_EPS_M: f32 = 1.0e-3;

/// One frame's terrain-sampled geometry, independent of the final deck height -
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
}

/// A chain's trimmed, densified, terrain-sampled frames plus inter-frame segment
/// lengths - everything the deck height and mesh need that does NOT depend on the
/// chosen deck height. Sampled ONCE per chain (the only heightmap-sampling site).
pub(crate) struct ChainSample {
    pub(crate) frames: Vec<RawFrame>,
    pub(crate) seg: Vec<f32>,
}

/// Trim a chain at its junction ends (#575), round its sharp bends into arcs
/// of its outer half-width - deck, curb and chamfer - where its legs leave
/// room, so the ribbon does not fold there (#1567), densify it, and sample
/// the terrain floor per frame (Pass A) - `None` if nothing meshable
/// survives. The single heightmap-sampling site for a chain (#584).
pub(crate) fn sample_chain(
    chain: &Chain,
    start_trim: f32,
    end_trim: f32,
    hm: &HeightMap,
    dims: &Dims,
) -> Option<ChainSample> {
    let trimmed = trim_polyline(&chain.pts, start_trim, end_trim);
    let outer = chain.half_w + dims.curb_top_width + dims.chamfer_width;
    let pts = densify(&round_bends(&trimmed, outer), RIBBON_STEP_M);
    if pts.len() < 2 {
        return None;
    }
    let half_w = chain.half_w;
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
        frames.push(RawFrame {
            cx,
            cz,
            rx,
            rz,
            scale,
            arc,
            floor: maxh + ROAD_DEPTH_BIAS_M,
        });
    }
    let seg: Vec<f32> = frames
        .windows(2)
        .map(|w| (w[1].cx - w[0].cx).hypot(w[1].cz - w[0].cz).max(1.0e-3))
        .collect();
    Some(ChainSample { frames, seg })
}

/// Resolve the deck base height per frame from the terrain `floor` (#573/#584).
/// Two parts, each an upward-only lower bound (the deck only ever rises - it never
/// buries): first the longitudinal grade-limit at [`MAX_LONGITUDINAL_GRADE`] (a
/// gentle drainage grade that bridges dips), which is the road's natural
/// profile; then - for each junction end carrying a height `pin` - a ramp cone
/// down from that pinned height ([`approach_cone`]) that sheds the junction's
/// lift over the natural profile at up to the gentler
/// [`JUNCTION_APPROACH_GRADE`], so a road pinned up to a flat junction ramps
/// back to its natural height over enough of its length to read smooth, on a
/// slope as on the flat (#1737). A road falling away downhill would then descend
/// faster than the floor pass lets it, so the result is held to the
/// longitudinal grade once more: where the natural road falls at `s` the lift
/// sheds at `min(JUNCTION_APPROACH_GRADE, MAX_LONGITUDINAL_GRADE - s)` a metre -
/// slower as `s` nears the longitudinal grade, and not at all where the natural
/// road falls at it, so a lift there runs on undiminished (though, unlike
/// #584's ramp, it never grows). With both pins `None` this is exactly the #573
/// two-pass levelling (a strict refactor), and from each pinned end a road that
/// is flat or climbs comes out as #584 levelled it, to the bit.
pub(crate) fn level_chain(floor: &[f32], seg: &[f32], pin: [Option<f32>; 2]) -> Vec<f32> {
    let mut natural = floor.to_vec();
    limit_grade(&mut natural, seg);
    if natural.is_empty() || pin.iter().all(Option::is_none) {
        return natural;
    }
    let n = natural.len();
    let mut base_y = natural.clone();
    if let Some(h) = pin[0] {
        let mut dist = 0.0;
        for i in 0..n {
            base_y[i] = base_y[i].max(approach_cone(h, natural[0], natural[i], dist));
            if i + 1 < n {
                dist += seg[i];
            }
        }
    }
    if let Some(h) = pin[1] {
        let mut dist = 0.0;
        for i in (0..n).rev() {
            base_y[i] = base_y[i].max(approach_cone(h, natural[n - 1], natural[i], dist));
            if i > 0 {
                dist += seg[i - 1];
            }
        }
    }
    limit_grade(&mut base_y, seg);
    base_y
}

/// The least height a deck may stand `dist` metres along its road from a
/// junction end pinned at `pin`, where the road's natural profile stands at
/// `mouth` at that end and at `natural` here: the junction's lift over the
/// natural road (`pin - mouth`) shed at [`JUNCTION_APPROACH_GRADE`], never above
/// the pin's own ramp (`pin - grade * dist`, #584), which a flat or an uphill
/// road keeps exactly. That ramp alone held a downhill road at `pin - grade *
/// dist` all its length, so on ground falling faster than the grade it never
/// came back down, and its lifted mouth pinned the next junction downhill up
/// to it: the lift cascaded to the foot of a slope, and Isoline's streets ended
/// 3-4 m up at its shore (#1737). Written as the ramp less the natural road's
/// drop since the mouth, which is exactly zero on the flat; once the lift is
/// shed that falls under the natural road, which the caller keeps the deck on.
fn approach_cone(pin: f32, mouth: f32, natural: f32, dist: f32) -> f32 {
    let ramp = pin - JUNCTION_APPROACH_GRADE * dist;
    ramp.min(ramp - (mouth - natural))
}

/// The two-pass longitudinal grade limit (#573), in place: no frame stands more
/// than [`MAX_LONGITUDINAL_GRADE`] times its distance below either neighbour,
/// so a dip is bridged UPWARD and never buried.
fn limit_grade(y: &mut [f32], seg: &[f32]) {
    for i in 1..y.len() {
        y[i] = y[i].max(y[i - 1] - MAX_LONGITUDINAL_GRADE * seg[i - 1]);
    }
    for i in (0..y.len().saturating_sub(1)).rev() {
        y[i] = y[i].max(y[i + 1] - MAX_LONGITUDINAL_GRADE * seg[i]);
    }
}

/// Resolve a FLAT height per hub and the final deck height per chain across
/// the whole network (#584). Each hub is lifted to the max of (a) the terrain
/// under its centre, its junction nodes and its `ground` points (the deck
/// outline, per hub, where the caller knows it) + the depth bias, so a hub on
/// a local rise stays flat by lifting its mouths to clear it rather than
/// doming, and (b) the highest road mouth meeting it; every incident road is
/// then ramped up to that height by [`level_chain`]'s pin cones. A chain joins two
/// hubs, and raising one can raise the next, so hub heights are RELAXED to a
/// monotone-upward fixed point. Heights only ever rise and are bounded by the
/// highest terrain floor, so it converges; capped at [`MAX_LEVEL_ITERS`]
/// (graceful degradation - every chain still levels watertight, just
/// under-pinned). Returns `base_y` per chain (empty where the chain had no
/// meshable sample). The hubs and which chain ends open into them come from
/// the junction plan (#1558), so a cluster of junctions drawn as one deck
/// levels as one.
pub(crate) fn level_network(
    chains: &[Chain],
    samples: &[Option<ChainSample>],
    plan: &JunctionPlan,
    ground: &[Vec<[f32; 2]>],
    hm: &HeightMap,
) -> Vec<Vec<f32>> {
    // Per-chain floor profiles are immutable across the levelling passes, so
    // build them once here instead of re-collecting a fresh `Vec` per chain on
    // every one of the ~64 iterations + the final re-level (#643).
    let floors: Vec<Vec<f32>> = samples
        .iter()
        .map(|opt| {
            opt.as_ref()
                .map_or_else(Vec::new, |s| s.frames.iter().map(|r| r.floor).collect())
        })
        .collect();

    // Seed each hub at the highest terrain under its centre, its nodes and
    // `ground` - its deck outline, where the caller knows it - so no corner
    // of the flat deck buries.
    let mut hub_h: Vec<f32> = plan
        .hubs
        .iter()
        .enumerate()
        .map(|(i, h)| {
            std::iter::once(h.centre)
                .chain(h.points.iter().copied())
                .chain(ground.get(i).into_iter().flatten().map(|p| (p[0], p[1])))
                .map(|(x, z)| hm.get_height_at(x, z))
                .fold(f32::MIN, f32::max)
                + ROAD_DEPTH_BIAS_M
        })
        .collect();

    let pins = |hub_h: &[f32], ci: usize| plan.arm_hub[ci].map(|h| h.map(|h| hub_h[h]));
    let mut base_ys: Vec<Vec<f32>> = vec![Vec::new(); chains.len()];
    for _ in 0..MAX_LEVEL_ITERS {
        // Re-level every chain to its current hub pins.
        for (ci, s) in samples.iter().enumerate() {
            let Some(s) = s else { continue };
            base_ys[ci] = level_chain(&floors[ci], &s.seg, pins(&hub_h, ci));
        }
        // Lift each hub to the highest mouth now meeting it; track movement.
        let mut moved = 0.0_f32;
        for (ci, base) in base_ys.iter().enumerate() {
            if base.is_empty() {
                continue;
            }
            for (slot, hub) in plan.arm_hub[ci].iter().enumerate() {
                let Some(h) = *hub else {
                    continue;
                };
                let m = if slot == 0 {
                    base[0]
                } else {
                    base[base.len() - 1]
                };
                if m > hub_h[h] + LEVEL_CONVERGE_EPS_M {
                    moved = moved.max(m - hub_h[h]);
                    hub_h[h] = m;
                }
            }
        }
        if moved < LEVEL_CONVERGE_EPS_M {
            break;
        }
    }
    // Final re-level to the converged hub heights: the loop breaks just after a
    // lift, so without this the deck would lag the last (sub-eps) lift. This pins
    // every mouth EXACTLY to its hub's resolved height → spread is exactly 0.
    for (ci, s) in samples.iter().enumerate() {
        let Some(s) = s else { continue };
        base_ys[ci] = level_chain(&floors[ci], &s.seg, pins(&hub_h, ci));
    }
    base_ys
}

/// Per-hub incident-mouth height SPREAD (max − min over the roads meeting it)
/// for hubs with ≥ 2 meshed incident roads - 0 once the network levelling has
/// pinned every incident mouth to one height (#584 diagnostic).
pub(crate) fn junction_mouth_spreads(base_ys: &[Vec<f32>], plan: &JunctionPlan) -> Vec<f32> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<usize, (f32, f32, u32)> = BTreeMap::new(); // (min, max, count)
    for (ci, base) in base_ys.iter().enumerate() {
        if base.is_empty() {
            continue;
        }
        for (slot, hub) in plan.arm_hub[ci].iter().enumerate() {
            let Some(h) = *hub else {
                continue;
            };
            let m = if slot == 0 {
                base[0]
            } else {
                base[base.len() - 1]
            };
            let e = acc.entry(h).or_insert((f32::MAX, f32::MIN, 0));
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
