use bevy_symbios_ground::HeightMap;
use symbios_tensor::{RoadGraph, RoadType};

use crate::urban::{Dims, ROAD_INTERIOR_FRACTION};

/// One continuous road run (between intersections / endpoints), as an ordered
/// XZ polyline plus the deck half-width for its road class. `end_nodes` are the
/// graph node ids at `pts[0]` / `pts[last]`, so a chain end abutting a junction
/// can be recorded for the hub builder.
pub(crate) struct Chain {
    pub(crate) pts: Vec<(f32, f32)>,
    pub(crate) half_w: f32,
    pub(crate) end_nodes: [usize; 2],
    /// Per-end boundary-clip markers (index matches `end_nodes`). `true` when the
    /// run was cut at the district-interior boundary because the next sampled
    /// node fell *outside* — i.e. a road running off the network perimeter, which
    /// leaves an open cross-section and must be capped like a dead-end (#582). A
    /// genuine graph terminus is `false` here: a degree-1 dead-end is capped by
    /// degree (#579) and a loop closure / used-edge break stays open.
    pub(crate) clip: [bool; 2],
}

// --- Chain extraction -------------------------------------------------------

/// Split the planar graph into continuous chains: runs of degree-2 nodes
/// between intersections / endpoints, clipped to the district interior. Walks
/// the public adjacency by node degree — no dependency on tensor internals.
pub(crate) fn extract_chains(graph: &RoadGraph, hm: &HeightMap, dims: &Dims) -> Vec<Chain> {
    let center = hm.width() as f32 * hm.scale() * 0.5;
    let interior_r2 = (center * ROAD_INTERIOR_FRACTION).powi(2);
    let inside = |x: f32, z: f32| {
        let (dx, dz) = (x - center, z - center);
        dx * dx + dz * dz <= interior_r2
    };
    let pos = |i: usize| {
        let p = graph.nodes[i].position;
        (p.x, p.y)
    };
    let half_w = |ei: usize| match &graph.edges[ei].road_type {
        RoadType::Major => dims.major_half_width,
        RoadType::Minor => dims.minor_half_width,
    };

    let n = graph.nodes.len();
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (ei, e) in graph.edges.iter().enumerate() {
        if !e.active {
            continue;
        }
        adj[e.start as usize].push((e.end as usize, ei));
        adj[e.end as usize].push((e.start as usize, ei));
    }
    let mut used = vec![false; graph.edges.len()];
    let mut chains = Vec::new();

    // Chains anchored at intersections / endpoints (degree != 2).
    for s in 0..n {
        if adj[s].len() == 2 {
            continue;
        }
        for (nb, ei) in adj[s].clone() {
            if used[ei] {
                continue;
            }
            let nodes = walk_chain(&adj, &mut used, s, ei, nb);
            push_interior_runs(&nodes, &pos, &inside, half_w(ei), &mut chains);
        }
    }
    // Pure loops (every node degree 2) — start anywhere on an unused edge.
    for ei in 0..graph.edges.len() {
        if used[ei] || !graph.edges[ei].active {
            continue;
        }
        let e = &graph.edges[ei];
        let nodes = walk_chain(&adj, &mut used, e.start as usize, ei, e.end as usize);
        push_interior_runs(&nodes, &pos, &inside, half_w(ei), &mut chains);
    }
    chains
}

/// Follow degree-2 nodes from `start` (via edge `ei` to `nb`) until an
/// intersection / endpoint or an already-used edge, returning the node ids.
fn walk_chain(
    adj: &[Vec<(usize, usize)>],
    used: &mut [bool],
    start: usize,
    mut ei: usize,
    mut nb: usize,
) -> Vec<usize> {
    let mut nodes = vec![start];
    loop {
        used[ei] = true;
        nodes.push(nb);
        if adj[nb].len() != 2 {
            break;
        }
        // The other edge incident to nb.
        let next = adj[nb].iter().find(|&&(_, e)| e != ei).copied();
        match next {
            Some((nn, ne)) if !used[ne] => {
                ei = ne;
                nb = nn;
            }
            _ => break,
        }
    }
    nodes
}

/// Split a node-id run into maximal interior sub-runs (≥2 nodes) and push each
/// as a [`Chain`] of XZ positions, so streets terminate at the district edge.
pub(crate) fn push_interior_runs(
    nodes: &[usize],
    pos: &impl Fn(usize) -> (f32, f32),
    inside: &impl Fn(f32, f32) -> bool,
    half_w: f32,
    out: &mut Vec<Chain>,
) {
    let mut run: Vec<(usize, f32, f32)> = Vec::new();
    // Clip provenance: a run *starts* on a clip when the node just before it fell
    // outside (the street entered the interior from beyond the rim), and *ends* on
    // a clip when it is flushed because the next node is outside (it runs back off
    // the rim). A run flushed at the end of `nodes` reached the walked chain's real
    // terminus (dead-end / junction / loop end), so that end is not a clip.
    let mut run_start_clip = false;
    let mut prev_outside = false;
    let flush = |run: &mut Vec<(usize, f32, f32)>,
                 out: &mut Vec<Chain>,
                 start_clip: bool,
                 end_clip: bool| {
        if run.len() >= 2 {
            let end_nodes = [run[0].0, run[run.len() - 1].0];
            let pts = run.iter().map(|&(_, x, z)| (x, z)).collect();
            out.push(Chain {
                pts,
                half_w,
                end_nodes,
                clip: [start_clip, end_clip],
            });
        }
        run.clear();
    };
    for &nd in nodes {
        let (x, z) = pos(nd);
        if inside(x, z) {
            if run.is_empty() {
                run_start_clip = prev_outside;
            }
            run.push((nd, x, z));
            prev_outside = false;
        } else {
            // Boundary crossing: the open run (if any) ends at the district edge.
            flush(&mut run, out, run_start_clip, true);
            prev_outside = true;
        }
    }
    // Trailing flush: the run reached the walked chain's real terminus, not a clip.
    flush(&mut run, out, run_start_clip, false);
}

#[cfg(test)]
mod tests;
