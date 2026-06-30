use bevy_symbios_ground::HeightMap;
use symbios_tensor::RoadGraph;

use crate::pds::generator::RoadConfig;
use crate::urban::graph::{
    WELD_TOL_FRACTION, active_adjacency, build_road_graph_raw, sanitize_graph, weld_candidate,
};
use crate::urban::{
    ChainSample, Dims, RIBBON_STEP_M, ROAD_INTERIOR_FRACTION, compute_truncations, densify,
    extract_chains, frame_right, junction_mouth_spreads, level_chain, level_network, sample_chain,
};

// --- Diagnostics ------------------------------------------------------------
//
// A no-render dump of the *meshed* road graph's topology + geometry-risk, to
// guide road-network data filtering (the spurious junctions, dead-end spurs,
// near-duplicate nodes and tight-bend spikes seen in-game all originate in raw
// tracer/rationalize output that the mesher consumes with no clean-up pass).
// Surfaced through the render harness's `--road-dump <seed|did>`.
//
// The classification thresholds below are *reporting* heuristics — the dump
// also prints the raw distributions so they can be retuned against real seeds
// before any filter is baked into generation.

/// Node pair closer than this (m) counts as near-coincident (lumpy double-hubs).
const DIAG_NEAR_NODE_EPS_M: f32 = 1.0;
/// A dead-end edge shorter than this (m) counts as a stub (degree-inflating).
const DIAG_STUB_LEN_M: f32 = 8.0;
/// Two branches within this of 180° count as a straight through-road.
const DIAG_COLLINEAR_TOL_DEG: f32 = 25.0;
/// A third branch within this of the through-line counts as a glancing graze.
const DIAG_SHALLOW_ANGLE_DEG: f32 = 20.0;
/// Miter scale at/above this (the builder clamps at 3.0) marks a spike-risk bend.
const DIAG_SPIKE_SCALE: f32 = 2.5;
/// Tolerances (m) the diagnostic sweeps when counting near-miss dead-ends (#583
/// weld candidates), to size the candidate population before pinning the weld
/// tolerance below the next parallel street.
const NEAR_MISS_SWEEP_M: [f32; 3] = [4.0, 6.0, 8.0];

/// Topology + geometry-risk statistics for one room's *meshed* road graph,
/// gathered by [`road_graph_diagnostics`]. Purely diagnostic — nothing here
/// feeds generation; it exists to size the filtering work.
pub struct RoadGraphStats {
    nodes: usize,
    edges_total: usize,
    edges_active: usize,
    degree_hist: std::collections::BTreeMap<usize, usize>,
    dead_ends_total: usize,
    short_stubs: usize,
    spur_lengths: Vec<f32>,
    coincident_pairs: usize,
    hubs_total: usize,
    hubs_with_stub: usize,
    hubs_collinear_graze: usize,
    hubs_near_duplicate: usize,
    hubs_spurious: usize,
    hub_min_branch_angle: Vec<f32>,
    truncation_dists: Vec<f32>,
    /// #584 per-junction incident-mouth height spread (m): natural (each road
    /// levelled alone) vs network-levelled (mouths pinned flat). The levelled
    /// spread collapses toward 0 as junctions go flat.
    junction_spread_raw: Vec<f32>,
    junction_spread_level: Vec<f32>,
    chains: usize,
    chain_lengths: Vec<f32>,
    /// Chain-end disposition, mirroring how the mesher closes each end:
    /// `[junction, dead_end, clip, open]`. junction = degree ≥ 3 (closed by a
    /// hub); dead_end = degree 1 (capped, #579); clip = boundary clip running off
    /// the perimeter (capped, #582); open = the residue (degree-2 loop closure /
    /// used-edge break) deliberately left open. `open` is the count of still-open
    /// cross-sections — only genuine interior seams should remain here.
    chain_end_class: [usize; 4],
    /// Near-miss dead-ends (degree-1 nodes that would weld onto a non-incident
    /// edge) at each [`NEAR_MISS_SWEEP_M`] tolerance — the #583 weld-candidate
    /// population, used to size the weld tolerance.
    near_miss_dangles: [usize; 3],
    densified_vertices: usize,
    spike_vertices: usize,
    spike_max_scale: f32,
}

/// `min / p50 / p90 / max / mean` of a sample, or `—` when empty.
fn distro(v: &[f32]) -> String {
    if v.is_empty() {
        return "—".to_string();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pick = |q: f32| s[(((s.len() - 1) as f32) * q).round() as usize];
    let mean = s.iter().sum::<f32>() / s.len() as f32;
    format!(
        "min {:.1}  p50 {:.1}  p90 {:.1}  max {:.1}  mean {:.1}",
        s[0],
        pick(0.5),
        pick(0.9),
        s[s.len() - 1],
        mean
    )
}

/// Count unordered node pairs within `eps` that are **not** directly joined by
/// an active edge — genuine near-duplicate vertices the topology should merge,
/// excluding legitimately close chain-adjacent nodes (consecutive fillet
/// points). `pts` holds `(node_index, position)`; grid-bucketed, O(n).
fn count_near_duplicate_nodes(
    pts: &[(usize, (f32, f32))],
    eps: f32,
    adj: &[Vec<(usize, f32)>],
) -> usize {
    use std::collections::HashMap;
    let cell = eps.max(1.0e-3);
    let key = |p: (f32, f32)| ((p.0 / cell).floor() as i32, (p.1 / cell).floor() as i32);
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let mut count = 0usize;
    for (slot, &(ni, p)) in pts.iter().enumerate() {
        let (kx, kz) = key(p);
        for dx in -1..=1 {
            for dz in -1..=1 {
                if let Some(bucket) = grid.get(&(kx + dx, kz + dz)) {
                    for &other in bucket {
                        let (nj, q) = pts[other];
                        if (p.0 - q.0).hypot(p.1 - q.1) < eps
                            && !adj[ni].iter().any(|&(nb, _)| nb == nj)
                        {
                            count += 1;
                        }
                    }
                }
            }
        }
        grid.entry((kx, kz)).or_default().push(slot);
    }
    count
}

/// Before/after sanitation diagnostics for a room's road graph (raw rationalized
/// vs [`sanitize_graph`]-cleaned), for the render harness's `--road-dump`.
/// `None` when the network is disabled or the tracer produces nothing.
pub fn road_graph_diagnostics(hm: &HeightMap, config: &RoadConfig) -> Option<RoadDiagnostics> {
    let dims = Dims::from_config(config);
    let (graph_raw, sub, _lo) = build_road_graph_raw(hm, config)?;
    let raw = collect_graph_stats(&graph_raw, &sub, &dims);
    // Sanitise a fresh raw build (deterministic, so byte-identical to `graph_raw`).
    let (mut graph_san, sub2, _lo2) = build_road_graph_raw(hm, config)?;
    sanitize_graph(&mut graph_san, WELD_TOL_FRACTION * config.minor_spacing.0);
    let sanitized = collect_graph_stats(&graph_san, &sub2, &dims);
    Some(RoadDiagnostics { raw, sanitized })
}

/// Gather topology + geometry-risk stats for one graph — the exact one
/// [`crate::urban::build_road_geometry`] would mesh from `sub`.
fn collect_graph_stats(graph: &RoadGraph, sub: &HeightMap, dims: &Dims) -> RoadGraphStats {
    let pos = |i: usize| {
        let p = graph.nodes[i].position;
        (p.x, p.y)
    };
    let dist = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);

    let n = graph.nodes.len();
    // Active adjacency: (neighbour, edge length).
    let mut adj: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
    let mut edges_active = 0usize;
    for e in &graph.edges {
        if !e.active {
            continue;
        }
        edges_active += 1;
        let (s, t) = (e.start as usize, e.end as usize);
        let l = dist(pos(s), pos(t));
        adj[s].push((t, l));
        adj[t].push((s, l));
    }
    let degree: Vec<usize> = adj.iter().map(Vec::len).collect();

    let mut degree_hist: std::collections::BTreeMap<usize, usize> = Default::default();
    for &d in &degree {
        *degree_hist.entry(d).or_default() += 1;
    }

    // Interior clip — matches `extrude_junctions`' emission gate, so the hub
    // counts reflect what is actually rendered.
    let center = sub.width() as f32 * sub.scale() * 0.5;
    let interior_r2 = (center * ROAD_INTERIOR_FRACTION).powi(2);
    let inside = |i: usize| {
        let (x, z) = pos(i);
        let (dx, dz) = (x - center, z - center);
        dx * dx + dz * dz <= interior_r2
    };

    // Dead-end spurs (active degree 1).
    let spur_lengths: Vec<f32> = (0..n)
        .filter(|&i| degree[i] == 1)
        .map(|i| adj[i][0].1)
        .collect();
    let dead_ends_total = spur_lengths.len();
    let short_stubs = spur_lengths
        .iter()
        .filter(|&&l| l < DIAG_STUB_LEN_M)
        .count();

    let active_nodes: Vec<(usize, (f32, f32))> = (0..n)
        .filter(|&i| degree[i] > 0)
        .map(|i| (i, pos(i)))
        .collect();
    let coincident_pairs = count_near_duplicate_nodes(&active_nodes, DIAG_NEAR_NODE_EPS_M, &adj);

    // Hubs = rendered fans = active degree ≥ 3 inside the district interior.
    let hub_ids: Vec<usize> = (0..n).filter(|&i| degree[i] >= 3 && inside(i)).collect();
    let hubs_total = hub_ids.len();

    let collinear_cos = (180.0 - DIAG_COLLINEAR_TOL_DEG).to_radians().cos();
    let shallow_cos = DIAG_SHALLOW_ANGLE_DEG.to_radians().cos();

    let (mut hubs_with_stub, mut hubs_collinear_graze, mut hubs_near_duplicate, mut hubs_spurious) =
        (0usize, 0usize, 0usize, 0usize);
    let mut hub_min_branch_angle: Vec<f32> = Vec::with_capacity(hubs_total);

    for &h in &hub_ids {
        let hp = pos(h);
        let mut dirs: Vec<(f32, f32)> = Vec::with_capacity(adj[h].len());
        let mut has_stub = false;
        for &(nb, l) in &adj[h] {
            let np = pos(nb);
            let (dx, dz) = (np.0 - hp.0, np.1 - hp.1);
            let m = (dx * dx + dz * dz).sqrt().max(1.0e-6);
            dirs.push((dx / m, dz / m));
            if degree[nb] == 1 && l < DIAG_STUB_LEN_M {
                has_stub = true;
            }
        }

        // Smallest angle between any two branches — a tiny value means two
        // roads graze almost tangentially (a false crossing).
        let mut min_ang = 180.0_f32;
        for a in 0..dirs.len() {
            for b in (a + 1)..dirs.len() {
                let c = (dirs[a].0 * dirs[b].0 + dirs[a].1 * dirs[b].1).clamp(-1.0, 1.0);
                min_ang = min_ang.min(c.acos().to_degrees());
            }
        }
        hub_min_branch_angle.push(min_ang);

        // Degree-3 "through-road + glancing spur": two branches near-collinear,
        // the third nearly parallel to that through-line.
        let mut collinear_graze = false;
        if dirs.len() == 3 {
            let mut best = (0usize, 1usize, 1.0_f32);
            for a in 0..3 {
                for b in (a + 1)..3 {
                    let c = dirs[a].0 * dirs[b].0 + dirs[a].1 * dirs[b].1;
                    if c < best.2 {
                        best = (a, b, c);
                    }
                }
            }
            if best.2 <= collinear_cos {
                let k = 3 - best.0 - best.1;
                let (ax, az) = (
                    dirs[best.0].0 - dirs[best.1].0,
                    dirs[best.0].1 - dirs[best.1].1,
                );
                let am = (ax * ax + az * az).sqrt().max(1.0e-6);
                let c = (dirs[k].0 * ax / am + dirs[k].1 * az / am).abs();
                collinear_graze = c >= shallow_cos;
            }
        }

        let near_dup = hub_ids
            .iter()
            .any(|&o| o != h && dist(hp, pos(o)) < DIAG_NEAR_NODE_EPS_M);

        hubs_with_stub += usize::from(has_stub);
        hubs_collinear_graze += usize::from(collinear_graze);
        hubs_near_duplicate += usize::from(near_dup);
        hubs_spurious += usize::from(has_stub || collinear_graze || near_dup);
    }

    // Chains + spike risk, via the *exact* mesher paths.
    let chains = extract_chains(graph, sub, dims);
    // Per-arm junction pull-back (#575) — the same truncation the mesher applies,
    // so the dump reports how far each ribbon retreats into its hub.
    let trims = compute_truncations(&chains, |nd| degree[nd] >= 3, dims);
    let truncation_dists: Vec<f32> = trims
        .iter()
        .flatten()
        .copied()
        .filter(|&t| t > 0.0)
        .collect();
    // #584 junction levelling: per-junction incident-mouth height SPREAD, natural
    // (each road levelled independently) vs network-levelled (mouths pinned to one
    // height) — the spread collapses toward 0 as junctions go flat.
    let degree_u32: Vec<u32> = degree.iter().map(|&d| d as u32).collect();
    let samples: Vec<Option<ChainSample>> = chains
        .iter()
        .zip(&trims)
        .map(|(c, &[s, e])| sample_chain(c, s, e, sub))
        .collect();
    let natural_by: Vec<Vec<f32>> = samples
        .iter()
        .map(|s| match s {
            Some(s) => {
                let floor: Vec<f32> = s.frames.iter().map(|r| r.floor).collect();
                level_chain(&floor, &s.seg, [None, None])
            }
            None => Vec::new(),
        })
        .collect();
    let levelled_by = level_network(&chains, &samples, &degree_u32, sub);
    let is_junction = |nd: usize| degree.get(nd).copied().unwrap_or(0) >= 3;
    let junction_spread_raw = junction_mouth_spreads(&chains, &natural_by, &is_junction);
    let junction_spread_level = junction_mouth_spreads(&chains, &levelled_by, &is_junction);
    let mut chain_lengths: Vec<f32> = Vec::with_capacity(chains.len());
    let (mut densified_vertices, mut spike_vertices, mut spike_max_scale) =
        (0usize, 0usize, 0.0_f32);
    // Classify each chain end the way the mesher closes it (hub / cap / open).
    let mut chain_end_class = [0usize; 4];
    for chain in &chains {
        for (slot, &nd) in chain.end_nodes.iter().enumerate() {
            let bucket = if degree.get(nd).copied().unwrap_or(0) >= 3 {
                0 // junction → hub
            } else if degree.get(nd).copied().unwrap_or(0) == 1 {
                1 // dead-end → cap (#579)
            } else if chain.clip[slot] {
                2 // perimeter clip → cap (#582)
            } else {
                3 // loop closure / used-edge break → left open
            };
            chain_end_class[bucket] += 1;
        }
        let len = chain
            .pts
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
            .sum();
        chain_lengths.push(len);
        let pts = densify(&chain.pts, RIBBON_STEP_M);
        densified_vertices += pts.len();
        for i in 1..pts.len().saturating_sub(1) {
            let (_, _, scale) = frame_right(&pts, i);
            spike_max_scale = spike_max_scale.max(scale);
            spike_vertices += usize::from(scale >= DIAG_SPIKE_SCALE);
        }
    }

    // #583 weld-candidate population: degree-1 dead-ends that would weld onto a
    // non-incident edge at each sweep tolerance (monotone in the tolerance).
    let weld_adj = active_adjacency(graph);
    let mut near_miss_dangles = [0usize; 3];
    for (p, edges) in weld_adj.iter().enumerate() {
        if edges.len() != 1 {
            continue;
        }
        for (i, &tol) in NEAR_MISS_SWEEP_M.iter().enumerate() {
            if weld_candidate(graph, &weld_adj, p, tol).is_some() {
                near_miss_dangles[i] += 1;
            }
        }
    }

    RoadGraphStats {
        nodes: n,
        edges_total: graph.edges.len(),
        edges_active,
        degree_hist,
        dead_ends_total,
        short_stubs,
        spur_lengths,
        coincident_pairs,
        hubs_total,
        hubs_with_stub,
        hubs_collinear_graze,
        hubs_near_duplicate,
        hubs_spurious,
        hub_min_branch_angle,
        truncation_dists,
        junction_spread_raw,
        junction_spread_level,
        chains: chains.len(),
        chain_lengths,
        chain_end_class,
        near_miss_dangles,
        densified_vertices,
        spike_vertices,
        spike_max_scale,
    }
}

impl RoadGraphStats {
    /// Render one graph's stats as a labelled report block.
    fn section(&self, title: &str) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(s, "-- {title} --");
        let _ = writeln!(
            s,
            "nodes: {}   edges: {} active / {} total",
            self.nodes, self.edges_active, self.edges_total
        );
        let _ = write!(s, "active-degree histogram:");
        for (d, c) in &self.degree_hist {
            let _ = write!(s, "  deg{d}:{c}");
        }
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "dead-end spurs: {} ({} shorter than {:.0} m)   spur length: {}",
            self.dead_ends_total,
            self.short_stubs,
            DIAG_STUB_LEN_M,
            distro(&self.spur_lengths)
        );
        let _ = writeln!(
            s,
            "near-duplicate node pairs (< {:.1} m, non-adjacent): {}",
            DIAG_NEAR_NODE_EPS_M, self.coincident_pairs
        );
        let _ = writeln!(
            s,
            "-- junctions (rendered hubs: active-degree >= 3, interior) --"
        );
        let _ = writeln!(s, "total hubs: {}", self.hubs_total);
        let _ = writeln!(
            s,
            "  spurious: {}  [stub-induced {}, collinear-graze {}, near-duplicate {}]",
            self.hubs_spurious,
            self.hubs_with_stub,
            self.hubs_collinear_graze,
            self.hubs_near_duplicate
        );
        let _ = writeln!(
            s,
            "  real (survive gate): {}",
            self.hubs_total.saturating_sub(self.hubs_spurious)
        );
        let _ = writeln!(
            s,
            "  min branch-angle per hub (deg): {}",
            distro(&self.hub_min_branch_angle)
        );
        let _ = writeln!(
            s,
            "  truncation pull-back per arm (m): {}   (arms truncated: {})",
            distro(&self.truncation_dists),
            self.truncation_dists.len()
        );
        let _ = writeln!(
            s,
            "  junction mouth-height spread (m, #584): natural {}  ->  levelled {}",
            distro(&self.junction_spread_raw),
            distro(&self.junction_spread_level)
        );
        let _ = writeln!(s, "-- ribbon / spike risk --");
        let _ = writeln!(
            s,
            "chains: {}   chain length (m): {}",
            self.chains,
            distro(&self.chain_lengths)
        );
        let [j, d, c, o] = self.chain_end_class;
        let _ = writeln!(
            s,
            "chain ends: {} junction(hub)  {} dead-end(cap)  {} perimeter-clip(cap, #582)  {} open(loop/break)",
            j, d, c, o
        );
        let [m0, m1, m2] = self.near_miss_dangles;
        let [s0, s1, s2] = NEAR_MISS_SWEEP_M;
        let _ = writeln!(
            s,
            "near-miss dead-ends (#583 weld candidates) within {s0:.0}/{s1:.0}/{s2:.0} m: {m0} / {m1} / {m2}",
        );
        let _ = writeln!(
            s,
            "densified vertices: {}   spike-risk (miter scale >= {:.1}): {}   max miter scale: {:.2}",
            self.densified_vertices, DIAG_SPIKE_SCALE, self.spike_vertices, self.spike_max_scale
        );
        s
    }
}

/// Raw-vs-sanitised road-graph diagnostics — the before/after the
/// [`sanitize_graph`] pass achieves, surfaced by [`road_graph_diagnostics`].
pub struct RoadDiagnostics {
    raw: RoadGraphStats,
    sanitized: RoadGraphStats,
}

impl RoadDiagnostics {
    /// A before/after report block for the CLI dump: the raw rationalized graph,
    /// the sanitised graph, and the headline deltas sanitation achieved.
    pub fn report(&self, label: &str) -> String {
        use std::fmt::Write;
        let (r, c) = (&self.raw, &self.sanitized);
        let mut s = String::new();
        let _ = writeln!(s, "=== road-graph diagnostics — room {label} ===");
        let _ = write!(
            s,
            "{}",
            r.section("RAW (generate_roads + rationalize_graph)")
        );
        let _ = writeln!(s);
        let _ = write!(s, "{}", c.section("SANITIZED (+ sanitize_graph)"));
        let _ = writeln!(s, "-- sanitation delta --");
        let _ = writeln!(
            s,
            "spurious hubs {} -> {} (-{})   spike-risk verts {} -> {} (-{})",
            r.hubs_spurious,
            c.hubs_spurious,
            r.hubs_spurious.saturating_sub(c.hubs_spurious),
            r.spike_vertices,
            c.spike_vertices,
            r.spike_vertices.saturating_sub(c.spike_vertices),
        );
        let _ = writeln!(
            s,
            "active edges {} -> {}   dead-ends {} -> {}   near-dup pairs {} -> {}",
            r.edges_active,
            c.edges_active,
            r.dead_ends_total,
            c.dead_ends_total,
            r.coincident_pairs,
            c.coincident_pairs,
        );
        s
    }
}

#[cfg(test)]
mod tests;
