//! District graph: the road network's planar topology, traced once and shared by
//! the mesher and the lot layer - so a building can only stand on a street the
//! player actually sees. The authored district window is copied into its own
//! sub-heightmap (never written back; nothing carves the terrain), traced by
//! `symbios-tensor`, then rationalized. The network's street field (#1556) -
//! smoothing, designer basis fields and keep-out discs, authored in room
//! metres - is moved into that window's frame for the trace
//! ([`tensor_config`]). Sanitation clears the tracer artefacts
//! the mesher would otherwise inherit: coincident nodes and near-zero segments
//! merge (miter spikes, double-hubs), stub / grazing-junction edges are cut
//! (#571) and near-miss dead-ends weld into junctions (#583), to a fixed point.
//! A network at layout revision 1 (#1558) is then tidied ([`tidy_graph`]):
//! cut to the drawn district, its junction clusters merged, its doubled
//! streets, loop streets, tiny loops and stubs dropped, each street given one
//! road class. Revision 0 stops at the sanitation, so every saved district
//! keeps the graph its lots were grown from.

use bevy_symbios_ground::HeightMap;
use symbios_tensor::{
    BasisField, KeepOut, RationalizeConfig, RoadGraph, RoadType, TensorConfig, generate_roads,
    rationalize_graph,
};

use crate::pds::generator::{RoadBasis, RoadConfig};
use crate::pds::types::Fp2;

/// The road network's rationalized planar graph for `config`, plus the district
/// sub-heightmap it was traced on and that window's lower cell index `lo`.
/// `None` when the network is disabled, the window is too small, or the tracer
/// can't produce a network. Deterministic in `config.seed` and, for a network
/// that avoids water, `water_level`. Never writes back to `hm` (the `sub` copy
/// is the only mutable surface, and nothing carves it).
///
/// `water_level` is the room's water line in world Y
/// ([`crate::world_builder::compile::room_water_level`]), `None` for a dry room. It is
/// read only when `config.avoid_water` is set (#1552).
///
/// Shared by [`crate::urban::build_road_geometry`] (the draped mesh) and
/// [`crate::urban::extract_building_lots`] (footprints) so both read the *same* graph - a
/// building can only sit on a street if it was placed from the geometry the
/// player actually sees.
pub(crate) fn build_road_graph(
    hm: &HeightMap,
    config: &RoadConfig,
    water_level: Option<f32>,
) -> Option<(RoadGraph, HeightMap, [usize; 2])> {
    let (mut graph, sub, lo) = build_road_graph_raw(hm, config, water_level)?;
    // Clean tracer / rationalize artefacts (grazing false junctions and dead-end
    // stubs) out of the topology, and weld near-miss dead-ends into junctions,
    // before it is meshed *or* lotted - see [`sanitize_graph`]. Both consumers read
    // the same cleaned graph. Weld tolerance is per-room (a fraction of spacing).
    sanitize_graph(&mut graph, WELD_TOL_FRACTION * config.minor_spacing.0);
    // Layout revision 1 (#1558) tidies what the sanitiser leaves: a network
    // saved before it (revision 0) keeps the graph its lots were grown by.
    if config.tidies_layout() {
        tidy_graph(&mut graph, &sub, config);
    }
    Some((graph, sub, lo))
}

/// The raw rationalized graph - `generate_roads` + `rationalize_graph`, *before*
/// [`sanitize_graph`]. Split out so the diagnostic dump can compare the graph
/// before and after sanitation (see [`crate::urban::road_graph_diagnostics`]).
pub(crate) fn build_road_graph_raw(
    hm: &HeightMap,
    config: &RoadConfig,
    water_level: Option<f32>,
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
    // window always stays fully inside the heightmap - pushing the centre past
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

    let cfg = tensor_config(hm, config, water_level, lo);
    let mut graph = generate_roads(&sub, &cfg).ok()?;
    // Rationalize for clean XZ geometry (RDP straighten + Bézier fillets). We
    // ignore its smoothed elevations and sample the real terrain when draping.
    rationalize_graph(&mut graph, &sub, &RationalizeConfig::default());
    Some((graph, sub, lo))
}

/// The offset from a district window's own frame to the room frame (#889,
/// #1556): window point `p` stands at room `p + shift`, for the window whose
/// lower cell is `lo` in `hm`. The road mesh draws window point `p` at
/// heightmap point `p + lo * scale`, and the terrain and its roads are drawn
/// `half` back on each axis so the room origin stands in their middle. Every
/// layer that crosses between the frames - the lots and street furniture out
/// of the window, the street field's centres into it - goes through this one
/// sum.
pub(crate) fn window_to_room_shift(hm: &HeightMap, lo: [usize; 2]) -> [f32; 2] {
    let scale = hm.scale();
    let half = hm.width().saturating_sub(1) as f32 * scale * 0.5;
    [lo[0] as f32 * scale - half, lo[1] as f32 * scale - half]
}

/// The tracer's grid angle (radians) for a compass bearing in degrees
/// (#1556). The tracer lays a grid's major roads along `(cos a, sin a)` in
/// its XZ plane; a bearing `b` clockwise from north (-Z) toward east (+X)
/// points along `(sin b, -cos b)`, and `(cos(b - 90), sin(b - 90))` is that
/// same vector, so `a = b - 90` degrees - a shift, not a trig call. The
/// district window is the room frame moved, never turned, so a bearing
/// holds in both. The bearing is folded into `[0, 180)` first
/// ([`RoadBasis::canonical_bearing`]), so the two spellings of one grid trace
/// the same bits.
pub(crate) fn grid_angle(bearing_deg: f32) -> f32 {
    (RoadBasis::canonical_bearing(bearing_deg) - 90.0).to_radians()
}

/// The tracer's settings for `config` over the district window whose lower
/// cell is `lo` in `hm`: the network's seed and spacings, the water line when
/// it avoids water, its style preset, and its street field (#1556) - every
/// basis field's and keep-out disc's centre moved from the room frame into
/// the window's (window = room - [`window_to_room_shift`]), radii and
/// strengths as authored. An untouched street field leaves the tracer's own
/// defaults - no smoothing, terrain weight 1, no basis field, no keep-out
/// disc - so a network that never set one traces exactly as before it
/// existed. A basis field of a kind this build does not know is left out.
pub(crate) fn tensor_config(
    hm: &HeightMap,
    config: &RoadConfig,
    water_level: Option<f32>,
    lo: [usize; 2],
) -> TensorConfig {
    let mut cfg = TensorConfig {
        seed: config.seed,
        major_road_dist: config.major_spacing.0,
        minor_road_dist: config.minor_spacing.0,
        ..TensorConfig::default()
    };
    // Streets stop at the shore (#1552): the tracer spawns no seed at or
    // below the water line and ends a trace whose next step would dip under
    // it. The district copy holds the terrain's own heights, which are
    // world Y (the terrain anchor sits at the origin), so the room's water
    // level passes straight through.
    if config.avoid_water
        && let Some(level) = water_level
    {
        cfg.water_level = level;
    }
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
    // The street field (#1556). The tracer works in the window's own frame,
    // (0, 0) at its first cell; the record speaks room metres, so each
    // centre moves by the inverse of the shift the lots come back out by.
    let shift = window_to_room_shift(hm, lo);
    let window = |c: Fp2| [c.0[0] - shift[0], c.0[1] - shift[1]];
    let field = &config.field;
    cfg.field.smoothing = field.smoothing.0;
    cfg.field.terrain_weight = field.terrain_weight.0;
    cfg.field.basis = field
        .basis
        .iter()
        .filter_map(|basis| match *basis {
            RoadBasis::Ring {
                center,
                radius,
                strength,
            } => Some(BasisField::Radial {
                center: window(center).into(),
                radius: radius.0,
                strength: strength.0,
            }),
            RoadBasis::Grid {
                center,
                bearing,
                radius,
                strength,
            } => Some(BasisField::Grid {
                center: window(center).into(),
                angle: grid_angle(bearing.0),
                radius: radius.0,
                strength: strength.0,
            }),
            // A kind from a newer client: the rest of the field still traces.
            RoadBasis::Unknown => None,
        })
        .collect();
    cfg.keep_out = field
        .keep_out
        .iter()
        .map(|disc| KeepOut {
            center: window(disc.center).into(),
            radius: disc.radius.0,
        })
        .collect();
    cfg
}

// --- Graph sanitation (#571) ------------------------------------------------
//
// The tensor tracer welds a junction wherever a trace passes within
// `snap_radius` of an existing edge, and leaves dead-end stubs where a trace
// runs out; `rationalize_graph` straightens and fillets but never cleans the
// *topology*. So the mesher inherits two artefacts the `--road-dump` diagnostic
// measured as dominant: grazing false junctions (~23 % of hubs) and short
// dead-end stubs. We clear both here by deactivating edges - the exact `active`
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
/// weld onto. Shallower than this the two roads run near-parallel - a graze, not a
/// junction - and are left alone. The additive counterpart to the #571 graze CUT:
/// that removes false junctions, this creates the missing true ones.
const WELD_MIN_CROSS_ANGLE_DEG: f32 = 25.0;
/// Weld tolerance as a fraction of the room's minor-road spacing (#583): a dead-end
/// whose perpendicular gap to a non-incident edge is under `fraction × minor_spacing`
/// welds into it. Per-room-relative so a dense room can't cross-weld the next street.
/// At 0.08 it is ≈7.5 m on the densest seeded room (94 m spacing) up to ≈14 m on the
/// sparsest, and ≥ 4 m even on the 55 m struct default - always well under spacing yet
/// at/above the tracer's 4 m snap radius (the sizing sweep showed the candidate count
/// is flat from 4–8 m on every road-growing seed, so the exact value isn't delicate).
pub(crate) const WELD_TOL_FRACTION: f32 = 0.08;

/// Clean the road graph in place and deterministically. First **merge**
/// coincident nodes (collapsing near-zero segments and near-duplicate vertices -
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
        // undone by them - the loop still converges.
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
/// active edges shorter than [`MERGE_EDGE_LEN_M`] (degenerate segments - the
/// unstable direction that spikes the miter) and distinct active nodes within
/// [`MERGE_NODE_EPS_M`] (snap-welded duplicates that render as double-hubs /
/// parallel edges). Each cluster collapses to its lowest-index node; edges are
/// rewired to representatives, and self-loops / duplicate edges are deactivated.
///
/// Only `edge.start/end`, `edge.active` and `node.edges` change - positions are
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
    //    collapse and distort real curves - those are left to the near-zero rule
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

// --- Graph tidy (#1558, layout revision 1) -----------------------------------
//
// What the sanitiser leaves that the owner saw as flaws in a dense city: a
// cluster of junctions a few metres apart where one belongs, two streets
// traced side by side a few metres apart (one thick band with a sliver
// down it), a tiny loop street, a short stub left where a trace ran out,
// and a street running out past the drawn district into nothing. Each is
// removed here - only for a network at layout revision 1 or later, since a
// changed graph changes the lots every saved district was grown from. Every
// step works on the active edges in index order, so the result is
// deterministic, and its own decisions are basic IEEE arithmetic on the
// traced positions (square roots, a cosine written out, no platform
// transcendental). The one exception is the faces the tiny-loop step reads
// from symbios-tensor's `extract_blocks`, which orders each node's streets
// by the platform's `atan2f`: two streets leaving a node at angles equal to
// the last bit are the one way two peers that traced the same graph could
// tidy it apart.
//
// A loop street is the tracer's, so it is judged on the plan as traced,
// before anything is cut: once the district edge has cut the streets
// leaving a real block's corners, the long way round that block reads as
// one street beside its short side, and a block with one junction left as
// a loop of its own. No bound can chain: a cluster of junctions is bounded
// in diameter, so a wide street on a dense plan merges the junctions of a
// junction, never a district. And the stub and doubled-street bounds the
// spacing sets are capped by the streets' own widths, so a sparse plan
// keeps its streets: a stub is never longer than four street widths, and
// two streets with room for a lot between their curbs are never a double.

/// Junctions joined by a street shorter than this many major outer
/// footprints (deck + curb + chamfer) merge into one ...
const TIDY_CLUSTER_FOOTPRINTS: f32 = 2.5;
/// ... but never by a street longer than this fraction of the minor
/// spacing: a street that long is the short side of a block, not the gap
/// between two junctions of one crossing. The cluster threshold is the
/// lesser of the two, and it bounds a merged cluster's diameter too. At
/// the default widths (10.3 m) only a minor spacing under 31 m brings it
/// down; at the editor's widest streets (21.6 m), one under 65 m.
const TIDY_CLUSTER_SPACING_FRACTION: f32 = 1.0 / 3.0;
/// A dead-end street shorter than this fraction of the minor spacing is a
/// stub ...
const TIDY_STUB_FRACTION: f32 = 0.5;
/// ... and never one longer than this many major outer footprints (four
/// street widths, 33 m at the default widths): on a sparse plan half the
/// minor spacing is a street running to the district's edge, not a stub.
const TIDY_STUB_FOOTPRINTS: f32 = 8.0;
/// A face of the street plan smaller than this fraction of a nominal block
/// (major × minor spacing) is a tiny loop.
const TIDY_TINY_BLOCK_FRACTION: f32 = 0.05;
/// A loop street leaving a junction and coming back to it goes when it
/// closes less than this fraction of a nominal block: a ring the tracer
/// spun round a degenerate point, not a ring road with room for lots.
const TIDY_RING_BLOCK_FRACTION: f32 = 0.25;
/// A second street between the same two junctions goes when it is at least
/// this many times as long as the shortest one: a detour looping out and
/// back (the owner's loop, 112 m against 33 m), not the far side of a thin
/// block (the crescents a curved field traces run under 2.7).
const TIDY_LOOP_DETOUR_RATIO: f32 = 3.0;
/// Two streets whose centrelines run closer than this fraction of the minor
/// spacing - or than their outer footprints plus [`TIDY_PARALLEL_GAP_M`],
/// whichever is wider - read as one doubled street ...
const TIDY_PARALLEL_FRACTION: f32 = 0.25;
/// See [`TIDY_PARALLEL_FRACTION`].
const TIDY_PARALLEL_GAP_M: f32 = 2.0;
/// ... but never two streets this fraction of the minor spacing apart or
/// more: wide streets on a dense plan are neighbours, not a double ...
const TIDY_PARALLEL_SPACING_CAP: f32 = 0.5;
/// ... nor two with room for a lot between their curbs - its least side
/// and a sidewalk either side: on a sparse plan a quarter of the minor
/// spacing is a block's depth, not a sliver.
const TIDY_PARALLEL_LOT_ROOM_M: f32 =
    crate::urban::LOT_MIN_SIDE_M + 2.0 * crate::urban::LOT_STREET_MARGIN_M;
/// Two streets run side by side only within 15 degrees of parallel (a fork
/// opening wider is a junction, not a double): the cosine, written out so
/// no platform's `cosf` decides it.
const TIDY_PARALLEL_COS: f32 = 0.965_925_8;
/// ...and only over at least this length (m) ...
const TIDY_PARALLEL_MIN_RUN_M: f32 = 20.0;
/// ... and this fraction of the shorter of the two.
const TIDY_PARALLEL_RUN_FRACTION: f32 = 0.4;
/// Spacing (m) of the samples a street is compared to its neighbours at.
const TIDY_SAMPLE_M: f32 = 2.0;

/// One street of the graph between junctions or ends: its node path, its
/// edges, its length and whether it is mostly major.
struct GraphChain {
    nodes: Vec<usize>,
    edges: Vec<usize>,
    len: f32,
    major: bool,
}

/// Every street of the active graph: maximal runs through degree-2 nodes,
/// from each node of another degree in index order (edges in index order),
/// then every pure loop.
fn graph_chains(graph: &RoadGraph) -> Vec<GraphChain> {
    let mut adj = active_adjacency(graph);
    for a in &mut adj {
        a.sort_by_key(|&(_, e)| e);
    }
    let pos = |i: usize| graph.nodes[i].position;
    let mut used = vec![false; graph.edges.len()];
    let mut chains = Vec::new();
    let walk = |start: usize, first: (usize, usize), used: &mut [bool]| {
        let (mut cur, mut e) = first;
        let mut nodes = vec![start];
        let mut edges = Vec::new();
        loop {
            used[e] = true;
            edges.push(e);
            nodes.push(cur);
            if adj[cur].len() != 2 || cur == start {
                break;
            }
            match adj[cur].iter().find(|&&(_, ne)| ne != e) {
                Some(&(nn, ne)) if !used[ne] => {
                    cur = nn;
                    e = ne;
                }
                _ => break,
            }
        }
        let (mut len, mut major_len) = (0.0_f32, 0.0_f32);
        for (k, &ei) in edges.iter().enumerate() {
            let l = (pos(nodes[k + 1]) - pos(nodes[k])).length();
            len += l;
            if matches!(graph.edges[ei].road_type, RoadType::Major) {
                major_len += l;
            }
        }
        GraphChain {
            nodes,
            edges,
            len,
            major: major_len * 2.0 >= len,
        }
    };
    for (s, spokes) in adj.iter().enumerate() {
        if spokes.len() == 2 || spokes.is_empty() {
            continue;
        }
        for &first in spokes {
            if !used[first.1] {
                chains.push(walk(s, first, &mut used));
            }
        }
    }
    for ei in 0..graph.edges.len() {
        let e = &graph.edges[ei];
        if e.active && !used[ei] {
            chains.push(walk(e.start as usize, (e.end as usize, ei), &mut used));
        }
    }
    chains
}

/// Deactivate every edge of `chain`.
fn drop_chain(graph: &mut RoadGraph, chain: &GraphChain) {
    for &e in &chain.edges {
        graph.edges[e].active = false;
    }
}

/// The tidy's bounds for one network (#1558), each set by its spacing as
/// well as its widths.
struct TidyBounds {
    /// Junctions closer than this (m) along a street, and within it of
    /// every other junction of their cluster, merge.
    cluster_len: f32,
    /// A dead end shorter than this (m) is a stub.
    stub_len: f32,
    /// A face smaller than this (m2) is a tiny loop.
    tiny_area: f32,
    /// A loop street closing less than this (m2) is a spun ring.
    ring_area: f32,
    major_outer: f32,
    minor_outer: f32,
    minor_spacing: f32,
}

impl TidyBounds {
    fn new(config: &RoadConfig) -> Self {
        let outer = config.curb_top_width.0 + config.chamfer_width.0;
        let major_outer = config.major_half_width.0 + outer;
        let (major, minor) = (config.major_spacing.0, config.minor_spacing.0);
        Self {
            cluster_len: (TIDY_CLUSTER_FOOTPRINTS * major_outer)
                .min(TIDY_CLUSTER_SPACING_FRACTION * minor),
            stub_len: (TIDY_STUB_FRACTION * minor).min(TIDY_STUB_FOOTPRINTS * major_outer),
            tiny_area: TIDY_TINY_BLOCK_FRACTION * major * minor,
            ring_area: TIDY_RING_BLOCK_FRACTION * major * minor,
            major_outer,
            minor_outer: config.minor_half_width.0 + outer,
            minor_spacing: minor,
        }
    }

    /// How close (m) two streets' centrelines run before they read as one
    /// doubled street, by whether each is major.
    fn parallel(&self, a_major: bool, b_major: bool) -> f32 {
        let wo = |m: bool| {
            if m {
                self.major_outer
            } else {
                self.minor_outer
            }
        };
        (TIDY_PARALLEL_FRACTION * self.minor_spacing)
            .max(wo(a_major) + wo(b_major) + TIDY_PARALLEL_GAP_M)
            .min(TIDY_PARALLEL_SPACING_CAP * self.minor_spacing)
            .min(wo(a_major) + wo(b_major) + TIDY_PARALLEL_LOT_ROOM_M)
    }
}

/// Tidy the sanitised street graph for layout revision 1 (#1558): drop the
/// tracer's loop streets, judged on the plan as traced; cut the plan to the
/// drawn district; merge each cluster of junctions once; then drop doubled
/// streets, tiny loops and stubs until a pass drops nothing, and give every
/// street one road class. Node positions move only where a cluster merges.
///
/// The loop streets go first, and once, because the cut and the removals
/// make loops of real blocks: a block whose corners lose their streets to
/// the district edge or to the stub rule reads as one street round three
/// of its sides beside the fourth, or as a loop at its one junction left.
/// The tidy cuts no graze either: the sanitiser has cut every graze the
/// tracer left, and one the merge or a removal makes is a real street
/// leaving a junction at a sharp angle, often a block's side.
///
/// The removals reach a fixed point by construction, with no pass cap in
/// the derivation: every one of them only deactivates edges, so a pass that
/// changes anything removes at least one and there are at most as many
/// passes as edges. The merge runs once, before them, because it is the one
/// step that moves a node: run again it could chain a merged junction on to
/// its next neighbour.
pub(crate) fn tidy_graph(graph: &mut RoadGraph, sub: &HeightMap, config: &RoadConfig) {
    let bounds = TidyBounds::new(config);
    drop_loop_streets(graph, bounds.ring_area);
    clip_to_district(graph, sub);
    merge_junction_clusters(graph, bounds.cluster_len);
    while tidy_pass(graph, &bounds) {}
    unify_street_types(graph);
    rebuild_incidence(graph);
}

/// One pass of the tidy's removals; whether it removed anything.
fn tidy_pass(graph: &mut RoadGraph, bounds: &TidyBounds) -> bool {
    let mut changed = drop_doubled_streets(graph, &|a, b| bounds.parallel(a, b));
    changed |= drop_tiny_loops(graph, bounds.tiny_area);
    changed |= drop_stubs(graph, bounds.stub_len);
    changed
}

/// Give every street one road class - the one most of its length is - so
/// it is drawn, and its lots cleared, at one width along its whole length:
/// where a major and a minor trace met mid-street the graph alternates
/// classes along it, and the mesher draws a street at its first edge's.
fn unify_street_types(graph: &mut RoadGraph) {
    for chain in graph_chains(graph) {
        let road_type = if chain.major {
            RoadType::Major
        } else {
            RoadType::Minor
        };
        for &e in &chain.edges {
            graph.edges[e].road_type = road_type;
        }
    }
}

/// Deactivate every edge with an end outside the drawn district interior
/// (the circle the mesher clips to), so no street of a tidied plan runs on
/// into nothing past the district's edge and no block is closed by a street
/// nobody sees.
fn clip_to_district(graph: &mut RoadGraph, sub: &HeightMap) {
    let center = sub.width() as f32 * sub.scale() * 0.5;
    let r2 = (center * crate::urban::ROAD_INTERIOR_FRACTION).powi(2);
    let inside: Vec<bool> = graph
        .nodes
        .iter()
        .map(|n| {
            let (dx, dz) = (n.position.x - center, n.position.y - center);
            dx * dx + dz * dz <= r2
        })
        .collect();
    for e in &mut graph.edges {
        if e.active && !(inside[e.start as usize] && inside[e.end as usize]) {
            e.active = false;
        }
    }
}

/// Merge junctions (active degree ≥ 3) joined by streets shorter than
/// `cluster_len` into one junction at their mean position: the short
/// streets go, the lowest-numbered node of each set stands for it, and
/// every other street that met the set meets it there. The streets are
/// taken shortest first, and one joins two sets only while every junction
/// of the joined set stays within `cluster_len` of every other, so a
/// cluster cannot chain along a run of short streets: its diameter is
/// bounded, and so is how far any junction moves. A short street whose two
/// ends are already one set goes with it. Returns whether anything merged.
fn merge_junction_clusters(graph: &mut RoadGraph, cluster_len: f32) -> bool {
    let degree: Vec<usize> = active_adjacency(graph).iter().map(Vec::len).collect();
    let n = graph.nodes.len();
    let pos = |i: usize| {
        let p = graph.nodes[i].position;
        (p.x, p.y)
    };
    let mut short: Vec<GraphChain> = graph_chains(graph)
        .into_iter()
        .filter(|c| {
            let (a, b) = (c.nodes[0], *c.nodes.last().expect("a chain has nodes"));
            a != b && degree[a] >= 3 && degree[b] >= 3 && c.len < cluster_len
        })
        .collect();
    short.sort_by(|x, y| x.len.total_cmp(&y.len).then(x.edges[0].cmp(&y.edges[0])));
    let mut parent: Vec<usize> = (0..n).collect();
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let within = |a: &[usize], b: &[usize]| {
        a.iter().all(|&i| {
            b.iter().all(|&j| {
                let (p, q) = (pos(i), pos(j));
                let (dx, dz) = (q.0 - p.0, q.1 - p.1);
                (dx * dx + dz * dz).sqrt() <= cluster_len
            })
        })
    };
    let mut merged = Vec::new();
    for chain in &short {
        let (a, b) = (
            chain.nodes[0],
            *chain.nodes.last().expect("a chain has nodes"),
        );
        let (ra, rb) = (uf_find(&mut parent, a), uf_find(&mut parent, b));
        if ra != rb {
            if !within(&members[ra], &members[rb]) {
                continue;
            }
            let (keep, gone) = (ra.min(rb), ra.max(rb));
            parent[gone] = keep;
            let moved = std::mem::take(&mut members[gone]);
            members[keep].extend(moved);
        }
        merged.push(chain);
    }
    if merged.is_empty() {
        return false;
    }
    for chain in merged {
        drop_chain(graph, chain);
    }
    // Each set to its lowest node, at the set's mean position.
    let mut members: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..graph.nodes.len() {
        let r = uf_find(&mut parent, i);
        members.entry(r).or_default().push(i);
    }
    for (&r, set) in &members {
        if set.len() < 2 {
            continue;
        }
        let k = set.len() as f32;
        let (x, z) = set.iter().fold((0.0_f32, 0.0_f32), |(x, z), &i| {
            let p = graph.nodes[i].position;
            (x + p.x, z + p.y)
        });
        let p = &mut graph.nodes[r].position;
        p.x = x / k;
        p.y = z / k;
    }
    // Rewire to the representatives; a street now looping on itself or
    // doubling another between the same two nodes goes.
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
    true
}

/// Drop every loop street - one leaving a node and coming back to it - that
/// closes less than `ring_area`, and every second street between the same
/// two nodes at least [`TIDY_LOOP_DETOUR_RATIO`] times as long as the
/// shortest one. A thin block between two streets of like length (a
/// crescent) stays; [`drop_tiny_loops`] opens one only when it is tiny.
/// Run on the plan as traced only (see [`tidy_graph`]). Returns whether
/// anything went.
fn drop_loop_streets(graph: &mut RoadGraph, ring_area: f32) -> bool {
    let chains = graph_chains(graph);
    let mut by_ends: std::collections::BTreeMap<(usize, usize), Vec<usize>> = Default::default();
    let mut dropped = false;
    for (k, c) in chains.iter().enumerate() {
        let (a, b) = (c.nodes[0], *c.nodes.last().expect("a chain has nodes"));
        if a == b {
            if ring_area_of(graph, &c.nodes) < ring_area {
                drop_chain(graph, c);
                dropped = true;
            }
        } else {
            by_ends.entry((a.min(b), a.max(b))).or_default().push(k);
        }
    }
    for ks in by_ends.values() {
        if ks.len() < 2 {
            continue;
        }
        let keep = *ks
            .iter()
            .min_by(|&&x, &&y| {
                chains[x]
                    .len
                    .total_cmp(&chains[y].len)
                    .then(chains[x].edges[0].cmp(&chains[y].edges[0]))
            })
            .expect("two or more");
        for &k in ks {
            if k != keep && chains[k].len >= TIDY_LOOP_DETOUR_RATIO * chains[keep].len {
                drop_chain(graph, &chains[k]);
                dropped = true;
            }
        }
    }
    dropped
}

/// The area (m2) a closed node path rings (shoelace).
fn ring_area_of(graph: &RoadGraph, ring: &[usize]) -> f32 {
    ring.windows(2)
        .map(|w| {
            let (p, q) = (graph.nodes[w[0]].position, graph.nodes[w[1]].position);
            p.x * q.y - q.x * p.y
        })
        .sum::<f32>()
        .abs()
        * 0.5
}

/// Drop one of every two streets that run side by side - closer than
/// `parallel(a_major, b_major)`, within 15 degrees of parallel
/// ([`TIDY_PARALLEL_COS`]),
/// over at least [`TIDY_PARALLEL_MIN_RUN_M`] and
/// [`TIDY_PARALLEL_RUN_FRACTION`] of the shorter - keeping the major one,
/// else the older trace (its lowest edge number). A sample counts only where
/// its nearest point on the other street lies along it, not at its end, so a
/// street's own continuation through a junction is never its double.
/// Returns whether anything went.
fn drop_doubled_streets(graph: &mut RoadGraph, parallel: &impl Fn(bool, bool) -> f32) -> bool {
    let chains = graph_chains(graph);
    let pos = |i: usize| {
        let p = graph.nodes[i].position;
        [p.x, p.y]
    };
    let reach = parallel(true, true);
    let key = |p: [f32; 2]| ((p[0] / reach).floor() as i32, (p[1] / reach).floor() as i32);
    // Every segment, bucketed by the cells it passes.
    let mut grid: std::collections::HashMap<(i32, i32), Vec<(usize, usize)>> = Default::default();
    for (c, chain) in chains.iter().enumerate() {
        for k in 0..chain.nodes.len() - 1 {
            let (a, b) = (pos(chain.nodes[k]), pos(chain.nodes[k + 1]));
            let (ka, kb) = (key(a), key(b));
            for x in ka.0.min(kb.0)..=ka.0.max(kb.0) {
                for z in ka.1.min(kb.1)..=ka.1.max(kb.1) {
                    grid.entry((x, z)).or_default().push((c, k));
                }
            }
        }
    }
    let mut pairs: std::collections::BTreeSet<(usize, usize)> = Default::default();
    for (a, chain) in chains.iter().enumerate() {
        let mut run: std::collections::BTreeMap<usize, f32> = Default::default();
        for k in 0..chain.nodes.len() - 1 {
            let (p, q) = (pos(chain.nodes[k]), pos(chain.nodes[k + 1]));
            let d = [q[0] - p[0], q[1] - p[1]];
            let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
            if len < 1.0e-4 {
                continue;
            }
            let dir = [d[0] / len, d[1] / len];
            let steps = (len / TIDY_SAMPLE_M).ceil().max(1.0) as usize;
            let w = len / steps as f32;
            for s in 0..steps {
                let t = (s as f32 + 0.5) / steps as f32;
                let at = [p[0] + d[0] * t, p[1] + d[1] * t];
                // The nearest qualifying point on each other street.
                let mut best: std::collections::BTreeMap<usize, f32> = Default::default();
                let (kx, kz) = key(at);
                for x in kx - 1..=kx + 1 {
                    for z in kz - 1..=kz + 1 {
                        for &(b, j) in grid.get(&(x, z)).map(Vec::as_slice).unwrap_or(&[]) {
                            if b == a {
                                continue;
                            }
                            let other = &chains[b];
                            let (u, v) = (pos(other.nodes[j]), pos(other.nodes[j + 1]));
                            let e = [v[0] - u[0], v[1] - u[1]];
                            let el = (e[0] * e[0] + e[1] * e[1]).sqrt();
                            if el < 1.0e-4
                                || (dir[0] * e[0] + dir[1] * e[1]).abs() < TIDY_PARALLEL_COS * el
                            {
                                continue;
                            }
                            let f = (((at[0] - u[0]) * e[0] + (at[1] - u[1]) * e[1]) / (el * el))
                                .clamp(0.0, 1.0);
                            let near = [u[0] + e[0] * f, u[1] + e[1] * f];
                            let ends = [
                                pos(other.nodes[0]),
                                pos(*other.nodes.last().expect("nodes")),
                            ];
                            if ends.iter().any(|n| {
                                let (dx, dz) = (n[0] - near[0], n[1] - near[1]);
                                dx * dx + dz * dz < 1.0
                            }) {
                                continue; // its end, not alongside it
                            }
                            let (dx, dz) = (at[0] - near[0], at[1] - near[1]);
                            let dist = (dx * dx + dz * dz).sqrt();
                            if dist < parallel(chain.major, other.major) {
                                let slot = best.entry(b).or_insert(f32::MAX);
                                *slot = slot.min(dist);
                            }
                        }
                    }
                }
                for b in best.keys() {
                    *run.entry(*b).or_default() += w;
                }
            }
        }
        for (b, r) in run {
            let shorter = chain.len.min(chains[b].len);
            if r >= TIDY_PARALLEL_MIN_RUN_M.max(TIDY_PARALLEL_RUN_FRACTION * shorter) {
                pairs.insert((a.min(b), a.max(b)));
            }
        }
    }
    let mut gone = vec![false; chains.len()];
    for (a, b) in pairs {
        if gone[a] || gone[b] {
            continue;
        }
        let rank = |c: &GraphChain| {
            (
                !c.major,
                c.edges.iter().copied().min().unwrap_or(usize::MAX),
            )
        };
        let drop = if rank(&chains[a]) > rank(&chains[b]) {
            a
        } else {
            b
        };
        drop_chain(graph, &chains[drop]);
        gone[drop] = true;
    }
    gone.iter().any(|&g| g)
}

/// Drop one street from the boundary of every face of the plan smaller than
/// `tiny_area` - its longest minor street, else its longest - so the tiny
/// loop opens into the block beside it. Returns whether anything went.
fn drop_tiny_loops(graph: &mut RoadGraph, tiny_area: f32) -> bool {
    let mut faces = graph.clone();
    rebuild_incidence(&mut faces);
    symbios_tensor::extract_blocks(&mut faces);
    let chains = graph_chains(graph);
    let mut chain_of = vec![usize::MAX; graph.edges.len()];
    for (c, chain) in chains.iter().enumerate() {
        for &e in &chain.edges {
            chain_of[e] = c;
        }
    }
    let mut edge_of: std::collections::HashMap<(usize, usize), usize> = Default::default();
    for (ei, e) in graph.edges.iter().enumerate() {
        if e.active {
            let (s, t) = (e.start as usize, e.end as usize);
            edge_of.entry((s.min(t), s.max(t))).or_insert(ei);
        }
    }
    let mut gone: std::collections::BTreeSet<usize> = Default::default();
    for block in &faces.blocks {
        let ring = &block.perimeter;
        let n = ring.len();
        if n < 3 {
            continue;
        }
        let area = (0..n)
            .map(|i| {
                let (p, q) = (
                    graph.nodes[ring[i] as usize].position,
                    graph.nodes[ring[(i + 1) % n] as usize].position,
                );
                p.x * q.y - q.x * p.y
            })
            .sum::<f32>()
            .abs()
            * 0.5;
        if area >= tiny_area {
            continue;
        }
        let mut sides: Vec<usize> = (0..n)
            .filter_map(|i| {
                let (a, b) = (ring[i] as usize, ring[(i + 1) % n] as usize);
                edge_of.get(&(a.min(b), a.max(b))).map(|&e| chain_of[e])
            })
            .filter(|&c| c != usize::MAX)
            .collect();
        sides.sort_unstable();
        sides.dedup();
        if sides.iter().any(|c| gone.contains(c)) {
            continue; // already opened this pass
        }
        if let Some(&c) = sides.iter().max_by(|&&x, &&y| {
            (!chains[x].major)
                .cmp(&!chains[y].major)
                .then(chains[x].len.total_cmp(&chains[y].len))
                .then(y.cmp(&x))
        }) {
            gone.insert(c);
        }
    }
    for &c in &gone {
        drop_chain(graph, &chains[c]);
    }
    !gone.is_empty()
}

/// Drop every street shorter than `stub_len` that ends in nothing: a dead
/// end off a junction, or a lone piece. Returns whether anything went.
fn drop_stubs(graph: &mut RoadGraph, stub_len: f32) -> bool {
    let degree: Vec<usize> = active_adjacency(graph).iter().map(Vec::len).collect();
    let mut dropped = false;
    for chain in graph_chains(graph) {
        let (a, b) = (
            chain.nodes[0],
            *chain.nodes.last().expect("a chain has nodes"),
        );
        if (degree[a] == 1 || degree[b] == 1) && a != b && chain.len < stub_len {
            drop_chain(graph, &chain);
            dropped = true;
        }
    }
    dropped
}

/// Rebuild every node's edge list from the active edges.
fn rebuild_incidence(graph: &mut RoadGraph) {
    for node in &mut graph.nodes {
        node.edges.clear();
    }
    for i in 0..graph.edges.len() {
        if graph.edges[i].active {
            let (s, t) = (graph.edges[i].start as usize, graph.edges[i].end as usize);
            graph.nodes[s].edges.push(i as u32);
            graph.nodes[t].edges.push(i as u32);
        }
    }
}

// --- Endpoint-to-edge weld (#583) -------------------------------------------
//
// The tracer welds a junction only where a trace passes within `snap_radius`
// (~4 m) of an existing edge; a road that ends just beyond that is left as a
// free degree-1 dead-end touching another road's flank - so the mesher caps it
// as a cul-de-sac (#579) instead of meeting the junction. We close that gap by
// splitting the touched edge at the foot-of-perpendicular and welding the
// endpoint in, creating a real (degree-3) junction the hub builder then renders.
// Purely additive - the opposite of the subtractive stub/graze cuts.

/// Active adjacency as `(neighbour, edge_id)` per node, built from the `active`
/// edge flags - NOT `node.edges`, which [`sanitize_targets`] leaves carrying stale
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
/// ≥ [`WELD_MIN_CROSS_ANGLE_DEG`] - not a near-parallel graze). Returns
/// `(edge_id, t)` with `t` the parametric foot along the edge; the caller
/// reconstructs the split point from the edge's own endpoints so the `glam` `Vec2`
/// type never crosses this boundary. Planar (XZ) - the mesher re-drapes elevation.
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
/// target edge a prior weld this pass already split is skipped - its dead-end is
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
