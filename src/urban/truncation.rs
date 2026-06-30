use crate::urban::{Chain, Dims};

/// Shortest ribbon (m) worth meshing after junction truncation (#575). A chain
/// trimmed below this at both ends sits entirely inside its hubs, so it grows no
/// ribbon — the hubs cover the gap — rather than a curb-framed sliver.
pub(crate) const MIN_RIBBON_LEN_M: f32 = 1.0;

// --- Junction truncation (#575) ---------------------------------------------
//
// At a real intersection (active degree ≥ 3) the incident ribbons must be
// *truncated* — pulled back along their centreline so they stop at the hub
// boundary rather than running to the node and overlapping each other (the
// un-truncated ribbons left holes / diamond gaps and the hub had no real
// polygon to fill). The pull-back distance per arm is the field-standard
// adjacent-boundary intersection, ported from `symbios-tensor`
// `roads_3d::compute_truncations`: arms are sorted by angle and each adjacent
// pair's *outer* boundary lines are intersected (a 2×2 solve) to find how far
// each arm must retreat so its footprint just clears its neighbour's. The
// boundary half-width is the full outer footprint `wo` (deck + curb + chamfer),
// so neither asphalt nor curb/skirt of adjacent roads overlaps; the hub
// (#576) still places its deck corners at the deck half-width.

/// Baseline (m) over which an arm's outgoing heading is measured, past the
/// junction fillet — short enough to track the road's true direction at the cut,
/// long enough that a rounded-corner tangent segment doesn't read as acute.
const ARM_DIR_BASELINE_M: f32 = 6.0;
/// Cap on a single arm's pull-back as a multiple of its outer footprint width.
/// Bounds the acute-fork blow-up (t → ∞ as the branch angle → 0) so truncation
/// never deletes a chain; genuinely acute joins are handled by the merge (#578).
pub(crate) const MAX_TRUNCATION_FACTOR: f32 = 4.0;

/// One road arm meeting a junction: which chain end it is, plus the centreline
/// geometry (unit `dir` node→road, its `right` perpendicular, deck half-width)
/// and the `angle` used to order arms around the node.
struct Arm {
    chain: usize,
    slot: usize,
    dir: (f32, f32),
    right: (f32, f32),
    half_w: f32,
    angle: f32,
}

/// The arm geometry at end `slot` (0 = start, 1 = end) of `chain`, or `None`
/// when the chain is degenerate (near-zero length). The heading is the chord
/// from the end node to the first point at least [`ARM_DIR_BASELINE_M`] inward,
/// so a short tangent *fillet* segment at the junction (rationalize rounds every
/// corner) can't masquerade as a near-parallel fork and blow the boundary solve
/// up. `dir` points from the end node *into* the road; `angle` uses the tensor
/// `atan2(-dz, dx)` convention so the radial sort matches the ported solve.
fn chain_arm(chain: &Chain, slot: usize) -> Option<(f32, f32, f32, f32, f32)> {
    let pts = &chain.pts;
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let base = if slot == 0 { pts[0] } else { pts[n - 1] };
    // Walk inward from the junction end, accumulating arc length, until the
    // chord clears the fillet baseline or the chain runs out.
    let (mut tip, mut prev, mut acc) = (base, base, 0.0_f32);
    for step in 1..n {
        let p = pts[if slot == 0 { step } else { n - 1 - step }];
        acc += (p.0 - prev.0).hypot(p.1 - prev.1);
        tip = p;
        prev = p;
        if acc >= ARM_DIR_BASELINE_M {
            break;
        }
    }
    let (dx, dz) = (tip.0 - base.0, tip.1 - base.1);
    let m = (dx * dx + dz * dz).sqrt();
    if m < 1.0e-6 {
        return None;
    }
    let dir = (dx / m, dz / m);
    let right = (-dir.1, dir.0);
    let angle = (-dir.1).atan2(dir.0);
    Some((dir.0, dir.1, right.0, right.1, angle))
}

/// Per-chain `[start_trim, end_trim]` pull-back distances (m): how far to shorten
/// each chain at each end that abuts a junction (`is_junction(end_node)` true).
/// Non-junction ends (dead-ends, district-edge clips) trim `0`. Deterministic:
/// chains are visited in order and arms ordered by a stable radial sort, so the
/// 2×2 solve assigns the same `t` to the same `(chain, slot)` every run.
pub(crate) fn compute_truncations(
    chains: &[Chain],
    is_junction: impl Fn(usize) -> bool,
    dims: &Dims,
) -> Vec<[f32; 2]> {
    use std::collections::BTreeMap;

    let mut trims = vec![[0.0_f32; 2]; chains.len()];
    let extra = dims.curb_top_width + dims.chamfer_width;

    // Gather the arms meeting each junction node.
    let mut by_node: BTreeMap<usize, Vec<Arm>> = BTreeMap::new();
    for (ci, chain) in chains.iter().enumerate() {
        for slot in 0..2 {
            let nd = chain.end_nodes[slot];
            if !is_junction(nd) {
                continue;
            }
            if let Some((dx, dz, rx, rz, angle)) = chain_arm(chain, slot) {
                by_node.entry(nd).or_default().push(Arm {
                    chain: ci,
                    slot,
                    dir: (dx, dz),
                    right: (rx, rz),
                    half_w: chain.half_w,
                    angle,
                });
            }
        }
    }

    for (_node, mut arms) in by_node {
        // Radial sort (stable → deterministic even for coincident angles).
        arms.sort_by(|a, b| {
            a.angle
                .partial_cmp(&b.angle)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let n = arms.len();
        // Each arm retreats at least its own deck half-width (a minimum hub
        // volume), then is pushed back further by each adjacent-boundary solve.
        let mut t: Vec<f32> = arms.iter().map(|a| a.half_w).collect();

        if n >= 2 {
            for i in 0..n {
                let j = (i + 1) % n;
                let (a, b) = (&arms[i], &arms[j]);
                let w_a = a.half_w + extra;
                let w_b = b.half_w + extra;

                // Arm A's left boundary  : center − right_A·w_A + dir_A·t_A
                // Arm B's right boundary : center + right_B·w_B + dir_B·t_B
                // Equate and solve the 2×2 system for (t_A, t_B):
                //   [dir_A.x  −dir_B.x][t_A]   [right_A.x·w_A + right_B.x·w_B]
                //   [dir_A.z  −dir_B.z][t_B] = [right_A.z·w_A + right_B.z·w_B]
                let rhs_x = a.right.0 * w_a + b.right.0 * w_b;
                let rhs_z = a.right.1 * w_a + b.right.1 * w_b;
                let det = a.dir.0 * (-b.dir.1) - (-b.dir.0) * a.dir.1;

                if det.abs() < 1.0e-6 {
                    // Near-parallel (collinear through-road or an acute pair):
                    // no clean crossing — fall back to half the combined width.
                    let fallback = (w_a + w_b) * 0.5;
                    t[i] = t[i].max(fallback);
                    t[j] = t[j].max(fallback);
                    continue;
                }

                let t_a = (rhs_x * (-b.dir.1) - (-b.dir.0) * rhs_z) / det;
                let t_b = (a.dir.0 * rhs_z - a.dir.1 * rhs_x) / det;
                if t_a > 0.0 {
                    t[i] = t[i].max(t_a);
                }
                if t_b > 0.0 {
                    t[j] = t[j].max(t_b);
                }
            }
        }

        for (k, a) in arms.iter().enumerate() {
            // Cap the pull-back at a width-relative maximum. Acute forks need a
            // far-away boundary crossing (t → ∞ as the branch angle → 0); without
            // a cap a single acute join would truncate whole chains out of
            // existence. Capping keeps a blunt over-truncation here — acute joins
            // are blended properly by the smooth-merge pass (#578).
            let cap = MAX_TRUNCATION_FACTOR * (a.half_w + extra);
            trims[a.chain][a.slot] = t[k].min(cap);
        }
    }

    // Keep at least [`MIN_RIBBON_LEN_M`] of ribbon on every trimmed chain. The
    // hub builder is mouth-driven — a chain only tells its junction where to put
    // the mouth by recording a `RoadEnd` during extrusion, which it can only do
    // if it meshes at least a stub. A short connector between two close junctions
    // (both ends pulled back ~wo) would otherwise be wholly consumed, dropping
    // its mouths and deleting the whole intersection (a hole — the inverse of the
    // gap #575 closes). Scale a chain's two pull-backs down together so the
    // surviving length is the floor; an untrimmed chain is left alone.
    for (ci, chain) in chains.iter().enumerate() {
        let [s, e] = trims[ci];
        if s + e <= 0.0 {
            continue;
        }
        let total: f32 = chain
            .pts
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
            .sum();
        let avail = (total - MIN_RIBBON_LEN_M).max(0.0);
        if s + e > avail {
            let scale = avail / (s + e); // s + e > 0 here
            trims[ci] = [s * scale, e * scale];
        }
    }

    trims
}

/// Shorten a polyline by `start_trim` / `end_trim` metres of arc length from
/// each end, inserting interpolated cut points so the ribbon stops exactly at
/// the hub boundary. If the two pull-backs would leave less than
/// [`MIN_RIBBON_LEN_M`] of road, returns fewer than two points (no ribbon).
/// Never inverts. In production [`compute_truncations`] already scales the
/// pull-backs so a junction chain keeps at least the floor — so this guard only
/// fires for a chain trimmed in isolation; a real junction chain always survives
/// to record its mouth.
pub(crate) fn trim_polyline(pts: &[(f32, f32)], start_trim: f32, end_trim: f32) -> Vec<(f32, f32)> {
    let (start_trim, end_trim) = (start_trim.max(0.0), end_trim.max(0.0));
    if pts.len() < 2 || (start_trim <= 0.0 && end_trim <= 0.0) {
        return pts.to_vec();
    }

    let mut arc = Vec::with_capacity(pts.len());
    arc.push(0.0_f32);
    for w in pts.windows(2) {
        let d = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        arc.push(arc[arc.len() - 1] + d);
    }
    let total = arc[arc.len() - 1];

    // Inversion guard only — the real keep-a-stub floor ([`MIN_RIBBON_LEN_M`]) is
    // applied upstream in [`compute_truncations`], which scales a junction chain's
    // pull-backs so a meshable length always survives. This catches a chain
    // trimmed in isolation (or a degenerate near-zero one) so we never emit a
    // back-to-front ribbon.
    let (t0, t1) = (start_trim, total - end_trim);
    if t1 - t0 < 1.0e-3 {
        return Vec::new();
    }

    let at = |target: f32| -> (f32, f32) {
        for i in 1..pts.len() {
            if arc[i] >= target {
                let seg = arc[i] - arc[i - 1];
                if seg < 1.0e-6 {
                    return pts[i];
                }
                let f = (target - arc[i - 1]) / seg;
                return (
                    pts[i - 1].0 + (pts[i].0 - pts[i - 1].0) * f,
                    pts[i - 1].1 + (pts[i].1 - pts[i - 1].1) * f,
                );
            }
        }
        *pts.last().unwrap_or(&pts[0])
    };

    let mut out = Vec::new();
    out.push(at(t0));
    for i in 1..pts.len() - 1 {
        if arc[i] > t0 && arc[i] < t1 {
            out.push(pts[i]);
        }
    }
    out.push(at(t1));
    out
}

#[cfg(test)]
mod tests;
