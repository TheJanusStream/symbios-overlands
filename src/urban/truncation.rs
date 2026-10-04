//! Junction truncation and hub planning (#575, #1558): how far each chain end
//! retreats before its hub, and which junction nodes are drawn as one hub.
//! Un-truncated ribbons run to the junction node and overlap, leaving diamond
//! holes and no real polygon for the hub to fill. Each arm's pull-back is the
//! adjacent-boundary intersection - arms sorted round their node, each
//! neighbouring pair's *outer* footprints solved 2×2 - ported from
//! `symbios-tensor`, so a corner's two curb lines meet where the hub's curb
//! turns. The distance is capped, since it diverges as a fork closes.
//!
//! Junctions a few metres apart are drawn as ONE hub (#1558): a chain too
//! short to carry the pull-backs at both its ends is swallowed by the hub
//! rather than meshed as a sliver, and its two junctions merge. Before this
//! the chain kept a 1 m stub of ribbon and each junction grew its own hub
//! over the other's mouths - the corners of the two decks interleaved and the
//! hub's curbs and skirts were swept across the asphalt, the shards seen at
//! dense junctions. The merge is a drawing decision only: the graph, and the
//! lots it grows, are untouched.
//!
//! A hub is capped ([`MAX_HUB_NODES`] junction nodes, and no two of them
//! further apart than one swallowed chain can reach): on a dense plan the
//! swallow used to chain from junction to junction until one hub paved a
//! whole district over as a flat plate. A chain that would grow a hub past
//! its cap is drawn as a short ribbon between two hubs instead.

use std::collections::BTreeMap;

use crate::urban::{Chain, Dims};

/// Shortest ribbon (m) worth meshing after junction truncation (#575). A chain
/// that would be left shorter than this between its pull-backs is drawn inside
/// its hub instead (#1558) - its junctions merge into one hub.
pub(crate) const MIN_RIBBON_LEN_M: f32 = 1.0;

// --- Junction truncation (#575) ---------------------------------------------
//
// At a real intersection (active degree ≥ 3) the incident ribbons must be
// *truncated* - pulled back along their centreline so they stop at the hub
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
/// junction fillet - short enough to track the road's true direction at the cut,
/// long enough that a rounded-corner tangent segment doesn't read as acute.
const ARM_DIR_BASELINE_M: f32 = 6.0;
/// Cap on an arm's pull-back as a multiple of the widest outer footprint at
/// its hub. Bounds the acute-fork blow-up (t → ∞ as the branch angle → 0),
/// so a sharp fork's hub never becomes a long flat gore over the land: past
/// the cap the two ribbons still overlap, and the hub joins their mouths with
/// a chord. Six widths part two equal streets down to about 19 degrees, the
/// sharpest junction the sanitiser's graze cut leaves on a through road.
pub(crate) const MAX_TRUNCATION_FACTOR: f32 = 6.0;
/// Most junction nodes one hub may cover (#1558). In a sound plan no hub
/// comes near it - Isoline's largest covers 3 nodes, a 70/35 m plan's 8 -
/// while on a dense one the swallow chained on until one hub covered the
/// district. The hub's span is capped too: no two of its nodes further
/// apart than the longest chain one swallow can take (both ends' pull-backs
/// at [`MAX_TRUNCATION_FACTOR`] plus [`MIN_RIBBON_LEN_M`]), so every hub of
/// two nodes still forms and none grows by chaining swallows.
pub(crate) const MAX_HUB_NODES: usize = 8;

/// One road arm meeting a hub: which chain end it is, plus the centreline
/// geometry (its node, unit `dir` node→road, its counter-clockwise `left`
/// perpendicular, deck and outer half-widths).
#[derive(Clone, Copy)]
struct Arm {
    chain: usize,
    slot: usize,
    node_id: usize,
    node: (f32, f32),
    dir: (f32, f32),
    left: (f32, f32),
    half_w: f32,
    outer: f32,
}

impl Arm {
    /// The arm's direction as an angle (`atan2(z, x)`), its order round its
    /// node.
    fn angle(&self) -> f32 {
        self.dir.1.atan2(self.dir.0)
    }
}

/// The arm geometry at end `slot` (0 = start, 1 = end) of `chain`, or `None`
/// when the chain is degenerate (near-zero length): `(node, dir)`. The
/// heading is the chord from the end node to the first point at least
/// [`ARM_DIR_BASELINE_M`] inward, so a short tangent *fillet* segment at the
/// junction (rationalize rounds every corner) can't masquerade as a
/// near-parallel fork and blow the boundary solve up. `dir` points from the
/// end node *into* the road.
fn chain_arm(chain: &Chain, slot: usize) -> Option<((f32, f32), (f32, f32))> {
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
    Some((base, (dx / m, dz / m)))
}

/// A set of junction nodes drawn as one deck (#1558): one node, or a cluster
/// whose connecting chains were too short to carry a ribbon.
pub(crate) struct Hub {
    /// The junction nodes the deck covers, ascending.
    pub(crate) nodes: Vec<usize>,
    /// Their positions in the district window, in the same order.
    pub(crate) points: Vec<(f32, f32)>,
    /// The swallowed chains joining two of its nodes, as `(node, node,
    /// deck half-width)`: drawn by the hub, its outline walking round them.
    pub(crate) links: Vec<(usize, usize, f32)>,
    /// The deck's centre in the district window: its node, or the mean of
    /// its nodes.
    pub(crate) centre: (f32, f32),
    /// The chain ends `(chain, slot)` that open into it.
    pub(crate) arms: Vec<(usize, usize)>,
}

/// How the network's junctions are drawn (#575, #1558): every chain end's
/// pull-back, which hub it opens into, which ends are capped, and which
/// chains are swallowed by a hub.
pub(crate) struct JunctionPlan {
    /// Per chain `[start, end]` pull-back (m): 0 at an end that opens into
    /// no hub.
    pub(crate) trims: Vec<[f32; 2]>,
    /// Per chain end, the index into [`Self::hubs`] of the hub it opens into.
    pub(crate) arm_hub: Vec<[Option<usize>; 2]>,
    /// Per chain end, whether its ribbon closes with an end cap: a dead end
    /// (#579), a district-edge clip (#582), or a junction whose hub kept
    /// only this one arm.
    pub(crate) cap: Vec<[bool; 2]>,
    /// Per chain, whether it is drawn inside a hub (too short to carry its
    /// pull-backs) and so grows no ribbon of its own.
    pub(crate) internal: Vec<bool>,
    /// Per chain, whether it was too short to carry its pull-backs but
    /// swallowing it would have grown a hub past its cap, so it is drawn as
    /// a short ribbon: its pull-backs shrunk in proportion to leave
    /// [`MIN_RIBBON_LEN_M`] of it between its two hubs.
    pub(crate) short_ribbon: Vec<bool>,
    /// The hubs, each with at least two arms.
    pub(crate) hubs: Vec<Hub>,
}

/// How one chain's two ends close (#1558), as the ribbon mesher reads it:
/// the hub each end opens into, whether it is capped, and how far it was
/// pulled back (the stub of chain the hub draws in its place).
#[derive(Clone, Copy, Default)]
pub(crate) struct ChainEnds {
    pub(crate) hub: [Option<usize>; 2],
    pub(crate) cap: [bool; 2],
    pub(crate) trim: [f32; 2],
}

impl JunctionPlan {
    /// How chain `ci`'s ends close.
    pub(crate) fn chain_ends(&self, ci: usize) -> ChainEnds {
        ChainEnds {
            hub: self.arm_hub[ci],
            cap: self.cap[ci],
            trim: self.trims[ci],
        }
    }
}

/// Arc length of a chain's polyline.
fn chain_length(chain: &Chain) -> f32 {
    chain
        .pts
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum()
}

/// Union-find root with path-halving.
fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Plan every junction of the drawn network (#575, #1558). A node of active
/// `degree` ≥ 3 is a junction; the chain ends meeting it are its arms. Each
/// arm retreats far enough that its outer footprint clears its angular
/// neighbours' round its node (the adjacent-boundary solve), at least its
/// own deck half-width, at most [`MAX_TRUNCATION_FACTOR`] times the hub's
/// widest outer footprint. A chain left shorter than [`MIN_RIBBON_LEN_M`]
/// between its pull-backs is swallowed, shortest first: its two junctions
/// merge into one hub, the chain becoming one of the hub's links (a
/// neighbour its nodes' other arms must clear too), or at a dead end the
/// stub is dropped; the plan is re-solved until nothing more is swallowed.
/// A swallow that would grow a hub past [`MAX_HUB_NODES`] nodes or its span
/// cap is refused, and that chain drawn as a short ribbon. A hub left with
/// one arm caps it instead.
///
/// Deterministic: chains are visited in order of length then index, arms
/// ordered by a stable radial sort, clusters keyed by their lowest node.
pub(crate) fn plan_junctions(chains: &[Chain], degree: &[u32], dims: &Dims) -> JunctionPlan {
    let n = chains.len();
    let extra = dims.curb_top_width + dims.chamfer_width;
    let is_junction = |nd: usize| degree.get(nd).copied().unwrap_or(0) >= 3;
    let lens: Vec<f32> = chains.iter().map(chain_length).collect();

    // Node positions, read off the chain ends.
    let mut node_pos: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    for c in chains {
        if let (Some(&first), Some(&last)) = (c.pts.first(), c.pts.last()) {
            node_pos.entry(c.end_nodes[0]).or_insert(first);
            node_pos.entry(c.end_nodes[1]).or_insert(last);
        }
    }
    // Every chain end at a junction, with its centreline geometry.
    let arm_at = |ci: usize, slot: usize| -> Option<Arm> {
        let c = &chains[ci];
        if !is_junction(c.end_nodes[slot]) {
            return None;
        }
        let (node, dir) = chain_arm(c, slot)?;
        Some(Arm {
            chain: ci,
            slot,
            node_id: c.end_nodes[slot],
            node,
            dir,
            left: (-dir.1, dir.0),
            half_w: c.half_w,
            outer: c.half_w + extra,
        })
    };
    let arms: Vec<[Option<Arm>; 2]> = (0..n).map(|ci| [arm_at(ci, 0), arm_at(ci, 1)]).collect();

    let max_node = chains
        .iter()
        .flat_map(|c| c.end_nodes)
        .max()
        .map_or(0, |m| m + 1);
    let mut parent: Vec<usize> = (0..max_node).collect();
    // Every cluster's junction nodes, at its root, for the hub cap.
    let mut members: Vec<Vec<usize>> = (0..max_node).map(|nd| vec![nd]).collect();
    let widest = dims.major_half_width.max(dims.minor_half_width) + extra;
    let span_cap = 2.0 * MAX_TRUNCATION_FACTOR * widest + MIN_RIBBON_LEN_M;
    let fits = |a: &[usize], b: &[usize]| {
        a.len() + b.len() <= MAX_HUB_NODES
            && a.iter().all(|p| {
                b.iter().all(|q| match (node_pos.get(p), node_pos.get(q)) {
                    (Some(p), Some(q)) => {
                        let (dx, dz) = (q.0 - p.0, q.1 - p.1);
                        (dx * dx + dz * dz).sqrt() <= span_cap
                    }
                    _ => true,
                })
            })
    };
    let mut internal = vec![false; n];
    let mut trims = vec![[0.0_f32; 2]; n];
    let mut groups: BTreeMap<usize, Vec<Arm>> = BTreeMap::new();

    // Each pass swallows at least one more chain or stops, so n + 1 passes
    // always reach the fixed point.
    for _ in 0..=n {
        // Hubs: every live arm grouped by its junction's cluster root.
        groups.clear();
        for (ci, ends) in arms.iter().enumerate() {
            if internal[ci] {
                continue;
            }
            for arm in ends.iter().flatten() {
                let root = uf_find(&mut parent, chains[ci].end_nodes[arm.slot]);
                groups.entry(root).or_default().push(*arm);
            }
        }
        let links = hub_links(chains, &internal, &mut parent, &node_pos, degree);
        trims = vec![[0.0_f32; 2]; n];
        for (root, group) in &groups {
            let hub_links = links.get(root).map(Vec::as_slice).unwrap_or(&[]);
            for (arm, t) in group
                .iter()
                .zip(hub_pullbacks(group, hub_links, &node_pos, extra))
            {
                trims[arm.chain][arm.slot] = t;
            }
        }
        // Swallow every chain its pull-backs leave without a meshable ribbon,
        // shortest first, unless that grows a hub past its cap.
        let mut short: Vec<usize> = (0..n)
            .filter(|&ci| !internal[ci] && too_short(trims[ci], lens[ci]))
            .collect();
        short.sort_by(|&x, &y| lens[x].total_cmp(&lens[y]).then(x.cmp(&y)));
        let mut swallowed = false;
        for ci in short {
            let [a, b] = chains[ci].end_nodes;
            if is_junction(a) && is_junction(b) {
                let (ra, rb) = (uf_find(&mut parent, a), uf_find(&mut parent, b));
                if ra != rb {
                    if !fits(&members[ra], &members[rb]) {
                        continue; // held: drawn as a short ribbon
                    }
                    let (keep, gone) = (ra.min(rb), ra.max(rb));
                    parent[gone] = keep;
                    let moved = std::mem::take(&mut members[gone]);
                    members[keep].extend(moved);
                }
            }
            internal[ci] = true;
            swallowed = true;
        }
        if !swallowed {
            break;
        }
    }

    // A chain the hub cap held is drawn as a short ribbon: its pull-backs
    // shrink in proportion to leave MIN_RIBBON_LEN_M of it, and its hubs'
    // mouths open where they now stop.
    let short_ribbon: Vec<bool> = (0..n)
        .map(|ci| !internal[ci] && too_short(trims[ci], lens[ci]))
        .collect();
    for ci in (0..n).filter(|&ci| short_ribbon[ci]) {
        let [s, e] = trims[ci];
        let f = (lens[ci] - MIN_RIBBON_LEN_M).max(0.0) / (s + e);
        trims[ci] = [s * f, e * f];
    }

    // The hubs that keep two or more arms; a lone arm is capped instead.
    let members = cluster_members(&mut parent, &node_pos, degree);
    let links = hub_links(chains, &internal, &mut parent, &node_pos, degree);
    let mut arm_hub = vec![[None; 2]; n];
    let mut hubs = Vec::new();
    for (root, group) in &groups {
        if group.len() < 2 {
            for arm in group {
                trims[arm.chain][arm.slot] = 0.0;
            }
            continue;
        }
        let nodes = members.get(root).cloned().unwrap_or_default();
        let points: Vec<(f32, f32)> = nodes.iter().map(|nd| node_pos[nd]).collect();
        let k = points.len().max(1) as f32;
        let centre = (
            points.iter().map(|p| p.0).sum::<f32>() / k,
            points.iter().map(|p| p.1).sum::<f32>() / k,
        );
        for arm in group {
            arm_hub[arm.chain][arm.slot] = Some(hubs.len());
        }
        hubs.push(Hub {
            nodes,
            points,
            links: links
                .get(root)
                .map(|l| l.iter().map(|k| (k.from, k.to, k.half_w)).collect())
                .unwrap_or_default(),
            centre,
            arms: group.iter().map(|a| (a.chain, a.slot)).collect(),
        });
    }
    let cap = (0..n)
        .map(|ci| {
            std::array::from_fn(|slot| {
                let nd = chains[ci].end_nodes[slot];
                !internal[ci]
                    && arm_hub[ci][slot].is_none()
                    && (degree.get(nd).copied().unwrap_or(0) == 1
                        || chains[ci].clip[slot]
                        || is_junction(nd))
            })
        })
        .collect();
    JunctionPlan {
        trims,
        arm_hub,
        cap,
        internal,
        short_ribbon,
        hubs,
    }
}

/// Whether pull-backs `[s, e]` leave a chain of length `len` too short to
/// mesh as a ribbon of its own (under [`MIN_RIBBON_LEN_M`]).
fn too_short([s, e]: [f32; 2], len: f32) -> bool {
    s + e > 0.0 && s + e + MIN_RIBBON_LEN_M > len
}

/// A swallowed chain joining two junction nodes of one hub: the hub draws
/// it, so to its nodes it is one more street leaving them.
#[derive(Clone, Copy)]
struct Link {
    from: usize,
    to: usize,
    half_w: f32,
}

/// Every cluster's links, keyed by its root: the swallowed chains whose two
/// ends are distinct junction nodes of the same cluster.
fn hub_links(
    chains: &[Chain],
    internal: &[bool],
    parent: &mut [usize],
    node_pos: &BTreeMap<usize, (f32, f32)>,
    degree: &[u32],
) -> BTreeMap<usize, Vec<Link>> {
    let is_junction = |nd: usize| degree.get(nd).copied().unwrap_or(0) >= 3;
    let mut links: BTreeMap<usize, Vec<Link>> = BTreeMap::new();
    for (ci, c) in chains.iter().enumerate() {
        let [a, b] = c.end_nodes;
        if !internal[ci] || a == b || !is_junction(a) || !is_junction(b) {
            continue;
        }
        let (ra, rb) = (uf_find(parent, a), uf_find(parent, b));
        if ra != rb || !node_pos.contains_key(&a) || !node_pos.contains_key(&b) {
            continue;
        }
        links.entry(ra).or_default().push(Link {
            from: a,
            to: b,
            half_w: c.half_w,
        });
    }
    links
}

/// Every cluster's junction nodes, keyed by its root, ascending.
fn cluster_members(
    parent: &mut [usize],
    node_pos: &BTreeMap<usize, (f32, f32)>,
    degree: &[u32],
) -> BTreeMap<usize, Vec<usize>> {
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &nd in node_pos.keys() {
        if degree.get(nd).copied().unwrap_or(0) >= 3 {
            let root = uf_find(parent, nd);
            members.entry(root).or_default().push(nd);
        }
    }
    members
}

/// Each arm's pull-back (m) for one hub: at least its deck half-width,
/// pushed back by the adjacent-boundary solve against each angular
/// neighbour round its own node - another arm, or a link to another of the
/// hub's nodes, whose footprint the hub draws - and capped at
/// [`MAX_TRUNCATION_FACTOR`] times the hub's widest outer footprint.
fn hub_pullbacks(
    arms: &[Arm],
    links: &[Link],
    node_pos: &BTreeMap<usize, (f32, f32)>,
    extra: f32,
) -> Vec<f32> {
    let mut t: Vec<f32> = arms.iter().map(|a| a.half_w).collect();
    // Round every node of the hub: its arms (by index) and its links (as
    // fixed neighbours with no mouth of their own).
    let mut at: BTreeMap<usize, Vec<(Option<usize>, Arm)>> = BTreeMap::new();
    for (k, a) in arms.iter().enumerate() {
        at.entry(a.node_id).or_default().push((Some(k), *a));
    }
    for l in links {
        for (from, to) in [(l.from, l.to), (l.to, l.from)] {
            let (p, q) = (node_pos[&from], node_pos[&to]);
            let (dx, dz) = (q.0 - p.0, q.1 - p.1);
            let len = dx.hypot(dz);
            if len < 1.0e-6 {
                continue;
            }
            let dir = (dx / len, dz / len);
            at.entry(from).or_default().push((
                None,
                Arm {
                    chain: usize::MAX,
                    slot: 0,
                    node_id: from,
                    node: p,
                    dir,
                    left: (-dir.1, dir.0),
                    half_w: l.half_w,
                    outer: l.half_w + extra,
                },
            ));
        }
    }
    for items in at.values_mut() {
        items.sort_by(|(_, a), (_, b)| a.angle().total_cmp(&b.angle()));
        let n = items.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let ((ka, a), (kb, b)) = (items[i], items[j]);
            // B is A's counter-clockwise neighbour: A's left boundary faces
            // B's right boundary.
            //   A : node_A + dir_A·t_A + left_A·wo_A
            //   B : node_B + dir_B·t_B − left_B·wo_B
            // Equate and solve the 2×2 system for (t_A, t_B):
            //   dir_A·t_A − dir_B·t_B = node_B − node_A − left_B·wo_B − left_A·wo_A
            let rhs_x = b.node.0 - a.node.0 - b.left.0 * b.outer - a.left.0 * a.outer;
            let rhs_z = b.node.1 - a.node.1 - b.left.1 * b.outer - a.left.1 * a.outer;
            let det = b.dir.0 * a.dir.1 - a.dir.0 * b.dir.1;
            let (t_a, t_b) = if det.abs() < 1.0e-6 {
                // Near-parallel (a collinear through-road or an acute pair):
                // no clean crossing - fall back to half the combined width.
                let fallback = (a.outer + b.outer) * 0.5;
                (fallback, fallback)
            } else {
                (
                    (b.dir.0 * rhs_z - b.dir.1 * rhs_x) / det,
                    (a.dir.0 * rhs_z - a.dir.1 * rhs_x) / det,
                )
            };
            if let Some(ka) = ka
                && t_a > 0.0
            {
                t[ka] = t[ka].max(t_a);
            }
            if let Some(kb) = kb
                && t_b > 0.0
            {
                t[kb] = t[kb].max(t_b);
            }
        }
    }
    // Cap the solve: past it, a closing fork's ribbons overlap rather than
    // the hub stretching into a long flat gore.
    let widest = arms.iter().map(|a| a.outer).fold(0.0_f32, f32::max);
    for tk in &mut t {
        *tk = tk.min(MAX_TRUNCATION_FACTOR * widest);
    }
    t
}

/// Shorten a polyline by `start_trim` / `end_trim` metres of arc length from
/// each end, inserting interpolated cut points so the ribbon stops exactly at
/// the hub boundary. If the two pull-backs would leave less than a millimetre
/// of road, returns fewer than two points (no ribbon). Never inverts. In
/// production [`plan_junctions`] already swallows a chain its pull-backs
/// would consume, so this guard only fires for a chain trimmed in isolation.
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

    // Inversion guard only - the real keep-a-ribbon rule lives upstream in
    // [`plan_junctions`]. This catches a chain trimmed in isolation (or a
    // degenerate near-zero one) so we never emit a back-to-front ribbon.
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
