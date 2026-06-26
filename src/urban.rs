//! Tensor-field urban layout — deterministic, terrain-conforming road networks
//! for urban-themed rooms (cyberpunk pilot).
//!
//! `symbios-tensor` is used purely as a road-**topology** generator: we take
//! its tensor-field [`RoadGraph`] and build our own road geometry that *drapes*
//! over overlands' existing terrain (sampling the heightmap per vertex), rather
//! than the crate's carve-and-bridge path — which regrades the heightmap into
//! flat road shelves and shatters the natural relief of our ~1 km rooms.
//! Nothing is carved here; the terrain stays natural and the road conforms to
//! its surface.
//!
//! The road is built by extracting continuous **chains** (runs of connected
//! nodes between intersections) from the graph and extruding a closed
//! cross-section profile along each, with **miter joins** at the bends (so
//! curves have no gaps) and continuous arc-length UVs (so a texture flows down
//! the street). The profile is a chamfered curb framing a flat deck, over a
//! deep skirt that buries into the terrain and is capped by a textured bottom
//! — so where the road runs out over a cliff edge it reads as a solid
//! retaining structure, not a hollow strip.
//!
//! Generation is localized to a district window around spawn (the seeded
//! settlement only reaches ~140 m) and clipped to the district interior so no
//! street runs off to the visible edge. Everything is deterministic in the
//! room's terrain seed and recomputed at load, never stored — like the
//! heightmap itself.
//!
//! `symbios-tensor` consumes a `symbios_ground::HeightMap`; overlands' own
//! [`bevy_symbios_ground::HeightMap`] is the same crate/type (unified by the
//! `[patch.crates-io]` on `symbios-ground` in `Cargo.toml`), so the heightmap
//! passes straight through with no conversion.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;
use symbios_tensor::{
    LotConfig, RationalizeConfig, RoadGraph, RoadType, TensorConfig, extract_blocks, extract_lots,
    generate_roads, rationalize_graph,
};

use crate::pds::generator::RoadConfig;

// --- Tuning -----------------------------------------------------------------
//
// The authorable knobs (district extent, road spacing/widths, curb + skirt
// dimensions, layout seed) live on [`RoadConfig`] in the room record. The
// constants below are pure *rendering* details with no gameplay/aesthetic
// reason to vary per room, so they stay in code.

/// Lift (m) of the deck above the sampled terrain — keeps the deck clear of the
/// ground and the curb framing it proud.
const ROAD_DEPTH_BIAS_M: f32 = 0.2;
/// Spacing (m) of ribbon cross-sections along a road. Straight edges are
/// subdivided to this so the deck still drapes over relief between graph nodes.
const RIBBON_STEP_M: f32 = 3.0;
/// Shortest ribbon (m) worth meshing after junction truncation (#575). A chain
/// trimmed below this at both ends sits entirely inside its hubs, so it grows no
/// ribbon — the hubs cover the gap — rather than a curb-framed sliver.
const MIN_RIBBON_LEN_M: f32 = 1.0;
/// Drop edges whose endpoints fall beyond this fraction of the district
/// half-extent, so the network ends in the interior, not at the visible edge.
const ROAD_INTERIOR_FRACTION: f32 = 0.88;
/// World metres per UV tile, both along the road and around the cross-section.
const UV_TILE_M: f32 = 6.0;
/// Width (m) of the emissive neon edge-line strip riding the inner curb top.
const NEON_LINE_WIDTH_M: f32 = 0.07;
/// Lift (m) of that strip above the curb top so it sits proud and never
/// z-fights the curb face it rides (see the coplanar-z-fight rule).
const NEON_LINE_LIFT_M: f32 = 0.04;

/// Resolved per-room road dimensions, pulled out of [`RoadConfig`]'s fixed-point
/// fields once so the geometry builders take plain `f32`s.
#[derive(Clone, Copy)]
struct Dims {
    major_half_width: f32,
    minor_half_width: f32,
    curb_height: f32,
    curb_top_width: f32,
    chamfer_width: f32,
    skirt_depth: f32,
}

impl Dims {
    fn from_config(c: &RoadConfig) -> Self {
        Self {
            major_half_width: c.major_half_width.0,
            minor_half_width: c.minor_half_width.0,
            curb_height: c.curb_height.0,
            curb_top_width: c.curb_top_width.0,
            chamfer_width: c.chamfer_width.0,
            skirt_depth: c.skirt_depth.0,
        }
    }
}

/// Engine-agnostic vertex buffers for one road *surface* (Y-up), built CPU-side
/// in the terrain task and uploaded by the caller. Ribbon strips carry normals
/// **smoothed along their length** so the deck reads as one continuous surface;
/// the crease *across* the profile (deck↔curb↔skirt) stays sharp because each
/// profile face is its own strip. Junction fans use the draped terrain normal.
#[derive(Default)]
pub struct RoadGeometry {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl RoadGeometry {
    /// True when no faces were emitted — the caller skips spawning a mesh.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Append one quad (corners `a,b,c,d`, wound `a→b→d→c`) with a shared flat
    /// `nrm` and the four corner UVs.
    fn push_quad(
        &mut self,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        d: [f32; 3],
        uvs: [[f32; 2]; 4],
        nrm: [f32; 3],
    ) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&[a, b, c, d]);
        self.uvs.extend_from_slice(&uvs);
        for _ in 0..4 {
            self.normals.push(nrm);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }

    /// Append one longitudinally-smoothed quad strip for a single profile face:
    /// `left[i]`/`right[i]` are the face's two edges at frame `i`, `seg_normals`
    /// (len `frames-1`) the flat outward normal of each segment. Each frame
    /// contributes a shared vertex pair carrying the **average** of its adjacent
    /// segment normals, so the strip shades smoothly along its length while
    /// remaining a hard crease against the neighbouring face (a separate strip).
    /// `uv_u` is the lateral U of the two edges; `v[i]` the along-road V.
    fn push_smoothed_strip(
        &mut self,
        left: &[[f32; 3]],
        right: &[[f32; 3]],
        seg_normals: &[[f32; 3]],
        uv_u: (f32, f32),
        v: &[f32],
    ) {
        let n = left.len();
        if n < 2 {
            return;
        }
        let base = self.vertices.len() as u32;
        for i in 0..n {
            // Average the (up to two) segment normals meeting at frame `i`.
            let mut acc = [0.0_f32; 3];
            for s in [i.checked_sub(1), (i < seg_normals.len()).then_some(i)]
                .into_iter()
                .flatten()
            {
                let nrm = seg_normals[s];
                acc = [acc[0] + nrm[0], acc[1] + nrm[1], acc[2] + nrm[2]];
            }
            let nrm = normalize(acc);
            self.vertices.push(left[i]);
            self.vertices.push(right[i]);
            self.normals.push(nrm);
            self.normals.push(nrm);
            self.uvs.push([uv_u.0, v[i]]);
            self.uvs.push([uv_u.1, v[i]]);
        }
        for i in 0..n - 1 {
            let a = base + (i as u32) * 2; // left[i]
            self.indices
                .extend_from_slice(&[a, a + 1, a + 3, a, a + 3, a + 2]);
        }
    }
}

/// The road split into its material surfaces, so the caller can give each the
/// look it needs — a dark wet-asphalt **deck**, a concrete/metal **structure**
/// (curb + skirt + bottom cap) and emissive neon **edge-lines** — without
/// stacking textures on the splat material (WebGL2's 16-sampler ceiling). Each
/// non-empty part is uploaded as its own mesh + material.
#[derive(Default)]
pub struct RoadParts {
    /// Flat drivable top surface plus the intersection fans.
    pub deck: RoadGeometry,
    /// Curb walls, chamfers, the deep skirt and its bottom cap.
    pub structure: RoadGeometry,
    /// Thin strips riding proud of each curb's inner top crease.
    pub neon: RoadGeometry,
}

/// One continuous road run (between intersections / endpoints), as an ordered
/// XZ polyline plus the deck half-width for its road class. `end_nodes` are the
/// graph node ids at `pts[0]` / `pts[last]`, so a chain end abutting a junction
/// can be recorded for the hub builder.
struct Chain {
    pts: Vec<(f32, f32)>,
    half_w: f32,
    end_nodes: [usize; 2],
}

/// The road network's rationalized planar graph for `config`, plus the district
/// sub-heightmap it was traced on and that window's lower cell index `lo`.
/// `None` when the network is disabled, the window is too small, or the tracer
/// can't produce a network. Deterministic in `config.seed`. Never writes back
/// to `hm` (the `sub` copy is the only mutable surface, and nothing carves it).
///
/// Shared by [`build_road_geometry`] (the draped mesh) and
/// [`extract_building_lots`] (footprints) so both read the *same* graph — a
/// building can only sit on a street if it was placed from the geometry the
/// player actually sees.
fn build_road_graph(hm: &HeightMap, config: &RoadConfig) -> Option<(RoadGraph, HeightMap, usize)> {
    let (mut graph, sub, lo) = build_road_graph_raw(hm, config)?;
    // Clean tracer / rationalize artefacts (grazing false junctions and dead-end
    // stubs) out of the topology before it is meshed *or* lotted — see
    // [`sanitize_graph`]. Both consumers read the same cleaned graph.
    sanitize_graph(&mut graph);
    Some((graph, sub, lo))
}

/// The raw rationalized graph — `generate_roads` + `rationalize_graph`, *before*
/// [`sanitize_graph`]. Split out so the diagnostic dump can compare the graph
/// before and after sanitation (see [`road_graph_diagnostics`]).
fn build_road_graph_raw(
    hm: &HeightMap,
    config: &RoadConfig,
) -> Option<(RoadGraph, HeightMap, usize)> {
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
    let lo = full_w / 2 - half_cells;

    // District window → its own heightmap, both for tensor to road and for us
    // to sample heights from. Copied, never written back (no carving).
    let mut sub = HeightMap::new(side, side, scale);
    for z in 0..side {
        for x in 0..side {
            sub.set(x, z, hm.get(lo + x, lo + z));
        }
    }

    let cfg = TensorConfig {
        seed: config.seed,
        major_road_dist: config.major_spacing.0,
        minor_road_dist: config.minor_spacing.0,
        ..TensorConfig::default()
    };
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

/// Clean the road graph in place and deterministically. First **merge**
/// coincident nodes (collapsing near-zero segments and near-duplicate vertices —
/// the source of the glitch spikes and lumpy double-hubs), then **cut** the
/// remaining stub / graze artefacts to a fixed point (a cut can expose a fresh
/// stub, and vice-versa, so passes repeat until one cuts nothing).
fn sanitize_graph(graph: &mut RoadGraph) {
    merge_coincident_nodes(graph);
    for _ in 0..SANITIZE_MAX_PASSES {
        let targets = sanitize_targets(graph);
        if targets.is_empty() {
            break;
        }
        for ei in targets {
            graph.edges[ei].active = false;
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
fn merge_coincident_nodes(graph: &mut RoadGraph) {
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

/// Build terrain-conforming road geometry from a [`RoadConfig`], or `None` if
/// the config is disabled or the tracer can't produce a network. Deterministic
/// in `config.seed`. Does **not** modify `hm` — the road drapes over the
/// natural terrain. Which rooms *get* a road config is the seeding layer's
/// policy ([`crate::pds::room`]); this just renders whatever it's handed.
pub fn build_road_geometry(hm: &HeightMap, config: &RoadConfig) -> Option<RoadParts> {
    let (graph, sub, lo) = build_road_graph(hm, config)?;
    let dims = Dims::from_config(config);
    let chains = extract_chains(&graph, &sub, &dims);

    // Active degree per node — distinguishes a junction end (≥3) from a mid-chain
    // / district-clip terminus, so only real intersections grow a hub.
    let mut degree = vec![0u32; graph.nodes.len()];
    for e in &graph.edges {
        if e.active {
            degree[e.start as usize] += 1;
            degree[e.end as usize] += 1;
        }
    }

    // Pull-back distance per chain end abutting a junction (active degree ≥ 3),
    // so each ribbon stops at the intersection boundary instead of overlapping
    // into the hub (#575). Computed once, ahead of extrusion.
    let trims = compute_truncations(
        &chains,
        |nd| degree.get(nd).copied().unwrap_or(0) >= 3,
        &dims,
    );

    let mut parts = RoadParts::default();
    let world_offset = lo as f32 * sub.scale();
    // Each chain extrudes its ribbon and records its end-frames at junctions, so
    // the hubs can be built to meet every incident road at its exact mouth.
    let mut road_ends: Vec<RoadEnd> = Vec::new();
    for (ci, chain) in chains.iter().enumerate() {
        let [start_trim, end_trim] = trims[ci];
        extrude_chain(
            chain,
            start_trim,
            end_trim,
            &sub,
            world_offset,
            &dims,
            &degree,
            &mut road_ends,
            &mut parts,
        );
    }
    extrude_hubs(&road_ends, &sub, world_offset, &dims, &mut parts);
    (!parts.deck.is_empty() || !parts.structure.is_empty()).then_some(parts)
}

/// A building footprint extracted from the road network's enclosed city blocks,
/// in the **room placement frame** — XZ centred on spawn, matching the road
/// mesh's `-half` spawn offset — so each maps straight onto a
/// [`Placement::Absolute`](crate::pds::generator::Placement) translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingLot {
    /// Footprint centre, room-centred XZ.
    pub position: [f32; 2],
    /// Yaw (radians, around +Y) aligning the footprint to its street frontage.
    pub yaw: f32,
    /// Frontage extent (m) along the street.
    pub width: f32,
    /// Depth (m) perpendicular to the street.
    pub depth: f32,
}

/// Extract building footprints from the road network's enclosed city blocks,
/// deterministic in `config.seed`. Footprints are returned in the room
/// placement frame and never carve `hm` (extraction uses
/// [`symbios_tensor::WaterPolicy::Skip`], which leaves the heightmap untouched).
/// Empty when the network is disabled, fails to trace, or encloses no blocks.
///
/// This is the seed for the lot-based building layer ([`crate::terrain`]'s
/// load-time populate-lots system): it shares [`build_road_graph`] with the
/// road mesh, so every footprint sits on a street the player can see.
pub fn extract_building_lots(hm: &HeightMap, config: &RoadConfig) -> Vec<BuildingLot> {
    let Some((mut graph, mut sub, lo)) = build_road_graph(hm, config) else {
        return Vec::new();
    };
    // Enclosed faces → blocks → recursively subdivided, street-aligned lots.
    extract_blocks(&mut graph);
    let lots = extract_lots(&graph, &mut sub, &LotConfig::default());

    // Sub-window XZ (origin at the window's lower corner) → room-centred frame:
    // the road mesh draws window coord `p` at world `p + lo*scale - half`, so a
    // footprint placed there lands exactly on its street.
    let scale = sub.scale();
    let half = hm.width().saturating_sub(1) as f32 * scale * 0.5;
    let shift = lo as f32 * scale - half;
    lots.into_iter()
        .map(|l| BuildingLot {
            position: [l.position.x + shift, l.position.y + shift],
            // tensor measures the lot's rotation in the XZ (top-down) plane;
            // placement yaw is around +Y, the opposite winding sense.
            yaw: -l.rotation,
            width: l.width,
            depth: l.depth,
        })
        .collect()
}

// --- Chain extraction -------------------------------------------------------

/// Split the planar graph into continuous chains: runs of degree-2 nodes
/// between intersections / endpoints, clipped to the district interior. Walks
/// the public adjacency by node degree — no dependency on tensor internals.
fn extract_chains(graph: &RoadGraph, hm: &HeightMap, dims: &Dims) -> Vec<Chain> {
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
fn push_interior_runs(
    nodes: &[usize],
    pos: &impl Fn(usize) -> (f32, f32),
    inside: &impl Fn(f32, f32) -> bool,
    half_w: f32,
    out: &mut Vec<Chain>,
) {
    let mut run: Vec<(usize, f32, f32)> = Vec::new();
    let flush = |run: &mut Vec<(usize, f32, f32)>, out: &mut Vec<Chain>| {
        if run.len() >= 2 {
            let end_nodes = [run[0].0, run[run.len() - 1].0];
            let pts = run.iter().map(|&(_, x, z)| (x, z)).collect();
            out.push(Chain {
                pts,
                half_w,
                end_nodes,
            });
        }
        run.clear();
    };
    for &nd in nodes {
        let (x, z) = pos(nd);
        if inside(x, z) {
            run.push((nd, x, z));
        } else {
            flush(&mut run, out);
        }
    }
    flush(&mut run, out);
}

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
const MAX_TRUNCATION_FACTOR: f32 = 4.0;

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
fn compute_truncations(
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
fn trim_polyline(pts: &[(f32, f32)], start_trim: f32, end_trim: f32) -> Vec<(f32, f32)> {
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

// --- Profile extrusion ------------------------------------------------------

/// The closed cross-section (lateral offset `u`, height `h` relative to the
/// deck top) for a deck of half-width `w`: flat deck, chamfered curb framing
/// each edge, and a deep skirt capped by a bottom face. Ten points, traced
/// around the solid; consecutive points (wrapping) are the profile's faces.
fn profile(w: f32, dims: &Dims) -> [(f32, f32); 10] {
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
fn frame_right(pts: &[(f32, f32)], i: usize) -> (f32, f32, f32) {
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
fn densify(pts: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
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

/// Lateral samples across the deck width for the upward-only height: the flat
/// deck is lifted to clear the MAX of these, so no part of the drivable surface
/// ever buries (the uphill edge sits flush, the downhill edge rides proud).
const DECK_SAMPLES: usize = 5;
/// Cap (rise/run) on the deck's *downhill* drop between frames — keeps the grade
/// gentle and lets the deck bridge dips as an embankment instead of diving in.
/// Up/down inclines are tolerable; only the lateral roll is engineered out.
const MAX_LONGITUDINAL_GRADE: f32 = 0.18;
/// How far (m) the skirt bottom sinks below the lower outer-edge terrain, so an
/// elevated (downhill) side always reads as a retaining wall meeting the ground.
const SKIRT_BURY_MARGIN_M: f32 = 0.3;

/// Per-vertex extrusion frame. The deck is **flat across** (no lateral banking,
/// so vehicles don't roll side-to-side) and drainage-correct: `base_y` is the
/// flat deck height — lifted to clear the highest terrain under the road and
/// longitudinally grade-limited — and `skirt_bottom_y` is where the skirt drops
/// to so an elevated side still meets the ground. `arc` is the running arc
/// length (for V UVs).
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
fn quad_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], axis: [f32; 3]) -> [f32; 3] {
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
/// over `hm` **flat-across and upward-only** (it never sinks below the terrain —
/// see [`Frame`]), shifted into the full-terrain frame by `world_offset`. The
/// drivable deck top, the structural curb/skirt and the emissive neon edge-lines
/// are routed to their respective [`RoadParts`] buffers.
#[allow(clippy::too_many_arguments)] // each arg is a distinct input/sink.
fn extrude_chain(
    chain: &Chain,
    start_trim: f32,
    end_trim: f32,
    hm: &HeightMap,
    world_offset: f32,
    dims: &Dims,
    degree: &[u32],
    road_ends: &mut Vec<RoadEnd>,
    parts: &mut RoadParts,
) {
    // Shorten the chain at any junction end (#575) so the ribbon stops at the hub
    // boundary, then drape the remaining run. A chain wholly consumed by its
    // hubs trims to nothing and grows no ribbon.
    let trimmed = trim_polyline(&chain.pts, start_trim, end_trim);
    let pts = densify(&trimmed, RIBBON_STEP_M);
    if pts.len() < 2 {
        return;
    }
    let prof = profile(chain.half_w, dims);
    let half_w = chain.half_w;
    let wo = half_w + dims.curb_top_width + dims.chamfer_width;

    // Pass A: per-frame geometry, the upward-only deck *floor* (a flat deck must
    // clear the highest terrain across its width) and the lowest outer-edge
    // terrain (how far the skirt must drop to meet the ground on an elevated side).
    struct Raw {
        cx: f32,
        cz: f32,
        rx: f32,
        rz: f32,
        scale: f32,
        arc: f32,
        floor: f32,
        ground: f32,
    }
    let mut raw: Vec<Raw> = Vec::with_capacity(pts.len());
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
        raw.push(Raw {
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

    // Longitudinal upward grade-limit: raise the deck so it never descends faster
    // than `MAX_LONGITUDINAL_GRADE` (a gentle grade that bridges dips), but never
    // below the floor (so the deck never buries). Two passes give symmetry both
    // ways along the chain.
    let mut base_y: Vec<f32> = raw.iter().map(|r| r.floor).collect();
    let seg: Vec<f32> = raw
        .windows(2)
        .map(|w| (w[1].cx - w[0].cx).hypot(w[1].cz - w[0].cz).max(1.0e-3))
        .collect();
    for i in 1..base_y.len() {
        base_y[i] = base_y[i].max(base_y[i - 1] - MAX_LONGITUDINAL_GRADE * seg[i - 1]);
    }
    for i in (0..base_y.len().saturating_sub(1)).rev() {
        base_y[i] = base_y[i].max(base_y[i + 1] - MAX_LONGITUDINAL_GRADE * seg[i]);
    }

    let frames: Vec<Frame> = raw
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let by = base_y[i];
            // Drop the skirt to the deeper of its fixed depth and just below the
            // outer ground, so an elevated downhill side still reaches terrain.
            let skirt_bottom_y = (by - dims.skirt_depth).min(r.ground - SKIRT_BURY_MARGIN_M);
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
}

/// One ribbon end abutting a junction node, recorded during chain extrusion so
/// the hub can meet each incident road at its exact mouth corners and deck
/// height (seamless, upward-only). All positions are in the sub-heightmap frame.
struct RoadEnd {
    node: usize,
    /// Truncated mouth centre (XZ): where the ribbon actually ends after #575.
    /// The hub fans from the centroid of all its mouth corners, so neither the
    /// node position nor the arm direction is needed here.
    cx: f32,
    cz: f32,
    /// Mouth-frame right axis and deck half-width — give the two mouth corners
    /// `(cx, cz) ± (rx, rz)·half_w`, which coincide with the ribbon's end edge.
    rx: f32,
    rz: f32,
    half_w: f32,
    deck_y: f32,
}

/// Build a real intersection hub at every junction (≥3 incident roads) from the
/// truncated ribbon ends (#576): a deck polygon whose mouth edges coincide with
/// each road's end cross-section (the deck flows in seamlessly at the road's own
/// height), its surface a **level-plane fit** to the incident mouth heights (the
/// apex sits at their mean, not the max — so it stays level instead of tenting),
/// kept upward-only, plus curb+skirt walls closing the angular gaps so the curb
/// runs round the corner and the hub meets the ground (#577 refines these into
/// curb-return arc fillets). Smooth-shaded; every deck triangle wound front-up.
fn extrude_hubs(
    road_ends: &[RoadEnd],
    hm: &HeightMap,
    world_offset: f32,
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
                    a.cx - a.rx * a.half_w + world_offset,
                    a.deck_y,
                    a.cz - a.rz * a.half_w + world_offset,
                ],
                ai,
            ));
            corners.push((
                [
                    a.cx + a.rx * a.half_w + world_offset,
                    a.deck_y,
                    a.cz + a.rz * a.half_w + world_offset,
                ],
                ai,
            ));
        }

        // Fan centre = the mouth corners' centroid (always inside their hull). A
        // node-anchored fan over arm-grouped corners self-intersects whenever the
        // per-arm truncations differ and the deck half-width is comparable to the
        // pull-back (the common case) — adjacent mouths splay past each other.
        // Sweeping the corners by angle around the centroid and fanning from it
        // tiles a SIMPLE polygon regardless. Level-plane fit: apex at the MEAN
        // incident deck height (the least-squares plane evaluated at the centroid
        // — no tent), kept upward-only.
        let (cx, cz) = (
            corners.iter().map(|(q, _)| q[0]).sum::<f32>() / corners.len() as f32,
            corners.iter().map(|(q, _)| q[2]).sum::<f32>() / corners.len() as f32,
        );
        let mean_y = arms.iter().map(|a| a.deck_y).sum::<f32>() / arms.len() as f32;
        let center_y =
            mean_y.max(hm.get_height_at(cx - world_offset, cz - world_offset) + ROAD_DEPTH_BIAS_M);
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

        // --- Gap walls: a polygon edge between corners of DIFFERENT arms is an
        //     exterior gap → curb + skirt down to ground so the curb runs round
        //     the corner and the hub grounds; an edge within ONE arm is a mouth →
        //     left open for the road. (#577 replaces these with curb-return arc
        //     fillets.) ---
        let (ct, cf) = (dims.curb_top_width, dims.chamfer_width);
        const GAP_SEG: usize = 3;
        for i in 0..p {
            let (l, la) = corners[i];
            let (r, ra) = corners[(i + 1) % p];
            if la == ra {
                continue; // a mouth edge: open for the road
            }
            // Ring at gap parameter t: (deck edge, curb top, skirt foot at ground).
            let ring = |t: f32| {
                let inner = [
                    l[0] + (r[0] - l[0]) * t,
                    l[1] + (r[1] - l[1]) * t,
                    l[2] + (r[2] - l[2]) * t,
                ];
                let (mut ox, mut oz) = (inner[0] - center[0], inner[2] - center[2]);
                let om = (ox * ox + oz * oz).sqrt().max(1.0e-6);
                ox /= om;
                oz /= om;
                let curb_top = [
                    inner[0] + ox * ct,
                    inner[1] + dims.curb_height,
                    inner[2] + oz * ct,
                ];
                let (fx, fz) = (inner[0] + ox * (ct + cf), inner[2] + oz * (ct + cf));
                // Skirt foot below the outer terrain, but never above the deck edge
                // it drops from — so the wall always descends, even where the gap
                // terrain humps up above the mouth grades.
                let fy = (hm.get_height_at(fx - world_offset, fz - world_offset)
                    - SKIRT_BURY_MARGIN_M)
                    .min(inner[1] - 1.0e-3);
                (inner, curb_top, [fx, fy, fz])
            };
            for s in 0..GAP_SEG {
                let (i0, c0, f0) = ring(s as f32 / GAP_SEG as f32);
                let (i1, c1, f1) = ring((s + 1) as f32 / GAP_SEG as f32);
                // Curb wall (deck edge → curb top), then skirt (curb top → foot).
                let n_curb = quad_normal(i0, c0, i1, c1, center);
                parts
                    .structure
                    .push_quad(i0, c0, i1, c1, [uv(i0), uv(c0), uv(i1), uv(c1)], n_curb);
                let n_skirt = quad_normal(c0, f0, c1, f1, center);
                parts.structure.push_quad(
                    c0,
                    f0,
                    c1,
                    f1,
                    [uv(c0), uv(f0), uv(c1), uv(f1)],
                    n_skirt,
                );
            }
        }
    }
}

/// Upward-facing flat normal of triangle `(c, a, b)`.
fn tri_up_normal(c: [f32; 3], a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let e1 = [a[0] - c[0], a[1] - c[1], a[2] - c[2]];
    let e2 = [b[0] - c[0], b[1] - c[1], b[2] - c[2]];
    let mut nn = cross(e1, e2);
    if nn[1] < 0.0 {
        nn = [-nn[0], -nn[1], -nn[2]];
    }
    normalize(nn)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l < 1.0e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

/// Convert [`RoadGeometry`] into a Bevy [`Mesh`].
pub fn to_bevy_mesh(geo: &RoadGeometry) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, geo.vertices.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, geo.normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, geo.uvs.clone());
    mesh.insert_indices(Indices::U32(geo.indices.clone()));
    mesh
}

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
    chains: usize,
    chain_lengths: Vec<f32>,
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
    sanitize_graph(&mut graph_san);
    let sanitized = collect_graph_stats(&graph_san, &sub2, &dims);
    Some(RoadDiagnostics { raw, sanitized })
}

/// Gather topology + geometry-risk stats for one graph — the exact one
/// [`build_road_geometry`] would mesh from `sub`.
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
    let truncation_dists: Vec<f32> = compute_truncations(&chains, |nd| degree[nd] >= 3, dims)
        .iter()
        .flatten()
        .copied()
        .filter(|&t| t > 0.0)
        .collect();
    let mut chain_lengths: Vec<f32> = Vec::with_capacity(chains.len());
    let (mut densified_vertices, mut spike_vertices, mut spike_max_scale) =
        (0usize, 0usize, 0.0_f32);
    for chain in &chains {
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
        chains: chains.len(),
        chain_lengths,
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
        let _ = writeln!(s, "-- ribbon / spike risk --");
        let _ = writeln!(
            s,
            "chains: {}   chain length (m): {}",
            self.chains,
            distro(&self.chain_lengths)
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
mod tests {
    use super::*;
    use bevy_symbios_ground::{FbmNoise, TerrainGenerator};

    /// A small heightmap with real slopes — tensor needs non-flat terrain for
    /// the major/minor directions to cross and enclose blocks.
    fn sloped_heightmap() -> HeightMap {
        let mut hm = HeightMap::new(64, 64, 4.0);
        FbmNoise {
            seed: 7,
            octaves: 5,
            persistence: 0.5,
            lacunarity: 2.0,
            base_frequency: 0.02,
        }
        .generate(&mut hm);
        hm.normalize();
        for v in hm.data_mut() {
            *v *= 40.0;
        }
        hm
    }

    /// A config with the given layout seed and otherwise-default dimensions.
    fn cfg(seed: u64) -> RoadConfig {
        RoadConfig {
            seed,
            ..RoadConfig::default()
        }
    }

    /// The three surface buffers, for tests that sweep every emitted vertex.
    fn surfaces(p: &RoadParts) -> [&RoadGeometry; 3] {
        [&p.deck, &p.structure, &p.neon]
    }

    #[test]
    fn default_config_actually_produces_a_network() {
        // Regression guard: the other tests tolerate `None`; this asserts the
        // shipped default config genuinely yields road geometry on sloped
        // terrain, so a config/clip change can't silently render nothing.
        let parts = build_road_geometry(&sloped_heightmap(), &cfg(7))
            .expect("default road config must produce a network on sloped terrain");
        assert!(!parts.deck.is_empty(), "no drivable deck");
        assert!(!parts.structure.is_empty(), "no curb/skirt structure");
        // The default curb has height, so the neon edge-line must be emitted.
        assert!(!parts.neon.is_empty(), "no neon curb edge-lining");
    }

    /// The pilot room's heightmap at real ~1 km scale (256², cyberpunk terrain
    /// seed) — big enough that the road network encloses real city blocks.
    fn pilot_heightmap() -> HeightMap {
        let mut hm = HeightMap::new(256, 256, 2.0);
        FbmNoise {
            seed: 4167901772298833237,
            octaves: 6,
            persistence: 0.5,
            lacunarity: 2.0,
            base_frequency: 0.012,
        }
        .generate(&mut hm);
        hm.normalize();
        for v in hm.data_mut() {
            *v *= 60.0;
        }
        hm
    }

    /// The pilot room's derived road seed (terrain seed ^ ROAD_SEED_SALT).
    const PILOT_ROAD_SEED: u64 = 4167901772298833237_u64 ^ 0xA0D5_EED5_A170_0001;

    #[test]
    fn produces_a_network_at_room_scale_for_the_pilot_seed() {
        // The pilot room at real scale + its derived road seed. Guards against
        // the windowed path yielding an empty network there.
        let parts = build_road_geometry(&pilot_heightmap(), &cfg(PILOT_ROAD_SEED))
            .expect("room-scale build for the pilot seed must produce roads");
        assert!(!parts.deck.is_empty());
    }

    #[test]
    fn extracts_building_lots_at_room_scale() {
        // The lot layer's load-bearing guard: the pilot network must enclose
        // blocks that subdivide into real footprints, all inside the district
        // window (room-centred) with positive, finite extents.
        let hm = pilot_heightmap();
        let lots = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED));
        assert!(!lots.is_empty(), "pilot network enclosed no building lots");

        let district = cfg(PILOT_ROAD_SEED).district_half_extent.0;
        for lot in &lots {
            assert!(
                lot.position.iter().all(|c| c.is_finite()),
                "non-finite lot position {:?}",
                lot.position
            );
            assert!(lot.yaw.is_finite() && lot.width > 0.0 && lot.depth > 0.0);
            // Footprints live inside the district window, centred on spawn.
            assert!(
                lot.position[0].abs() <= district + 1.0 && lot.position[1].abs() <= district + 1.0,
                "lot {:?} escaped the ±{district} m district window",
                lot.position
            );
        }
    }

    #[test]
    fn building_lots_are_deterministic() {
        // The bake-into-record contract needs lots reproducible from the seed,
        // so every peer deriving the same record lands identical footprints.
        let hm = pilot_heightmap();
        let a = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED));
        let b = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED));
        assert_eq!(a, b, "building lots non-deterministic for identical input");
    }

    #[test]
    fn disabled_network_extracts_no_lots() {
        let c = RoadConfig {
            enabled: false,
            ..cfg(PILOT_ROAD_SEED)
        };
        assert!(extract_building_lots(&pilot_heightmap(), &c).is_empty());
    }

    #[test]
    fn disabled_config_grows_no_roads() {
        let c = RoadConfig {
            enabled: false,
            ..cfg(7)
        };
        assert!(build_road_geometry(&sloped_heightmap(), &c).is_none());
    }

    /// The record-build ↔ client-render contract rests on the layout being
    /// deterministic from the seed: identical input must yield identical road
    /// geometry, vertex-for-vertex.
    #[test]
    fn road_geometry_is_deterministic() {
        let a = sloped_heightmap();
        let b = sloped_heightmap();
        match (
            build_road_geometry(&a, &cfg(7)),
            build_road_geometry(&b, &cfg(7)),
        ) {
            (Some(x), Some(y)) => {
                for (gx, gy) in surfaces(&x).into_iter().zip(surfaces(&y)) {
                    assert_eq!(gx.vertices, gy.vertices, "road geometry non-deterministic");
                    assert_eq!(gx.indices, gy.indices, "road topology non-deterministic");
                }
            }
            (None, None) => {}
            _ => panic!("road generation succeeded inconsistently for identical input"),
        }
    }

    /// Draping must not touch the terrain — the heightmap is rendered as-is.
    #[test]
    fn draping_leaves_the_heightmap_untouched() {
        let original = sloped_heightmap();
        let mut probe = sloped_heightmap();
        let _ = build_road_geometry(&probe, &cfg(7));
        assert_eq!(
            original.data(),
            probe.data_mut(),
            "build_road_geometry must not carve the terrain"
        );
    }

    /// The road-graph diagnostic must run on the pilot network and report
    /// internally-consistent counts — a guard for the filtering work that
    /// reads these numbers to size thresholds.
    #[test]
    fn road_graph_diagnostics_reports_consistent_stats() {
        let hm = pilot_heightmap();
        let dims = Dims::from_config(&cfg(PILOT_ROAD_SEED));
        let diag = road_graph_diagnostics(&hm, &cfg(PILOT_ROAD_SEED))
            .expect("pilot network must yield diagnostics");
        for stats in [&diag.raw, &diag.sanitized] {
            // The degree histogram partitions every node exactly once.
            let hist_sum: usize = stats.degree_hist.values().sum();
            assert_eq!(
                hist_sum, stats.nodes,
                "degree histogram must cover all nodes"
            );
            // Spurious-hub sub-counts are each a subset of all hubs.
            assert!(stats.hubs_spurious <= stats.hubs_total);
            assert!(stats.hubs_with_stub <= stats.hubs_total);
            assert!(stats.hubs_collinear_graze <= stats.hubs_total);
            assert!(stats.hubs_near_duplicate <= stats.hubs_total);
            // Spike-risk vertices are a subset of densified vertices, and the
            // builder's miter clamp (3.0) is never exceeded.
            assert!(stats.spike_vertices <= stats.densified_vertices);
            assert!(
                stats.spike_max_scale <= 3.0 + 1.0e-3,
                "miter scale clamp is 3.0"
            );
            // One sample per hub / per spur.
            assert_eq!(stats.hub_min_branch_angle.len(), stats.hubs_total);
            assert_eq!(stats.spur_lengths.len(), stats.dead_ends_total);
            // Every reported truncation is a finite, positive pull-back bounded
            // by the per-class cap (#575) — the dump can't render NaN or a
            // cap-escape from a degenerate fan.
            let trunc_cap = MAX_TRUNCATION_FACTOR
                * (dims.major_half_width + dims.curb_top_width + dims.chamfer_width);
            for &t in &stats.truncation_dists {
                assert!(t.is_finite() && t > 0.0, "bad truncation distance {t}");
                assert!(
                    t <= trunc_cap + 1.0e-3,
                    "truncation {t} exceeds the cap {trunc_cap}"
                );
            }
        }
        // WS1 acceptance: sanitation only ever removes the artefacts it targets —
        // never adds them — and the merge leaves no near-duplicate nodes behind.
        assert!(diag.sanitized.hubs_spurious <= diag.raw.hubs_spurious);
        assert!(diag.sanitized.spike_vertices <= diag.raw.spike_vertices);
        assert!(diag.sanitized.edges_active <= diag.raw.edges_active);
        assert_eq!(
            diag.sanitized.coincident_pairs, 0,
            "merge must leave no near-duplicate (non-adjacent) nodes"
        );
        // The report renders without panicking and labels the room.
        assert!(diag.report("pilot").contains("road-graph diagnostics"));
    }

    /// Every emitted vertex must be finite — a NaN from a degenerate miter or
    /// normalize would poison the mesh.
    #[test]
    fn geometry_is_finite() {
        if let Some(parts) = build_road_geometry(&sloped_heightmap(), &cfg(7)) {
            assert!(!parts.deck.is_empty());
            for geo in surfaces(&parts) {
                for v in &geo.vertices {
                    assert!(v.iter().all(|c| c.is_finite()), "non-finite vertex {v:?}");
                }
                for nrm in &geo.normals {
                    assert!(nrm.iter().all(|c| c.is_finite()), "non-finite normal");
                }
            }
        }
    }

    /// WS2: the ribbon is shaded smoothly. The deck strip welds its vertices
    /// along the chain (so adjacent quads share normals → no facet), which means
    /// far fewer vertices than the 4-per-quad an unwelded flat-shaded build would
    /// emit; and every normal is unit length (the smoothing's `normalize`).
    #[test]
    fn deck_is_welded_with_unit_normals() {
        let parts = build_road_geometry(&pilot_heightmap(), &cfg(PILOT_ROAD_SEED))
            .expect("pilot network must produce roads");
        let deck_quads = parts.deck.indices.len() / 6;
        assert!(deck_quads > 0, "no deck quads");
        assert!(
            parts.deck.vertices.len() < 4 * deck_quads,
            "deck is not welded ({} verts for {deck_quads} quads — flat per-face?)",
            parts.deck.vertices.len()
        );
        for geo in surfaces(&parts) {
            for nrm in &geo.normals {
                let len2 = nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2];
                assert!((len2 - 1.0).abs() < 1.0e-3, "non-unit normal {nrm:?}");
            }
        }
    }

    /// WS3: the drivable deck never sinks below the terrain. Every deck vertex
    /// sits at or above the ground beneath it — the upward-only drape. (Road
    /// geometry is authored in the heightmap frame, so `get_height_at` at the
    /// vertex XZ is the terrain under it.)
    #[test]
    fn deck_never_buries() {
        let hm = pilot_heightmap();
        let parts = build_road_geometry(&hm, &cfg(PILOT_ROAD_SEED))
            .expect("pilot network must produce roads");
        for v in &parts.deck.vertices {
            let ground = hm.get_height_at(v[0], v[2]);
            assert!(
                v[1] + 1.0e-3 >= ground,
                "deck vertex {v:?} buried below terrain {ground}"
            );
        }
    }

    /// #576 on the real pilot network (it carries acute junctions down to ~23°):
    /// every deck normal — ribbon *and* hub — faces up, so back-face culling
    /// keeps the drivable surface visible from above, and every vertex is finite.
    /// Guards against a folded / downward-wound hub fan on real data.
    #[test]
    fn pilot_deck_is_finite_and_faces_up() {
        let hm = pilot_heightmap();
        let parts = build_road_geometry(&hm, &cfg(PILOT_ROAD_SEED))
            .expect("pilot network must produce roads");
        for v in &parts.deck.vertices {
            assert!(
                v.iter().all(|c| c.is_finite()),
                "non-finite deck vertex {v:?}"
            );
        }
        for nrm in &parts.deck.normals {
            assert!(
                nrm.iter().all(|c| c.is_finite()) && nrm[1] > 0.0,
                "deck normal {nrm:?} not finite-and-upward",
            );
        }
    }

    /// WS4: a junction grows a real hub — a deck polygon meeting each incident
    /// road at its mouth (one centre + 2 corners per arm) plus curb/skirt walls
    /// closing the gaps — not the old circular fan.
    #[test]
    fn hub_meets_each_road_and_closes_gaps() {
        let dims = Dims::from_config(&cfg(7));
        // Three roads meeting at the origin at 0° / 120° / 240° — a clean Y.
        // Each arm's mouth is truncated 5 m out from the node along its heading.
        let t = 5.0_f32;
        let arm = |ang: f32| {
            let (dx, dz) = (ang.cos(), ang.sin());
            RoadEnd {
                node: 0,
                cx: dx * t,
                cz: dz * t,
                rx: -dz,
                rz: dx,
                half_w: 4.0,
                deck_y: 1.0,
            }
        };
        let third = std::f32::consts::TAU / 3.0;
        let ends = [arm(0.0), arm(third), arm(2.0 * third)];
        let hm = HeightMap::new(64, 64, 2.0); // flat → skirt feet at 0 − margin
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        // Deck: 1 centre + 3 arms × 2 corners = 7 verts, 6 fan triangles.
        assert_eq!(parts.deck.vertices.len(), 1 + 3 * 2);
        assert_eq!(parts.deck.indices.len(), 6 * 3);
        // The gaps grow curb/skirt walls (not an open or flat fan).
        assert!(!parts.structure.is_empty(), "hub gaps left unclosed");
        // Every deck vertex sits at the incident deck height or above (the level
        // fit is upward-only; nothing dips below the roads it joins).
        for v in &parts.deck.vertices {
            assert!(
                v[1] + 1.0e-4 >= 1.0,
                "hub deck vertex {v:?} below the roads"
            );
        }
        // Every deck normal points up (the fan is wound front-up, not folded).
        for nrm in &parts.deck.normals {
            assert!(nrm[1] > 0.0, "hub deck normal {nrm:?} not upward");
        }
    }

    /// #576: the hub surface is a LEVEL-plane fit — its apex sits at the mean of
    /// the incident deck heights, well below the highest mouth (no tent/crown) —
    /// while each mouth corner stays at its own road's deck height (seamless).
    #[test]
    fn hub_levels_to_the_mean_deck_height_not_a_crown() {
        let dims = Dims::from_config(&cfg(7));
        let t = 5.0_f32;
        // Three arms at deck heights 1 / 2 / 3 → mean 2, max 3.
        let arm = |ang: f32, deck_y: f32| {
            let (dx, dz) = (ang.cos(), ang.sin());
            RoadEnd {
                node: 0,
                cx: dx * t,
                cz: dz * t,
                rx: -dz,
                rz: dx,
                half_w: 4.0,
                deck_y,
            }
        };
        let third = std::f32::consts::TAU / 3.0;
        let ends = [arm(0.0, 1.0), arm(third, 2.0), arm(2.0 * third, 3.0)];
        let hm = HeightMap::new(64, 64, 2.0); // flat terrain at 0 → no upward clamp
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        // The apex (vertex 0) sits at the mean (2.0), well below the highest
        // mouth (3.0) — a level fit, not a crown to the peak.
        let apex_y = parts.deck.vertices[0][1];
        assert!((apex_y - 2.0).abs() < 1.0e-3, "apex {apex_y} ≠ mean 2.0");
        assert!(apex_y < 3.0 - 0.5, "apex tents toward the max deck height");
        // The mouth corners keep each road's own height → seamless at both
        // extremes.
        assert!(
            parts
                .deck
                .vertices
                .iter()
                .any(|v| (v[1] - 3.0).abs() < 1.0e-3),
            "highest mouth not met seamlessly"
        );
        assert!(
            parts
                .deck
                .vertices
                .iter()
                .any(|v| (v[1] - 1.0).abs() < 1.0e-3),
            "lowest mouth not met seamlessly"
        );
    }

    /// #576 regression (review wf_39a9f056-ef1): when arms truncate to different
    /// distances and the deck half-width rivals the pull-back, adjacent mouths
    /// splay past each other — a node-anchored fan over arm-grouped corners then
    /// self-intersects (overlapping deck triangles that z-fight at their differing
    /// heights). The centroid angular-sweep keeps the deck a SIMPLE polygon: its
    /// corners come out monotonically ordered by angle around the apex.
    #[test]
    fn hub_deck_stays_simple_with_asymmetric_mouths() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(64, 64, 2.0);
        // Arms at 0 / 120 / 240°, deliberately asymmetric pull-backs (1.5 / 8 / 4 m)
        // with a wide deck (half_w 4) so arm 0's short mouth splays ±~69°.
        let arm = |ang: f32, t: f32, deck_y: f32| {
            let (dx, dz) = (ang.cos(), ang.sin());
            RoadEnd {
                node: 0,
                cx: dx * t,
                cz: dz * t,
                rx: -dz,
                rz: dx,
                half_w: 4.0,
                deck_y,
            }
        };
        let third = std::f32::consts::TAU / 3.0;
        let ends = [
            arm(0.0, 1.5, 1.0),
            arm(third, 8.0, 2.0),
            arm(2.0 * third, 4.0, 1.5),
        ];
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        // Apex = vertex 0; the mouth corners follow in angular-sweep order, so
        // their angle around the apex is monotonic (⇒ a simple polygon).
        let apex = parts.deck.vertices[0];
        let angles: Vec<f32> = parts.deck.vertices[1..]
            .iter()
            .map(|v| (v[2] - apex[2]).atan2(v[0] - apex[0]))
            .collect();
        for w in angles.windows(2) {
            assert!(
                w[1] >= w[0] - 1.0e-4,
                "deck corners not angle-sorted (self-intersecting fan): {angles:?}"
            );
        }
        // Every triangle still faces up and is finite (not folded/degenerate).
        for nrm in &parts.deck.normals {
            assert!(
                nrm[1] > 0.0 && nrm.iter().all(|c| c.is_finite()),
                "bad hub deck normal {nrm:?}"
            );
        }
    }

    /// #576 seamlessness: the hub's two mouth corners must land exactly on the
    /// ribbon's end deck cross-section, so the deck flows in with no crack or
    /// overlap. Drives a real ribbon through `extrude_chain`, then checks the
    /// recorded `RoadEnd`'s mouth corners coincide with ribbon deck vertices.
    #[test]
    fn hub_mouth_corners_coincide_with_the_ribbon_end() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(64, 64, 2.0);
        let half = dims.minor_half_width;
        // A straight chain; node 1 (the +x end) is a junction, so it records a
        // truncated mouth.
        let chain = Chain {
            pts: vec![(10.0, 10.0), (20.0, 10.0), (40.0, 10.0)],
            half_w: half,
            end_nodes: [0, 1],
        };
        let degree = vec![1u32, 3u32];
        let mut road_ends = Vec::new();
        let mut parts = RoadParts::default();
        extrude_chain(
            &chain,
            0.0,
            3.0,
            &hm,
            0.0,
            &dims,
            &degree,
            &mut road_ends,
            &mut parts,
        );
        assert_eq!(road_ends.len(), 1, "the junction end must record a mouth");

        let e = &road_ends[0];
        let deck = parts.deck.vertices.clone();
        for sgn in [-1.0_f32, 1.0] {
            let corner = [
                e.cx + sgn * e.rx * e.half_w,
                e.deck_y,
                e.cz + sgn * e.rz * e.half_w,
            ];
            let hit = deck.iter().any(|v| {
                (v[0] - corner[0]).abs() < 1.0e-3
                    && (v[1] - corner[1]).abs() < 1.0e-3
                    && (v[2] - corner[2]).abs() < 1.0e-3
            });
            assert!(
                hit,
                "hub mouth corner {corner:?} not on the ribbon end (seam)"
            );
        }
    }

    /// #575: a clean orthogonal cross truncates every arm by exactly the outer
    /// footprint half-width `wo` — the adjacent-boundary solve's closed form for
    /// right-angle arms — while the non-junction far ends are left untrimmed.
    #[test]
    fn truncation_pulls_arms_back_at_an_orthogonal_cross() {
        let dims = Dims::from_config(&cfg(7));
        let w = dims.minor_half_width;
        let wo = w + dims.curb_top_width + dims.chamfer_width;
        // Four arms leaving junction node 0 along ±x / ±z; the far ends (nodes
        // 1..4) are dead-ends, so only the slot-0 (junction) end truncates.
        let arm = |to: (f32, f32), far: usize| Chain {
            pts: vec![
                (0.0, 0.0),
                (to.0 * 10.0, to.1 * 10.0),
                (to.0 * 40.0, to.1 * 40.0),
            ],
            half_w: w,
            end_nodes: [0, far],
        };
        let chains = [
            arm((1.0, 0.0), 1),
            arm((-1.0, 0.0), 2),
            arm((0.0, 1.0), 3),
            arm((0.0, -1.0), 4),
        ];
        let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
        for (ci, t) in trims.iter().enumerate() {
            assert!(
                (t[0] - wo).abs() < 1.0e-3,
                "arm {ci} start trim {} ≠ wo {wo}",
                t[0]
            );
            assert_eq!(t[1], 0.0, "non-junction far end of arm {ci} must not trim");
        }
    }

    /// #575: with no junction ends, nothing truncates (every trim is zero).
    #[test]
    fn truncation_skips_non_junction_ends() {
        let dims = Dims::from_config(&cfg(7));
        let chains = [Chain {
            pts: vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)],
            half_w: dims.minor_half_width,
            end_nodes: [0, 1],
        }];
        // No node is a junction → no pull-back anywhere.
        let trims = compute_truncations(&chains, |_| false, &dims);
        assert_eq!(trims, vec![[0.0, 0.0]]);
    }

    /// #575: `trim_polyline` removes arc length from each end, interpolating the
    /// cut points, and keeps the interior vertices that survive.
    #[test]
    fn trim_polyline_shortens_both_ends() {
        let pts = vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
        let out = trim_polyline(&pts, 3.0, 4.0);
        assert!(
            (out[0].0 - 3.0).abs() < 1.0e-4,
            "start cut at x=3, got {out:?}"
        );
        assert!(
            (out.last().unwrap().0 - 16.0).abs() < 1.0e-4,
            "end cut at x=16, got {out:?}"
        );
        // The mid vertex (x=10) lies inside (3, 16) → retained.
        assert!(out.iter().any(|p| (p.0 - 10.0).abs() < 1.0e-4));
    }

    /// #575: a chain shorter than the combined pull-back is wholly consumed by
    /// the hubs and grows no ribbon (fewer than two points back).
    #[test]
    fn trim_polyline_consumes_short_chain() {
        let pts = vec![(0.0, 0.0), (5.0, 0.0)];
        assert!(trim_polyline(&pts, 4.0, 4.0).len() < 2);
    }

    /// #575: truncation never changes the geometry's determinism — the same
    /// chains yield byte-identical pull-backs each run.
    #[test]
    fn truncation_is_deterministic() {
        let dims = Dims::from_config(&cfg(7));
        let mk = || {
            let arm = |to: (f32, f32), far: usize| Chain {
                pts: vec![(0.0, 0.0), (to.0 * 12.0, to.1 * 12.0)],
                half_w: dims.major_half_width,
                end_nodes: [0, far],
            };
            [
                arm((1.0, 0.2), 1),
                arm((-0.3, 1.0), 2),
                arm((-0.7, -0.7), 3),
            ]
        };
        let a = compute_truncations(&mk(), |nd| nd == 0, &dims);
        let b = compute_truncations(&mk(), |nd| nd == 0, &dims);
        assert_eq!(a, b, "truncation must be deterministic");
    }

    /// #575: an acute fork would need an unbounded pull-back (the boundary
    /// crossing runs to infinity as the branch angle → 0); the cap keeps it at a
    /// width-relative maximum so the chains survive for the merge pass (#578).
    #[test]
    fn truncation_caps_an_acute_fork() {
        let dims = Dims::from_config(&cfg(7));
        let w = dims.minor_half_width;
        let cap = MAX_TRUNCATION_FACTOR * (w + dims.curb_top_width + dims.chamfer_width);
        // Two arms leaving node 0 ~5° apart — a sliver fork. Long arms (60 m) so
        // the baseline heading is unambiguous and nothing else trims them.
        let ang = 5.0_f32.to_radians();
        let arm = |a: f32, far: usize| Chain {
            pts: vec![(0.0, 0.0), (a.cos() * 60.0, a.sin() * 60.0)],
            half_w: w,
            end_nodes: [0, far],
        };
        let chains = [arm(0.0, 1), arm(ang, 2)];
        let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
        for (ci, t) in trims.iter().enumerate() {
            assert!(
                t[0].is_finite() && t[0] <= cap + 1.0e-3,
                "acute arm {ci} pull-back {} exceeded the cap {cap}",
                t[0]
            );
        }
        // The fork is acute enough that at least one arm is pinned to the cap
        // (proving the bound actually engaged, not a coincidentally small solve).
        assert!(
            trims.iter().any(|t| (t[0] - cap).abs() < 1.0e-3),
            "cap never engaged on a 5° fork: {trims:?}"
        );
    }

    /// #575: a T-junction's straight through road is two anti-parallel adjacent
    /// arms, so its 2×2 boundary solve is singular and takes the parallel
    /// fallback `(w_a + w_b)/2 = wo`. Every arm (through pair + side street)
    /// truncates to `wo`. (This is the commonest real junction — the fallback is
    /// load-bearing, so it gets its own pin.)
    #[test]
    fn truncation_handles_a_t_junction_through_pair() {
        let dims = Dims::from_config(&cfg(7));
        let w = dims.minor_half_width;
        let wo = w + dims.curb_top_width + dims.chamfer_width;
        // Through road ±x with a side street +z, meeting node 0. Long arms so the
        // floor/clamp never interfere.
        let arm = |to: (f32, f32), far: usize| Chain {
            pts: vec![(0.0, 0.0), (to.0 * 40.0, to.1 * 40.0)],
            half_w: w,
            end_nodes: [0, far],
        };
        let chains = [arm((1.0, 0.0), 1), arm((-1.0, 0.0), 2), arm((0.0, 1.0), 3)];
        let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
        for (ci, t) in trims.iter().enumerate() {
            assert!(
                (t[0] - wo).abs() < 1.0e-3,
                "T-junction arm {ci} trim {} ≠ wo {wo}",
                t[0]
            );
        }
    }

    /// #575: a wide-open 120° Y is so splayed the adjacent-boundary solve returns
    /// *less* than the half-width floor, so every arm pins to `half_w` (not `wo`).
    /// Pins the floor branch — the dominant organic-junction regime — which a
    /// dropped floor-init would silently under-truncate.
    #[test]
    fn truncation_floors_a_wide_y_at_the_half_width() {
        let dims = Dims::from_config(&cfg(7));
        let w = dims.minor_half_width;
        let wo = w + dims.curb_top_width + dims.chamfer_width;
        let arm = |deg: f32, far: usize| {
            let a = deg.to_radians();
            Chain {
                pts: vec![(0.0, 0.0), (a.cos() * 40.0, a.sin() * 40.0)],
                half_w: w,
                end_nodes: [0, far],
            }
        };
        // 90° / 210° / 330° — three arms 120° apart.
        let chains = [arm(90.0, 1), arm(210.0, 2), arm(330.0, 3)];
        let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
        for (ci, t) in trims.iter().enumerate() {
            assert!(
                (t[0] - w).abs() < 1.0e-3,
                "wide-Y arm {ci} trim {} ≠ half_w floor {w}",
                t[0]
            );
        }
        assert!(w < wo, "sanity: the floor sits below the outer footprint");
    }

    /// #575 regression (review wf_e27b3d8b-91d): a short connector between two
    /// junctions is shorter than its combined pull-back, so before the clamp it
    /// trimmed to nothing and BOTH junctions silently lost an arm — and a
    /// mouth-driven hub with < 3 arms is dropped entirely (a hole at a real
    /// intersection). The clamp keeps a meshable stub, so each junction still
    /// records all three mouths and grows its hub.
    #[test]
    fn short_junction_connector_keeps_both_hubs() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(64, 64, 2.0);
        let w = dims.minor_half_width;
        let chain = |pts: Vec<(f32, f32)>, ends: [usize; 2]| Chain {
            pts,
            half_w: w,
            end_nodes: ends,
        };
        // Two degree-3 junctions (nodes 0, 1) 3 m apart, each with two splayed
        // dead-end arms; the connector abuts a junction at both ends.
        let chains = [
            chain(vec![(0.0, 0.0), (3.0, 0.0)], [0, 1]), // the short connector
            chain(vec![(0.0, 0.0), (-20.0, -20.0)], [0, 2]),
            chain(vec![(0.0, 0.0), (-20.0, 20.0)], [0, 3]),
            chain(vec![(3.0, 0.0), (23.0, -20.0)], [1, 4]),
            chain(vec![(3.0, 0.0), (23.0, 20.0)], [1, 5]),
        ];
        let mut degree = vec![0u32; 6];
        degree[0] = 3;
        degree[1] = 3;
        for d in degree.iter_mut().skip(2) {
            *d = 1;
        }
        let trims = compute_truncations(&chains, |nd| degree[nd] >= 3, &dims);
        // The connector keeps at least the floor (not consumed).
        let surviving = 3.0 - (trims[0][0] + trims[0][1]);
        assert!(
            surviving + 1.0e-4 >= MIN_RIBBON_LEN_M,
            "connector consumed: only {surviving} m left"
        );

        let mut road_ends = Vec::new();
        let mut parts = RoadParts::default();
        for (ci, c) in chains.iter().enumerate() {
            let [s, e] = trims[ci];
            extrude_chain(
                c,
                s,
                e,
                &hm,
                0.0,
                &dims,
                &degree,
                &mut road_ends,
                &mut parts,
            );
        }
        let arms_at = |n: usize| road_ends.iter().filter(|r| r.node == n).count();
        assert_eq!(arms_at(0), 3, "node 0 lost an arm to over-truncation");
        assert_eq!(arms_at(1), 3, "node 1 lost an arm to over-truncation");

        extrude_hubs(&road_ends, &hm, 0.0, &dims, &mut parts);
        assert!(
            !parts.deck.is_empty(),
            "both junctions failed to grow a hub"
        );
    }

    /// #575 regression on the real pilot network (review wf_e27b3d8b-91d measured
    /// 12 of 45 junctions losing their hub before the clamp): replays
    /// `build_road_geometry`'s mouth collection and asserts every junction keeps
    /// exactly the mouths its incident chains carry — no arm is silently trimmed
    /// out of existence, so no real intersection is left a hole.
    #[test]
    fn pilot_junctions_keep_every_mouth_after_truncation() {
        use std::collections::BTreeMap;
        let hm = pilot_heightmap();
        let config = cfg(PILOT_ROAD_SEED);
        let (graph, sub, _lo) = build_road_graph(&hm, &config).expect("pilot must trace");
        let dims = Dims::from_config(&config);
        let chains = extract_chains(&graph, &sub, &dims);

        let mut degree = vec![0u32; graph.nodes.len()];
        for e in &graph.edges {
            if e.active {
                degree[e.start as usize] += 1;
                degree[e.end as usize] += 1;
            }
        }
        let is_junction = |nd: usize| degree.get(nd).copied().unwrap_or(0) >= 3;
        let trims = compute_truncations(&chains, is_junction, &dims);

        // Mouths each junction *should* carry = chain ends abutting a degree≥3 node.
        let mut expected: BTreeMap<usize, usize> = BTreeMap::new();
        for c in &chains {
            for &nd in &c.end_nodes {
                if is_junction(nd) {
                    *expected.entry(nd).or_default() += 1;
                }
            }
        }
        // Mouths actually recorded after truncation + trimming.
        let mut road_ends = Vec::new();
        let mut parts = RoadParts::default();
        for (ci, c) in chains.iter().enumerate() {
            let [s, e] = trims[ci];
            extrude_chain(
                c,
                s,
                e,
                &sub,
                0.0,
                &dims,
                &degree,
                &mut road_ends,
                &mut parts,
            );
        }
        let mut recorded: BTreeMap<usize, usize> = BTreeMap::new();
        for r in &road_ends {
            *recorded.entry(r.node).or_default() += 1;
        }

        assert_eq!(
            recorded, expected,
            "truncation dropped a junction mouth on the pilot network"
        );
        // Sanity: the pilot really does exercise multi-arm junctions (so the
        // assertion above is non-vacuous).
        assert!(
            expected.values().filter(|&&c| c >= 3).count() > 10,
            "pilot expected to have many real junctions"
        );
    }
}
