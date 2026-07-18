use bevy_symbios_ground::HeightMap;
use symbios_tensor::{
    RationalizeConfig, RoadGraph, RoadType, TensorConfig, generate_roads, rationalize_graph,
};

use crate::pds::generator::RoadConfig;

/// The road network's rationalized planar graph for `config`, plus the district
/// sub-heightmap it was traced on and that window's lower cell index `lo`.
/// `None` when the network is disabled, the window is too small, or the tracer
/// can't produce a network. Deterministic in `config.seed`. Never writes back
/// to `hm` (the `sub` copy is the only mutable surface, and nothing carves it).
///
/// Shared by [`crate::urban::build_road_geometry`] (the draped mesh) and
/// [`crate::urban::extract_building_lots`] (footprints) so both read the *same* graph — a
/// building can only sit on a street if it was placed from the geometry the
/// player actually sees.
pub(crate) fn build_road_graph(
    hm: &HeightMap,
    config: &RoadConfig,
) -> Option<(RoadGraph, HeightMap, [usize; 2])> {
    let (mut graph, sub, lo) = build_road_graph_raw(hm, config)?;
    // Clean tracer / rationalize artefacts (grazing false junctions and dead-end
    // stubs) out of the topology, and weld near-miss dead-ends into junctions,
    // before it is meshed *or* lotted — see [`sanitize_graph`]. Both consumers read
    // the same cleaned graph. Weld tolerance is per-room (a fraction of spacing).
    sanitize_graph(&mut graph, WELD_TOL_FRACTION * config.minor_spacing.0);
    Some((graph, sub, lo))
}

/// The raw rationalized graph — `generate_roads` + `rationalize_graph`, *before*
/// [`sanitize_graph`]. Split out so the diagnostic dump can compare the graph
/// before and after sanitation (see [`crate::urban::road_graph_diagnostics`]).
pub(crate) fn build_road_graph_raw(
    hm: &HeightMap,
    config: &RoadConfig,
) -> Option<(RoadGraph, HeightMap, [usize; 2])> {
    if !config.enabled {
        return None;
    }
    let scale = hm.scale();
    let full_w = hm.width();
    let half_cells = ((config.district_half_extent.0 / scale).round() as usize).min(full_w / 2);
    let side = half_cells * 2;
    if side < 8 {
        return None;
    }
    // District centre (#889): the authored XZ offset in cells, clamped so the
    // window always stays fully inside the heightmap — pushing the centre past
    // an edge slides the district back rather than truncating it.
    let max_lo = full_w - side;
    let lo_axis = |offset_m: f32| -> usize {
        let centered = full_w as f32 / 2.0 + offset_m / scale;
        ((centered - half_cells as f32).round().max(0.0) as usize).min(max_lo)
    };
    let lo = [lo_axis(config.center.0[0]), lo_axis(config.center.0[1])];

    // District window → its own heightmap, both for tensor to road and for us
    // to sample heights from. Copied, never written back (no carving).
    let mut sub = HeightMap::new(side, side, scale);
    for z in 0..side {
        for x in 0..side {
            sub.set(x, z, hm.get(lo[0] + x, lo[1] + z));
        }
    }

    let mut cfg = TensorConfig {
        seed: config.seed,
        major_road_dist: config.major_spacing.0,
        minor_road_dist: config.minor_spacing.0,
        ..TensorConfig::default()
    };
    // Street-plan style (#890): trade the field's axis-aligned fallback
    // against terrain-derived directions. `Hillside` (and `Unknown`, the
    // forward-compat arm) keeps the historical adaptive blend.
    match config.style {
        crate::pds::generator::RoadStyle::Grid => {
            // Slope thresholds above any real terrain slope → the pure
            // axis-aligned Manhattan fallback everywhere.
            cfg.field.flat_threshold_low = f32::MAX;
            cfg.field.flat_threshold_high = f32::MAX;
        }
        crate::pds::generator::RoadStyle::Organic => {
            // Near-zero thresholds → terrain-derived directions on any real
            // slope (kept strictly positive so dead-flat ground still has
            // the axis fallback instead of a degenerate direction), plus a
            // gentle wander: low-frequency jitter + looser tracer momentum.
            cfg.field.flat_threshold_low = 1.0e-6;
            cfg.field.flat_threshold_high = 2.0e-6;
            cfg.field.jitter_amplitude = 0.15;
            cfg.tracer_inertia = 0.6;
        }
        crate::pds::generator::RoadStyle::Hillside | crate::pds::generator::RoadStyle::Unknown => {}
    }
    let mut graph = generate_roads(&sub, &cfg).ok()?;
    // Rationalize for clean XZ geometry (RDP straighten + Bézier fillets). We
    // ignore its smoothed elevations and sample the real terrain when draping.
    rationalize_graph(&mut graph, &sub, &RationalizeConfig::default());
    Some((graph, sub, lo))
}

// --- Graph sanitation (#571) ------------------------------------------------
//
// The tensor tracer welds a junction wherever a trace passes within
// `snap_radius` of an existing edge, and leaves dead-end stubs where a trace
// runs out; `rationalize_graph` straightens and fillets but never cleans the
// *topology*. So the mesher inherits two artefacts the `--road-dump` diagnostic
// measured as dominant: grazing false junctions (~23 % of hubs) and short
// dead-end stubs. We clear both here by deactivating edges — the exact `active`
// mechanism `prune_unused_roads` uses, so node lists / positions are untouched
// and the planar structure stays valid for `extract_blocks` / `extract_lots`.

/// A dead-end edge shorter than this (m) is a tracer stub: deactivated.
const SANITIZE_STUB_LEN_M: f32 = 8.0;
/// Two branches within this of 180° at a node form a straight through-road.
const SANITIZE_COLLINEAR_TOL_DEG: f32 = 25.0;
/// A third branch within this of the through-line is a glancing graze: cut.
const SANITIZE_GRAZE_ANGLE_DEG: f32 = 20.0;
/// Safety cap on sanitation passes. Cutting a graze can drop a degree-4 node to
/// degree-3 and expose a fresh graze (or leave a fresh stub), so removals
/// cascade; this bounds the fixed-point loop well above the depth real networks
/// reach.
const SANITIZE_MAX_PASSES: usize = 24;
/// Collapse an active edge shorter than this (m): a near-zero segment whose
/// unstable direction is what makes the miter spike (the in-game "glitch
/// segments"). Well below any real road feature.
const MERGE_EDGE_LEN_M: f32 = 0.5;
/// Merge two distinct nodes closer than this (m): the snap-welded near-duplicate
/// vertices that render as lumpy double-hubs and parallel edges. Far below the
/// ~100 m+ spacing of real junctions, so genuine ones never merge.
const MERGE_NODE_EPS_M: f32 = 1.0;
/// Foot-of-perpendicular must land at least this fraction of a target segment's
/// length inside each endpoint for a dead-end to weld onto it (#583): landing in
/// the outer margin is a near-NODE case, owned by [`merge_coincident_nodes`], not
/// a mid-span T-junction.
const WELD_T_MARGIN: f32 = 0.05;
/// Minimum crossing angle (deg) between a dead-end's heading and the edge it would
/// weld onto. Shallower than this the two roads run near-parallel — a graze, not a
/// junction — and are left alone. The additive counterpart to the #571 graze CUT:
/// that removes false junctions, this creates the missing true ones.
const WELD_MIN_CROSS_ANGLE_DEG: f32 = 25.0;
/// Weld tolerance as a fraction of the room's minor-road spacing (#583): a dead-end
/// whose perpendicular gap to a non-incident edge is under `fraction × minor_spacing`
/// welds into it. Per-room-relative so a dense room can't cross-weld the next street.
/// At 0.08 it is ≈7.5 m on the densest seeded room (94 m spacing) up to ≈14 m on the
/// sparsest, and ≥ 4 m even on the 55 m struct default — always well under spacing yet
/// at/above the tracer's 4 m snap radius (the sizing sweep showed the candidate count
/// is flat from 4–8 m on every road-growing seed, so the exact value isn't delicate).
pub(crate) const WELD_TOL_FRACTION: f32 = 0.08;

/// Clean the road graph in place and deterministically. First **merge**
/// coincident nodes (collapsing near-zero segments and near-duplicate vertices —
/// the source of the glitch spikes and lumpy double-hubs), then **cut** the
/// remaining stub / graze artefacts to a fixed point (a cut can expose a fresh
/// stub, and vice-versa, so passes repeat until one cuts nothing).
pub(crate) fn sanitize_graph(graph: &mut RoadGraph, weld_tol: f32) {
    merge_coincident_nodes(graph);
    for _ in 0..SANITIZE_MAX_PASSES {
        // Weld near-miss dead-ends into junctions (#583, additive), then cut the
        // remaining stub / graze artefacts (subtractive). A weld only raises node
        // degree (never makes a fresh dead-end) and always meets the split edge
        // perpendicularly (never a graze), so it neither feeds the cuts nor is
        // undone by them — the loop still converges.
        let welds = weld_endpoint_dangles(graph, weld_tol);
        let targets = sanitize_targets(graph);
        for ei in &targets {
            graph.edges[*ei].active = false;
        }
        if welds == 0 && targets.is_empty() {
            break;
        }
    }
}

/// One pass: the set of edge ids to deactivate given the current active graph.
/// Read-only so the caller applies all cuts atomically (order-independent →
/// deterministic). Returns sorted unique ids.
fn sanitize_targets(graph: &RoadGraph) -> Vec<usize> {
    let n = graph.nodes.len();
    let pos = |i: usize| {
        let p = graph.nodes[i].position;
        (p.x, p.y)
    };
    // Active adjacency: per node, (neighbour, edge id, length).
    let mut adj: Vec<Vec<(usize, usize, f32)>> = vec![Vec::new(); n];
    for (ei, e) in graph.edges.iter().enumerate() {
        if !e.active {
            continue;
        }
        let (s, t) = (e.start as usize, e.end as usize);
        let (a, b) = (pos(s), pos(t));
        let l = (a.0 - b.0).hypot(a.1 - b.1);
        adj[s].push((t, ei, l));
        adj[t].push((s, ei, l));
    }

    let mut targets: std::collections::BTreeSet<usize> = Default::default();

    // 1. Short dead-end stubs.
    for edges in &adj {
        if edges.len() == 1 {
            let (_, ei, l) = edges[0];
            if l < SANITIZE_STUB_LEN_M {
                targets.insert(ei);
            }
        }
    }

    // 2. Grazing T-junctions: a degree-3 node with a near-collinear through-pair
    //    and a third branch nearly parallel to that through-line. Real 3-way
    //    junctions (branches ~120° apart) have no collinear pair, so they are
    //    never touched; only the snap-welded tangential touch is cut.
    let collinear_cos = (180.0 - SANITIZE_COLLINEAR_TOL_DEG).to_radians().cos();
    let graze_cos = SANITIZE_GRAZE_ANGLE_DEG.to_radians().cos();
    for (h, edges) in adj.iter().enumerate() {
        if edges.len() != 3 {
            continue;
        }
        let hp = pos(h);
        let dir = |k: usize| {
            let np = pos(edges[k].0);
            let (dx, dz) = (np.0 - hp.0, np.1 - hp.1);
            let m = (dx * dx + dz * dz).sqrt().max(1.0e-6);
            (dx / m, dz / m)
        };
        let d = [dir(0), dir(1), dir(2)];
        // Through-pair = the pair closest to 180° (most-negative cosine).
        let mut best = (0usize, 1usize, 1.0_f32);
        for a in 0..3 {
            for b in (a + 1)..3 {
                let c = d[a].0 * d[b].0 + d[a].1 * d[b].1;
                if c < best.2 {
                    best = (a, b, c);
                }
            }
        }
        if best.2 > collinear_cos {
            continue; // no straight through-road here → a real junction
        }
        let k = 3 - best.0 - best.1; // the remaining (graze) branch
        let (ax, az) = (d[best.0].0 - d[best.1].0, d[best.0].1 - d[best.1].1);
        let am = (ax * ax + az * az).sqrt().max(1.0e-6);
        if (d[k].0 * ax / am + d[k].1 * az / am).abs() >= graze_cos {
            targets.insert(edges[k].1);
        }
    }

    targets.into_iter().collect()
}

/// Union-find root with path-halving.
fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Union two sets, keeping the lowest index as the representative (deterministic).
fn uf_union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (uf_find(parent, a), uf_find(parent, b));
    if ra != rb {
        parent[ra.max(rb)] = ra.min(rb);
    }
}

/// Merge coincident nodes in place. Two sources of coincidence get collapsed:
/// active edges shorter than [`MERGE_EDGE_LEN_M`] (degenerate segments — the
/// unstable direction that spikes the miter) and distinct active nodes within
/// [`MERGE_NODE_EPS_M`] (snap-welded duplicates that render as double-hubs /
/// parallel edges). Each cluster collapses to its lowest-index node; edges are
/// rewired to representatives, and self-loops / duplicate edges are deactivated.
///
/// Only `edge.start/end`, `edge.active` and `node.edges` change — positions are
/// untouched, and `extract_blocks` rebuilds its own adjacency from the active
/// edges, so the planar structure stays valid for the lot layer.
pub(crate) fn merge_coincident_nodes(graph: &mut RoadGraph) {
    let n = graph.nodes.len();
    let pos = |i: usize| {
        let p = graph.nodes[i].position;
        (p.x, p.y)
    };
    let mut parent: Vec<usize> = (0..n).collect();

    // Active degree, to tell curve samples (degree-2) from junctions (degree-3+).
    let mut deg = vec![0u32; n];
    for e in &graph.edges {
        if e.active {
            deg[e.start as usize] += 1;
            deg[e.end as usize] += 1;
        }
    }

    // 1. Collapse a short active edge when it is either a near-zero segment (the
    //    spike source) OR a short connector *between two junctions* (a double-
    //    hub). Real junctions are never within MERGE_NODE_EPS, while a real curve
    //    sample is degree-2, so this never collapses legitimate road geometry.
    for e in &graph.edges {
        if !e.active {
            continue;
        }
        let (s, t) = (e.start as usize, e.end as usize);
        let (a, b) = (pos(s), pos(t));
        let l = (a.0 - b.0).hypot(a.1 - b.1);
        let junction_pair = deg[s] >= 3 && deg[t] >= 3;
        if l < MERGE_EDGE_LEN_M || (junction_pair && l < MERGE_NODE_EPS_M) {
            uf_union(&mut parent, s, t);
        }
    }

    // 2. Merge near-duplicate active nodes that are NOT directly connected by an
    //    edge (grid-bucketed, O(n)). Skipping adjacent pairs is load-bearing: the
    //    tensor graph is sampled at ~1 m, so merging adjacent samples would
    //    collapse and distort real curves — those are left to the near-zero rule
    //    above. Only genuine snap-welded duplicates (two *distinct* roads meeting
    //    at the same point) are merged here.
    let mut is_active = vec![false; n];
    let mut adjacent: std::collections::HashSet<(usize, usize)> = Default::default();
    for e in &graph.edges {
        if e.active {
            let (s, t) = (e.start as usize, e.end as usize);
            is_active[s] = true;
            is_active[t] = true;
            adjacent.insert((s.min(t), s.max(t)));
        }
    }
    let cell = MERGE_NODE_EPS_M.max(1.0e-3);
    let key = |p: (f32, f32)| ((p.0 / cell).floor() as i32, (p.1 / cell).floor() as i32);
    let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = Default::default();
    for (i, &active) in is_active.iter().enumerate() {
        if !active {
            continue;
        }
        let p = pos(i);
        let (kx, kz) = key(p);
        for dx in -1..=1 {
            for dz in -1..=1 {
                if let Some(bucket) = grid.get(&(kx + dx, kz + dz)) {
                    for &j in bucket {
                        let q = pos(j);
                        if (p.0 - q.0).hypot(p.1 - q.1) < MERGE_NODE_EPS_M
                            && !adjacent.contains(&(i.min(j), i.max(j)))
                        {
                            uf_union(&mut parent, i, j);
                        }
                    }
                }
            }
        }
        grid.entry((kx, kz)).or_default().push(i);
    }

    // 3. Rewire edges to representatives; drop self-loops and parallels.
    let mut seen: std::collections::HashSet<(usize, usize)> = Default::default();
    for e in &mut graph.edges {
        if !e.active {
            continue;
        }
        let ns = uf_find(&mut parent, e.start as usize);
        let ne = uf_find(&mut parent, e.end as usize);
        if ns == ne || !seen.insert((ns.min(ne), ns.max(ne))) {
            e.active = false;
            continue;
        }
        e.start = ns as u32;
        e.end = ne as u32;
    }

    // 4. Rebuild `node.edges` from the surviving active edges (order-agnostic;
    //    consumers that read it re-derive any angular order they need).
    let incidence: Vec<(usize, u32)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| e.active)
        .flat_map(|(i, e)| [(e.start as usize, i as u32), (e.end as usize, i as u32)])
        .collect();
    for node in &mut graph.nodes {
        node.edges.clear();
    }
    for (nid, eid) in incidence {
        graph.nodes[nid].edges.push(eid);
    }
}

// --- Endpoint-to-edge weld (#583) -------------------------------------------
//
// The tracer welds a junction only where a trace passes within `snap_radius`
// (~4 m) of an existing edge; a road that ends just beyond that is left as a
// free degree-1 dead-end touching another road's flank — so the mesher caps it
// as a cul-de-sac (#579) instead of meeting the junction. We close that gap by
// splitting the touched edge at the foot-of-perpendicular and welding the
// endpoint in, creating a real (degree-3) junction the hub builder then renders.
// Purely additive — the opposite of the subtractive stub/graze cuts.

/// Active adjacency as `(neighbour, edge_id)` per node, built from the `active`
/// edge flags — NOT `node.edges`, which [`sanitize_targets`] leaves carrying stale
/// ids after a cut. Shared by the endpoint-weld search (#583).
pub(crate) fn active_adjacency(graph: &RoadGraph) -> Vec<Vec<(usize, usize)>> {
    let mut adj = vec![Vec::new(); graph.nodes.len()];
    for (ei, e) in graph.edges.iter().enumerate() {
        if e.active {
            adj[e.start as usize].push((e.end as usize, ei));
            adj[e.end as usize].push((e.start as usize, ei));
        }
    }
    adj
}

/// Edge ids on the chain a degree-1 node `p` belongs to: walk outward through
/// degree-2 nodes from `p`'s single neighbour to the first junction / dead-end /
/// ring. A dead-end must never weld onto its OWN road (a hairpin curling back near
/// its shaft), so these edges are excluded as weld targets (#583).
fn weld_self_chain(adj: &[Vec<(usize, usize)>], p: usize) -> std::collections::HashSet<usize> {
    let mut edges = std::collections::HashSet::new();
    if adj[p].len() != 1 {
        return edges;
    }
    let (mut cur, mut e) = adj[p][0];
    edges.insert(e);
    while cur != p && adj[cur].len() == 2 {
        match adj[cur].iter().find(|&&(_, ne)| ne != e) {
            Some(&(nn, ne)) => {
                edges.insert(ne);
                cur = nn;
                e = ne;
            }
            None => break,
        }
    }
    edges
}

/// The best edge for a degree-1 dead-end `p` to weld onto (#583), or `None`: the
/// nearest active, non-incident, non-self-chain edge whose foot-of-perpendicular
/// from `p` lies strictly interior (≥ [`WELD_T_MARGIN`] from each end), within
/// `tol` metres, and meets `p`'s heading transversely (crossing angle
/// ≥ [`WELD_MIN_CROSS_ANGLE_DEG`] — not a near-parallel graze). Returns
/// `(edge_id, t)` with `t` the parametric foot along the edge; the caller
/// reconstructs the split point from the edge's own endpoints so the `glam` `Vec2`
/// type never crosses this boundary. Planar (XZ) — the mesher re-drapes elevation.
/// Deterministic: ties break to the nearest foot, then the lowest edge id.
pub(crate) fn weld_candidate(
    graph: &RoadGraph,
    adj: &[Vec<(usize, usize)>],
    p: usize,
    tol: f32,
) -> Option<(usize, f32)> {
    if adj[p].len() != 1 {
        return None;
    }
    let nb = adj[p][0].0;
    let pe = graph.nodes[p].position;
    // Heading toward the dead end; degenerate (coincident) shaft → no weld.
    let arm = (pe - graph.nodes[nb].position).normalize_or_zero();
    if arm.length_squared() < 0.5 {
        return None;
    }
    let cos_min = WELD_MIN_CROSS_ANGLE_DEG.to_radians().cos();
    let self_chain = weld_self_chain(adj, p);
    let mut best: Option<(f32, usize, f32)> = None; // (foot distance, edge id, t)
    for (ei, e) in graph.edges.iter().enumerate() {
        if !e.active {
            continue;
        }
        let (s, t_node) = (e.start as usize, e.end as usize);
        if s == p || t_node == p || self_chain.contains(&ei) {
            continue;
        }
        let a = graph.nodes[s].position;
        let ab = graph.nodes[t_node].position - a;
        let len2 = ab.length_squared();
        if len2 < 1.0e-6 {
            continue;
        }
        let t = (pe - a).dot(ab) / len2;
        if t <= WELD_T_MARGIN || t >= 1.0 - WELD_T_MARGIN {
            continue; // near an endpoint → merge_coincident_nodes' job, not a T
        }
        let d = (pe - (a + ab * t)).length();
        if d >= tol {
            continue;
        }
        // Transverse-crossing gate: reject a near-parallel graze (the road runs
        // alongside the edge rather than ending into it).
        if arm.dot(ab / len2.sqrt()).abs() > cos_min {
            continue;
        }
        if best.is_none_or(|(bd, bei, _)| (d, ei) < (bd, bei)) {
            best = Some((d, ei, t));
        }
    }
    best.map(|(_, ei, t)| (ei, t))
}

/// Weld every degree-1 dead-end that ends within `tol` of a non-incident edge into
/// a real junction (#583): split the touched edge at the foot-of-perpendicular and
/// connect the dead-end to the new node, so the hub builder renders a junction
/// instead of the mesher capping a cul-de-sac. Returns the number of welds applied.
///
/// Candidates are chosen against a FROZEN snapshot of the active graph, so the
/// result is independent of application order (deterministic). A planned weld whose
/// target edge a prior weld this pass already split is skipped — its dead-end is
/// reconsidered on the next sanitation pass against the new geometry. Welds only
/// ever raise a node's degree, never create a degree-1 node, so the candidate set
/// strictly shrinks and the enclosing fixed-point loop terminates.
pub(crate) fn weld_endpoint_dangles(graph: &mut RoadGraph, tol: f32) -> usize {
    let adj = active_adjacency(graph);
    // Plan against the frozen snapshot, in node order; carry the dead-end's own
    // road type for the connector edge.
    let mut plans: Vec<(usize, usize, f32, RoadType)> = Vec::new();
    for (p, edges) in adj.iter().enumerate() {
        if edges.len() != 1 {
            continue;
        }
        if let Some((ei, t)) = weld_candidate(graph, &adj, p, tol) {
            plans.push((p, ei, t, graph.edges[edges[0].1].road_type));
        }
    }
    let mut welded = 0;
    for (p, ei, t, road_type) in plans {
        if !graph.edges[ei].active {
            continue; // a prior weld this pass already split this edge
        }
        // Reconstruct the split point from the edge's own endpoints, so the `glam`
        // `Vec2` type stays internal to the graph (and `split_edge` re-derives the
        // same `t` from the distance ratio, since the foot is exactly on the edge).
        let a = graph.nodes[graph.edges[ei].start as usize].position;
        let b = graph.nodes[graph.edges[ei].end as usize].position;
        let foot = a + (b - a) * t;
        let (mid, _, _) = graph.split_edge(ei as u32, foot);
        graph.add_edge(p as u32, mid, road_type);
        welded += 1;
    }
    welded
}

#[cfg(test)]
mod tests;
