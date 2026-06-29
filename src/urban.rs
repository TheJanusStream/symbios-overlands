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
/// profile face is its own strip. Junction hub decks are smooth-shaded from
/// accumulated up-facing triangle normals (see [`extrude_hubs`]).
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
    /// Per-end boundary-clip markers (index matches `end_nodes`). `true` when the
    /// run was cut at the district-interior boundary because the next sampled
    /// node fell *outside* — i.e. a road running off the network perimeter, which
    /// leaves an open cross-section and must be capped like a dead-end (#582). A
    /// genuine graph terminus is `false` here: a degree-1 dead-end is capped by
    /// degree (#579) and a loop closure / used-edge break stays open.
    clip: [bool; 2],
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
    /// The ribbon's skirt-bottom height at this mouth (`Frame::skirt_bottom_y`),
    /// so the hub fillet's skirt foot can drop to the *same* depth and weld to the
    /// ribbon skirt exactly — at any skirt depth or cross-slope, not just the deep
    /// default — leaving no open band at the seam.
    skirt_y: f32,
}

/// Curb-return fillet radius as a multiple of the deck half-width. Real curb
/// returns run ~1–1.5× a lane half-width (Minneapolis/AASHTO); this sets how
/// hard a hub corner rounds between two adjacent roads.
const CURB_RETURN_FACTOR: f32 = 1.5;
/// Cap on a fillet's outward bulge (sagitta) as a multiple of half-width, so a
/// wide gap can't balloon the corner apron well past the curb line.
const CURB_RETURN_MAX_SAG_FACTOR: f32 = 0.8;
/// Above this |cos| between two adjacent arms' headings the two are collinear —
/// a through road's two halves (anti-parallel) or an acute fork (parallel) — so
/// the gap is a near-straight curb and its fillet stays flat (sagitta 0) rather
/// than bumping a straight edge. Acute forks are blended properly by #578.
const FILLET_STRAIGHT_COS: f32 = 0.95;
/// Arc segments per curb-return fillet — sampled finely enough to read as a
/// smooth curve after the along-arc normal averaging.
const FILLET_SEG: usize = 6;

/// Build a real intersection hub at every junction (≥3 incident roads) from the
/// truncated ribbon ends (#576): a deck polygon whose mouth edges coincide with
/// each road's end cross-section (the deck flows in seamlessly at the road's own
/// height), its surface a **level-plane fit** to the incident mouth heights (the
/// apex sits at their mean, not the max — so it stays level instead of tenting),
/// kept upward-only, plus **curb-return arc fillets** (#577) closing the angular
/// gaps: each corner between two adjacent roads rounds with a circular arc
/// joining their outer curbs, the curb profile swept along it (continuous with
/// the incident ribbon curbs) and a skirt dropping to the ground. Smooth-shaded;
/// every deck triangle wound front-up.
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

        // --- Curb-return fillets (#577): a polygon edge between corners of
        //     DIFFERENT arms is an exterior corner → round it with a curb-return
        //     arc joining the two roads' outer curbs, the curb profile (curb wall
        //     + top + chamfer) swept along the arc and a skirt dropping to the
        //     ground. An edge within ONE arm is a mouth → left open for the road.
        //     The deck (#576) is left untouched: the curb's inner edge rides the
        //     deck's straight chord boundary, and only the curb top / chamfer /
        //     skirt round outward to the arc, so the deck stays the simple
        //     level-fit polygon. The arc starts/ends exactly on each ribbon's
        //     outer-curb point, so the curb is continuous across the seam. ---
        let (ct, cf, ch) = (dims.curb_top_width, dims.chamfer_width, dims.curb_height);
        for i in 0..p {
            let (l, la) = corners[i];
            let (r, ra) = corners[(i + 1) % p];
            if la == ra {
                continue; // a mouth edge: open for the road
            }
            let arm_l = arms[la];
            let arm_r = arms[ra];

            // Each gap corner's outward radial (unit, XZ) and its outer-curb point
            // — the deck-edge corner pushed out by curb_top + chamfer (= the outer
            // footprint `wo`), i.e. exactly where the incident ribbon's outer curb
            // ends — so the fillet arc joins one ribbon's outer curb to the next.
            let cl = [arm_l.cx + world_offset, arm_l.cz + world_offset];
            let cr = [arm_r.cx + world_offset, arm_r.cz + world_offset];
            let rad_l = norm2([l[0] - cl[0], l[2] - cl[1]]);
            let rad_r = norm2([r[0] - cr[0], r[2] - cr[1]]);
            let (wo_l, wo_r) = (arm_l.half_w + ct + cf, arm_r.half_w + ct + cf);
            let o_l = [cl[0] + rad_l[0] * wo_l, cl[1] + rad_l[1] * wo_l];
            let o_r = [cr[0] + rad_r[0] * wo_r, cr[1] + rad_r[1] * wo_r];
            let chord = (o_r[0] - o_l[0]).hypot(o_r[1] - o_l[1]);
            if chord < 1.0e-3 {
                continue; // coincident mouths — nothing to round
            }

            // Bulge outward (away from the hub centroid) by a sagitta derived from
            // a curb-return radius of ~CURB_RETURN_FACTOR · half_w, clamped gentle.
            // A near-straight gap (two anti-parallel arms — e.g. a through road's
            // far edge) keeps sagitta 0, so its curb stays a straight line.
            let half_w = (arm_l.half_w + arm_r.half_w) * 0.5;
            // Arm heading from the mouth-frame right (`(rz, −rx)` ⟂ right). Collinear
            // arms (a through road's two halves, or an acute fork) read as parallel
            // OR anti-parallel, so test |cosθ| — both keep the gap a straight curb
            // (a through road's far edge must not bump; acute forks are #578's job).
            let dir_l = [arm_l.rz, -arm_l.rx];
            let dir_r = [arm_r.rz, -arm_r.rx];
            let straight = fillet_gap_is_straight(dir_l, dir_r);
            let bd = norm2([
                (l[0] + r[0]) * 0.5 - center[0],
                (l[2] + r[2]) * 0.5 - center[2],
            ]);
            let radius = CURB_RETURN_FACTOR * half_w;
            let h = chord * 0.5;
            let sag = if straight {
                0.0
            } else {
                let s = if radius > h {
                    radius - (radius * radius - h * h).sqrt()
                } else {
                    h
                };
                s.min(CURB_RETURN_MAX_SAG_FACTOR * half_w).min(0.45 * chord)
            };
            let arc = fillet_arc(o_l, o_r, bd, sag, FILLET_SEG);

            // Per-arc-sample curb cross-section, the inner edge riding the deck's
            // straight chord (deck untouched) and the chamfer/skirt rounding out to
            // the arc. P1 curb inner bottom .. P5 skirt foot.
            let n = arc.len();
            let mut p1: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p2: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p3: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p4: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut p5: Vec<[f32; 3]> = Vec::with_capacity(n);
            let mut vlen: Vec<f32> = Vec::with_capacity(n);
            let mut acc = 0.0_f32;
            for (k, op) in arc.iter().enumerate() {
                let t = k as f32 / (n - 1) as f32;
                // Deck-level height interpolated between the two mouths — matching
                // the deck triangle edge l→r, which runs deck_y_l → deck_y_r.
                let dy = l[1] + (r[1] - l[1]) * t;
                let inner = [l[0] + (r[0] - l[0]) * t, dy, l[2] + (r[2] - l[2]) * t];
                let outer = [op[0], dy, op[1]];
                // Across-curb direction: from the deck-chord edge out to the arc.
                let rad = norm2([outer[0] - inner[0], outer[2] - inner[2]]);
                // Skirt foot: weld to the incident ribbons' skirt bottoms by
                // interpolating each arm's recorded `skirt_y` across the arc (so at
                // the two ends the foot equals the ribbon skirt exactly — at ANY
                // skirt depth or cross-slope, no open band at the seam), then drop
                // further to reach a terrain dip under the arc, but never above the
                // deck edge it drops from (the #576 finding-2 upward clamp).
                let skirt = arm_l.skirt_y + (arm_r.skirt_y - arm_l.skirt_y) * t;
                let fy = skirt
                    .min(
                        hm.get_height_at(outer[0] - world_offset, outer[2] - world_offset)
                            - SKIRT_BURY_MARGIN_M,
                    )
                    .min(dy - 1.0e-3);
                if k > 0 {
                    acc += (outer[0] - p4[k - 1][0]).hypot(outer[2] - p4[k - 1][2]);
                }
                p1.push(inner);
                p2.push([inner[0], inner[1] + ch, inner[2]]);
                p3.push([
                    inner[0] + rad[0] * ct,
                    inner[1] + ch,
                    inner[2] + rad[1] * ct,
                ]);
                p4.push(outer);
                p5.push([outer[0], fy, outer[2]]);
                vlen.push(acc / UV_TILE_M);
            }

            // One smoothed strip per profile face (smooth along the arc, hard
            // crease across — the WS2 idea), each wound so its front side faces out.
            let (u1, u2) = (ch / UV_TILE_M, (ch + ct) / UV_TILE_M);
            let u3 = (ch + ct + cf) / UV_TILE_M;
            let u4 = u3 + dims.skirt_depth / UV_TILE_M;
            push_fillet_face(parts, center, &p1, &p2, (0.0, u1), &vlen); // curb wall
            push_fillet_face(parts, center, &p2, &p3, (u1, u2), &vlen); // curb top
            push_fillet_face(parts, center, &p3, &p4, (u2, u3), &vlen); // chamfer
            push_fillet_face(parts, center, &p4, &p5, (u3, u4), &vlen); // skirt
        }
    }
}

/// Whether two adjacent hub arms are collinear — a through road's two halves
/// (anti-parallel) or an acute fork (parallel) — in which case the gap between
/// them is a near-straight curb that must NOT bulge (the fillet keeps sagitta 0).
/// Orientation-independent (tests |cosθ| of the unit headings), so it holds
/// whichever way each arm's recorded heading happens to point.
fn fillet_gap_is_straight(dir_l: [f32; 2], dir_r: [f32; 2]) -> bool {
    (dir_l[0] * dir_r[0] + dir_l[1] * dir_r[1]).abs() > FILLET_STRAIGHT_COS
}

/// Sample a circular curb-return arc from `a` to `b` (XZ) bulging by sagitta
/// `sag` toward the outward direction `bd`. Returns `segs + 1` points, the first
/// exactly `a` and the last exactly `b` (force-assigned, so they always coincide
/// with the incident ribbon's curb point regardless of `bd`). A non-positive
/// sagitta (or a degenerate chord) returns the straight chord, so a near-collinear
/// gap stays flat. Deterministic; no `Date`/random.
fn fillet_arc(a: [f32; 2], b: [f32; 2], bd: [f32; 2], sag: f32, segs: usize) -> Vec<[f32; 2]> {
    let lerp = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let chord = [b[0] - a[0], b[1] - a[1]];
    let clen = chord[0].hypot(chord[1]);
    let half = clen * 0.5;
    if segs == 0 || half < 1.0e-4 || sag < 1.0e-4 {
        return (0..=segs)
            .map(|i| lerp(i as f32 / segs.max(1) as f32))
            .collect();
    }
    let r = (sag * sag + half * half) / (2.0 * sag);
    let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    // Place the centre on the chord's OWN perpendicular bisector — then |a−c| =
    // |b−c| = r exactly, so both endpoints land on the circle whatever `bd` is.
    // `bd` only picks which side the arc bulges (the corner side).
    let perp = [-chord[1] / clen, chord[0] / clen];
    let pn = if perp[0] * bd[0] + perp[1] * bd[1] >= 0.0 {
        perp
    } else {
        [-perp[0], -perp[1]]
    };
    let center = [mid[0] - pn[0] * (r - sag), mid[1] - pn[1] * (r - sag)];
    let ang = |p: [f32; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
    let (pi, tau, frac) = (
        std::f32::consts::PI,
        std::f32::consts::TAU,
        std::f32::consts::FRAC_PI_2,
    );
    let a0 = ang(a);
    let mut d = ang(b) - a0;
    while d > pi {
        d -= tau;
    }
    while d <= -pi {
        d += tau;
    }
    // Keep the sweep on the bulge (apex) side: the apex bears along `pn`. If the
    // short sweep's midpoint faces away from it, take the complementary arc.
    let apex = pn[1].atan2(pn[0]);
    let mut diff = (a0 + d * 0.5) - apex;
    while diff > pi {
        diff -= tau;
    }
    while diff <= -pi {
        diff += tau;
    }
    if diff.abs() > frac {
        d += if d > 0.0 { -tau } else { tau };
    }
    let mut pts: Vec<[f32; 2]> = (0..=segs)
        .map(|i| {
            let th = a0 + d * (i as f32 / segs as f32);
            [center[0] + r * th.cos(), center[1] + r * th.sin()]
        })
        .collect();
    // Pin the endpoints exactly (kill float residue) so the seam is watertight.
    pts[0] = a;
    pts[segs] = b;
    pts
}

/// Push one curb-return-fillet face strip into `parts.structure`: `inner`/`outer`
/// are the face's two edges at each arc sample, smooth-shaded ALONG the arc (welded
/// vertices carrying averaged segment normals) with a hard crease ACROSS the
/// profile (one strip per face). Each segment is wound INDIVIDUALLY so its front
/// face matches that segment's own outward normal — a single per-strip decision is
/// wrong on a curved or height-sloped strip (where the geometric facing flips
/// partway), which would back-wind some triangles and mis-shade them under the
/// road's double-sided material.
fn push_fillet_face(
    parts: &mut RoadParts,
    center: [f32; 3],
    inner: &[[f32; 3]],
    outer: &[[f32; 3]],
    uv_u: (f32, f32),
    v: &[f32],
) {
    let n = inner.len();
    if n < 2 {
        return;
    }
    // Per-segment outward normals (oriented away from the hub centre), then the
    // welded per-vertex normal = average of the (up to two) segments meeting at i,
    // so the strip shades smoothly along the arc.
    let seg: Vec<[f32; 3]> = (0..n - 1)
        .map(|k| quad_normal(inner[k], outer[k], inner[k + 1], outer[k + 1], center))
        .collect();
    let vn: Vec<[f32; 3]> = (0..n)
        .map(|i| {
            let mut acc = [0.0_f32; 3];
            for s in [i.checked_sub(1), (i < seg.len()).then_some(i)]
                .into_iter()
                .flatten()
            {
                acc = [acc[0] + seg[s][0], acc[1] + seg[s][1], acc[2] + seg[s][2]];
            }
            normalize(acc)
        })
        .collect();
    let g = &mut parts.structure;
    let base = g.vertices.len() as u32;
    for i in 0..n {
        g.vertices.push(inner[i]);
        g.vertices.push(outer[i]);
        g.normals.push(vn[i]);
        g.normals.push(vn[i]);
        g.uvs.push([uv_u.0, v[i]]);
        g.uvs.push([uv_u.1, v[i]]);
    }
    // Emit each TRIANGLE wound so its geometric front matches its own averaged
    // shading normal. Per-triangle (not per-strip, nor even per-quad) is required:
    // a fillet quad is generally warped (the inner edge is a straight chord, the
    // outer edge a curved arc at varying height), so its two triangles can face
    // opposite ways — a single decision back-winds one of them and mis-shades it.
    let mut tri = |a: u32, b: u32, c: u32, na: [f32; 3], nb: [f32; 3], nc: [f32; 3]| {
        let (qa, qb, qc) = (
            g.vertices[a as usize],
            g.vertices[b as usize],
            g.vertices[c as usize],
        );
        let geo = cross(sub3(qb, qa), sub3(qc, qa));
        let nsum = [
            na[0] + nb[0] + nc[0],
            na[1] + nb[1] + nc[1],
            na[2] + nb[2] + nc[2],
        ];
        if dot(geo, nsum) >= 0.0 {
            g.indices.extend_from_slice(&[a, b, c]);
        } else {
            g.indices.extend_from_slice(&[a, c, b]);
        }
    };
    for i in 0..n - 1 {
        let (li, ri, lj, rj) = (
            base + 2 * i as u32,
            base + 2 * i as u32 + 1,
            base + 2 * i as u32 + 2,
            base + 2 * i as u32 + 3,
        );
        tri(li, ri, rj, vn[i], vn[i], vn[i + 1]);
        tri(li, rj, lj, vn[i], vn[i + 1], vn[i + 1]);
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

/// Normalize a 2D (XZ) vector; a near-zero vector falls back to `+X` so callers
/// never propagate a NaN direction.
fn norm2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l < 1.0e-6 {
        [1.0, 0.0]
    } else {
        [v[0] / l, v[1] / l]
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
    /// Chain-end disposition, mirroring how the mesher closes each end:
    /// `[junction, dead_end, clip, open]`. junction = degree ≥ 3 (closed by a
    /// hub); dead_end = degree 1 (capped, #579); clip = boundary clip running off
    /// the perimeter (capped, #582); open = the residue (degree-2 loop closure /
    /// used-edge break) deliberately left open. `open` is the count of still-open
    /// cross-sections — only genuine interior seams should remain here.
    chain_end_class: [usize; 4],
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
        chain_end_class,
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
        let [j, d, c, o] = self.chain_end_class;
        let _ = writeln!(
            s,
            "chain ends: {} junction(hub)  {} dead-end(cap)  {} perimeter-clip(cap, #582)  {} open(loop/break)",
            j, d, c, o
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
                skirt_y: -4.0,
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
                skirt_y: deck_y - 5.0,
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
                skirt_y: deck_y - 5.0,
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
            clip: [false, false],
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

    /// #577: `fillet_arc` samples a circular bulge — the apex sits one sagitta
    /// out from the chord midpoint along `bd`, and a zero sagitta is the straight
    /// chord (so a near-collinear gap stays flat).
    #[test]
    fn fillet_arc_is_a_circular_bulge() {
        let a = [-2.0_f32, 0.0];
        let b = [2.0_f32, 0.0];
        let bd = [0.0_f32, 1.0]; // bulge toward +y
        let sag = 0.5_f32;
        let arc = fillet_arc(a, b, bd, sag, 6);
        assert_eq!(arc.len(), 7, "segs+1 samples");
        assert!((arc[0][0] - a[0]).abs() < 1.0e-4 && (arc[0][1] - a[1]).abs() < 1.0e-4);
        assert!((arc[6][0] - b[0]).abs() < 1.0e-4 && (arc[6][1] - b[1]).abs() < 1.0e-4);
        // The midpoint sample bulges out by ~sag along +y.
        assert!(
            (arc[3][1] - sag).abs() < 1.0e-3,
            "apex {} ≠ sagitta {sag}",
            arc[3][1]
        );
        // Every sample is equidistant from the reconstructed circle centre.
        let r = (sag * sag + 4.0) / (2.0 * sag);
        let c = [0.0_f32, sag - r];
        for p in &arc {
            let d = (p[0] - c[0]).hypot(p[1] - c[1]);
            assert!((d - r).abs() < 1.0e-2, "off-circle sample d={d} r={r}");
        }
        // Zero sagitta → the straight chord.
        let flat = fillet_arc(a, b, bd, 0.0, 6);
        assert!(flat.iter().all(|p| p[1].abs() < 1.0e-4), "flat arc bulged");

        // The straight-gap detector flags ONLY collinear arms (so real corners keep
        // their fillet — a too-low threshold that flattened them would fail here).
        let unit = |deg: f32| {
            let r = deg.to_radians();
            [r.cos(), r.sin()]
        };
        assert!(
            fillet_gap_is_straight(unit(0.0), unit(180.0)),
            "through road"
        ); // anti-parallel
        assert!(fillet_gap_is_straight(unit(0.0), unit(8.0)), "acute fork"); // near-parallel
        assert!(
            !fillet_gap_is_straight(unit(0.0), unit(90.0)),
            "right-angle corner"
        );
        assert!(
            !fillet_gap_is_straight(unit(0.0), unit(120.0)),
            "wide-Y corner"
        );
        assert!(!fillet_gap_is_straight(unit(0.0), unit(45.0)), "45° corner");

        // Endpoints stay EXACT even when `bd` is NOT perpendicular to the chord
        // (the asymmetric-hub case): the centre is placed on the chord's own
        // perpendicular bisector and the endpoints are pinned. A sloped chord with
        // an off-axis bulge direction would drift the endpoints under the old
        // `centre = mid − bd·(r−sag)` formula.
        // `bd2` is deliberately FAR from perpendicular to the chord (5,−2) — under
        // the old `centre = mid − bd·(r−sag)` this drifted the endpoints; the chord-
        // bisector centre + pinned endpoints must keep them exact.
        let (a2, b2, bd2) = ([-3.0_f32, 1.0], [2.0_f32, -1.0], norm2([1.0, 0.0]));
        let arc2 = fillet_arc(a2, b2, bd2, 0.7, 6);
        assert!(
            (arc2[0][0] - a2[0]).abs() < 1.0e-4 && (arc2[0][1] - a2[1]).abs() < 1.0e-4,
            "off-axis bd drifted the start endpoint: {:?}",
            arc2[0]
        );
        assert!(
            (arc2[6][0] - b2[0]).abs() < 1.0e-4 && (arc2[6][1] - b2[1]).abs() < 1.0e-4,
            "off-axis bd drifted the end endpoint: {:?}",
            arc2[6]
        );
        // Interior samples still share one circle (a real arc, not a kink).
        let cc = {
            // centre = mid − pn·(r−sag) with pn the chord-perp toward bd2.
            let chord = [b2[0] - a2[0], b2[1] - a2[1]];
            let cl = chord[0].hypot(chord[1]);
            let half2 = cl * 0.5;
            let r2 = (0.7 * 0.7 + half2 * half2) / (2.0 * 0.7);
            let mut perp = [-chord[1] / cl, chord[0] / cl];
            if perp[0] * bd2[0] + perp[1] * bd2[1] < 0.0 {
                perp = [-perp[0], -perp[1]];
            }
            let mid = [(a2[0] + b2[0]) * 0.5, (a2[1] + b2[1]) * 0.5];
            (
                [mid[0] - perp[0] * (r2 - 0.7), mid[1] - perp[1] * (r2 - 0.7)],
                r2,
            )
        };
        for p in &arc2 {
            let d = (p[0] - cc.0[0]).hypot(p[1] - cc.0[1]);
            assert!(
                (d - cc.1).abs() < 1.0e-2,
                "off-axis arc off-circle d={d} r={}",
                cc.1
            );
        }
    }

    /// #577: the hub curb is continuous with the incident ribbons — each fillet
    /// arc starts/ends exactly on the road's outer-curb point, so there's no
    /// notch where the hub curb meets the ribbon curb.
    #[test]
    fn hub_fillet_joins_the_ribbon_outer_curbs() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(96, 96, 2.0);
        let half = dims.minor_half_width;
        let wo = half + dims.curb_top_width + dims.chamfer_width;
        // A Y of three chains meeting at node 0 (degree 3).
        let third = std::f32::consts::TAU / 3.0;
        let chains: Vec<Chain> = (0..3)
            .map(|k| {
                let ang = k as f32 * third;
                let (dx, dz) = (ang.cos(), ang.sin());
                Chain {
                    pts: vec![
                        (50.0, 50.0),
                        (50.0 + dx * 15.0, 50.0 + dz * 15.0),
                        (50.0 + dx * 40.0, 50.0 + dz * 40.0),
                    ],
                    half_w: half,
                    end_nodes: [0, 1 + k],
                    clip: [false, false],
                }
            })
            .collect();
        let mut degree = vec![1u32; 4];
        degree[0] = 3;
        let trims = compute_truncations(&chains, |nd| degree[nd] >= 3, &dims);
        let mut road_ends = Vec::new();
        let mut ribbon = RoadParts::default();
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
                &mut ribbon,
            );
        }
        assert_eq!(road_ends.len(), 3, "the Y must record three mouths");

        let mut hub = RoadParts::default();
        extrude_hubs(&road_ends, &hm, 0.0, &dims, &mut hub);

        let near = |verts: &[[f32; 3]], p: [f32; 3]| {
            verts.iter().any(|v| {
                (v[0] - p[0]).abs() < 1.0e-3
                    && (v[1] - p[1]).abs() < 1.0e-3
                    && (v[2] - p[2]).abs() < 1.0e-3
            })
        };
        // Flat terrain at 0 → the ribbon's skirt bottom is `(base_y −
        // skirt_depth).min(ground − margin)` with ground = 0; the fillet must drop
        // to the SAME depth so the two skirts weld (no open band at the seam).
        let skirt_y = |deck_y: f32| (deck_y - dims.skirt_depth).min(-SKIRT_BURY_MARGIN_M);
        for e in &road_ends {
            for sgn in [-1.0_f32, 1.0] {
                // Outer-curb point (chamfer base, deck level) — the arc endpoint.
                let o = [e.cx + sgn * e.rx * wo, e.deck_y, e.cz + sgn * e.rz * wo];
                assert!(
                    near(&ribbon.structure.vertices, o),
                    "ribbon curb missing its outer point {o:?}"
                );
                assert!(
                    near(&hub.structure.vertices, o),
                    "fillet does not meet the ribbon outer curb at {o:?}"
                );
                // Skirt bottom — the fillet skirt foot must meet the ribbon's deep
                // skirt bottom (the seam-continuity HIGH the review caught).
                let foot = [o[0], skirt_y(e.deck_y), o[2]];
                assert!(
                    near(&ribbon.structure.vertices, foot),
                    "ribbon skirt missing its bottom point {foot:?}"
                );
                assert!(
                    near(&hub.structure.vertices, foot),
                    "fillet skirt foot leaves an open band — does not reach the ribbon skirt bottom {foot:?}"
                );
            }
        }
    }
    /// #577 (verify wf_7f36d6ce LOW): the skirt welds even with a SHALLOW skirt on
    /// a CROSS-SLOPE — the case where sampling terrain at one outer point (rather
    /// than the ribbon's `min(g_left, g_right)`) would leave a partial seam. The
    /// fillet carries each ribbon's recorded `skirt_y`, so the foot lands exactly on
    /// the ribbon skirt bottom regardless. Reads the ribbon's own skirt-bottom
    /// vertex (lowest at each outer-curb XZ) and asserts the hub meets it.
    #[test]
    fn hub_fillet_skirt_welds_on_shallow_cross_slope() {
        let config = crate::pds::generator::RoadConfig {
            skirt_depth: crate::pds::types::Fp(0.5),
            ..cfg(7)
        };
        let dims = Dims::from_config(&config);
        let w = dims.minor_half_width;
        let wo = w + dims.curb_top_width + dims.chamfer_width;
        // A ramp in x → each mouth's two outer edges sit at different heights.
        let mut hm = HeightMap::new(96, 96, 2.0);
        let width = hm.width();
        for z in 0..width {
            for x in 0..width {
                hm.set(x, z, x as f32 * 0.3);
            }
        }
        let third = std::f32::consts::TAU / 3.0;
        let chains: Vec<Chain> = (0..3)
            .map(|k| {
                let ang = k as f32 * third;
                let (dx, dz) = (ang.cos(), ang.sin());
                Chain {
                    pts: vec![
                        (90.0, 90.0),
                        (90.0 + dx * 15.0, 90.0 + dz * 15.0),
                        (90.0 + dx * 40.0, 90.0 + dz * 40.0),
                    ],
                    half_w: w,
                    end_nodes: [0, 1 + k],
                    clip: [false, false],
                }
            })
            .collect();
        let mut degree = vec![1u32; 4];
        degree[0] = 3;
        let trims = compute_truncations(&chains, |nd| degree[nd] >= 3, &dims);
        let mut road_ends = Vec::new();
        let mut ribbon = RoadParts::default();
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
                &mut ribbon,
            );
        }
        assert_eq!(road_ends.len(), 3, "the Y must record three mouths");
        let mut hub = RoadParts::default();
        extrude_hubs(&road_ends, &hm, 0.0, &dims, &mut hub);

        // Lowest ribbon vertex at an XZ = that point's skirt bottom.
        let ribbon_floor = |xz: [f32; 2]| {
            ribbon
                .structure
                .vertices
                .iter()
                .filter(|v| (v[0] - xz[0]).abs() < 1.0e-3 && (v[2] - xz[1]).abs() < 1.0e-3)
                .map(|v| v[1])
                .fold(f32::INFINITY, f32::min)
        };
        let hub_has = |p: [f32; 3]| {
            hub.structure.vertices.iter().any(|v| {
                (v[0] - p[0]).abs() < 1.0e-3
                    && (v[1] - p[1]).abs() < 1.0e-3
                    && (v[2] - p[2]).abs() < 1.0e-3
            })
        };
        for e in &road_ends {
            for sgn in [-1.0_f32, 1.0] {
                let xz = [e.cx + sgn * e.rx * wo, e.cz + sgn * e.rz * wo];
                let floor = ribbon_floor(xz);
                assert!(floor.is_finite(), "no ribbon skirt at outer point {xz:?}");
                assert!(
                    hub_has([xz[0], floor, xz[1]]),
                    "fillet skirt foot {:?} did not weld to the ribbon skirt bottom {floor}",
                    [xz[0], floor, xz[1]]
                );
            }
        }
    }

    /// #577: the curb-return fillets are wound front-out — every structure
    /// triangle's geometric normal points away from the hub centre, so back-face
    /// culling keeps the curb/skirt visible from outside (no inside-out corner).
    /// A symmetric Y keeps the centroid at the origin so the radial faces-out
    /// proxy is exact (a skewed hub makes near-tangent faces ambiguous).
    #[test]
    fn hub_fillet_faces_out() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(64, 64, 2.0); // flat at 0 → no upward tent
        let third = std::f32::consts::TAU / 3.0;
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
                skirt_y: -4.0,
            }
        };
        let ends = [arm(0.0), arm(third), arm(2.0 * third)];
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        // The symmetric Y's mouth-corner centroid is the origin; the apex sits at
        // the mean deck height (clamped upward to flat terrain → stays 1.0).
        let center = [
            0.0_f32,
            1.0_f32.max(hm.get_height_at(0.0, 0.0) + ROAD_DEPTH_BIAS_M),
            0.0,
        ];
        let v = &parts.structure.vertices;
        let idx = &parts.structure.indices;
        assert!(!idx.is_empty(), "no fillet structure emitted");
        for tri in idx.chunks_exact(3) {
            let (a, b, c) = (v[tri[0] as usize], v[tri[1] as usize], v[tri[2] as usize]);
            let geo = cross(sub3(b, a), sub3(c, a));
            let mid = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            assert!(
                dot(geo, sub3(mid, center)) > 1.0e-5,
                "fillet triangle faces inward: n·out = {}",
                dot(normalize(geo), normalize(sub3(mid, center)))
            );
        }
    }

    /// #577: the skirt drops the full depth (welding to the ribbon) yet never
    /// inverts — even where the gap terrain humps ABOVE the deck, no structure
    /// pokes above the curb top (the #576 finding-2 clamp keeps the foot under the
    /// deck edge it drops from).
    #[test]
    fn hub_fillet_skirt_never_inverts() {
        let dims = Dims::from_config(&cfg(7));
        // Terrain humped to 2 m — above the deck (1 m) — so a terrain-only foot
        // would rise to ~1.7 (above the deck); the clamp + depth must keep it down.
        let mut hm = HeightMap::new(64, 64, 2.0);
        for c in hm.data_mut() {
            *c = 2.0;
        }
        let third = std::f32::consts::TAU / 3.0;
        let arm = |ang: f32| {
            let (dx, dz) = (ang.cos(), ang.sin());
            RoadEnd {
                node: 0,
                cx: dx * 5.0,
                cz: dz * 5.0,
                rx: -dz,
                rz: dx,
                half_w: 4.0,
                deck_y: 1.0,
                skirt_y: -4.0,
            }
        };
        let ends = [arm(0.0), arm(third), arm(2.0 * third)];
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        let curb_top = 1.0 + dims.curb_height; // highest structure point
        let mut min_y = f32::INFINITY;
        for v in &parts.structure.vertices {
            assert!(
                v[1] <= curb_top + 1.0e-2,
                "structure vertex {v:?} pokes above the curb top (skirt inverted?)"
            );
            min_y = min_y.min(v[1]);
        }
        // The skirt dropped the full depth (welding to the ribbon's deep skirt),
        // well below the deck — NOT clamped to the humped terrain at 1.7.
        let want = 1.0 - dims.skirt_depth; // deck_y − skirt_depth
        assert!(
            (min_y - want).abs() < 0.1,
            "skirt foot {min_y} did not drop the full depth to {want}",
        );
    }

    /// #577: a through road's far edge (two anti-parallel arms with no branch
    /// between) must stay a STRAIGHT curb, not bulge — the straight-gap detector
    /// drops the fillet sagitta to 0 there. Builds a T (arms at 0°/90°/180°) and
    /// checks the −z straight side runs flat at the outer-curb line (z = −wo).
    #[test]
    fn hub_through_road_far_edge_stays_straight() {
        let dims = Dims::from_config(&cfg(7));
        let w = 4.0_f32;
        let wo = w + dims.curb_top_width + dims.chamfer_width;
        let hm = HeightMap::new(64, 64, 2.0); // flat at 0
        let arm = |ang: f32| {
            let (dx, dz) = (ang.cos(), ang.sin());
            RoadEnd {
                node: 0,
                cx: dx * 6.0,
                cz: dz * 6.0,
                rx: -dz,
                rz: dx,
                half_w: w,
                deck_y: 1.0,
                skirt_y: -4.0,
            }
        };
        let (fp2, pi) = (std::f32::consts::FRAC_PI_2, std::f32::consts::PI);
        let ends = [arm(0.0), arm(fp2), arm(pi)];
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        // The straight −z side's outer edge sits at z = −wo; a bulged fillet would
        // push structure past it (more negative z).
        let min_z = parts
            .structure
            .vertices
            .iter()
            .fold(f32::INFINITY, |m, v| m.min(v[2]));
        assert!(
            min_z >= -wo - 1.0e-2,
            "through-road far edge bulged to z={min_z}, past −wo={}",
            -wo
        );
        // ...and its midpoint is present at the deck level on that straight line.
        let mid_present = parts.structure.vertices.iter().any(|v| {
            v[0].abs() < 1.0e-2 && (v[1] - 1.0).abs() < 1.0e-2 && (v[2] + wo).abs() < 1.0e-2
        });
        assert!(
            mid_present,
            "straight through-road far-edge midpoint missing"
        );
    }

    /// #577 (review wf_55dafda9 HIGH): on a SLOPED / asymmetric hub the fillet
    /// strip is non-planar (its inner edge rides a deck chord that runs between two
    /// different mouth heights), so a single per-strip winding decision back-winds
    /// some triangles. Every emitted structure triangle's geometric winding must
    /// agree with its (outward) stored shading normal — the per-segment winding
    /// guarantees it. A symmetric flat Y never twists, so this needs varying deck_y.
    #[test]
    fn hub_fillet_winding_consistent_on_sloped_hub() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(64, 64, 2.0); // flat terrain; the SLOPE is in deck_y
        let third = std::f32::consts::TAU / 3.0;
        // Adjacent mouths at clearly different deck heights (1 / 2 / 4) + asymmetric
        // pull-backs → the fillet strips slope and curve (the twisting regime).
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
                skirt_y: deck_y - 5.0,
            }
        };
        let ends = [
            arm(0.0, 5.0, 1.0),
            arm(third, 8.0, 2.0),
            arm(2.0 * third, 4.0, 4.0),
        ];
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        let v = &parts.structure.vertices;
        let nrm = &parts.structure.normals;
        let idx = &parts.structure.indices;
        assert!(!idx.is_empty(), "no fillet structure emitted");
        let mut backwound = 0;
        for tri in idx.chunks_exact(3) {
            let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let geo = cross(sub3(v[ib], v[ia]), sub3(v[ic], v[ia]));
            // Average the three stored (outward) shading normals.
            let avg = [
                nrm[ia][0] + nrm[ib][0] + nrm[ic][0],
                nrm[ia][1] + nrm[ib][1] + nrm[ic][1],
                nrm[ia][2] + nrm[ib][2] + nrm[ic][2],
            ];
            if dot(geo, avg) <= 0.0 {
                backwound += 1;
            }
        }
        assert_eq!(
            backwound, 0,
            "{backwound} fillet triangles are wound against their outward normal (mis-shaded)"
        );
    }

    /// #577 (review wf_55dafda9): the per-segment winding must hold on the REAL
    /// pilot network — every hub there is skewed (asymmetric truncation, mouths at
    /// different draped heights, ~23° acute branches), the regime a single
    /// per-strip winding decision got wrong (~9% of structure tris). Isolates the
    /// hub fillets (extrude_hubs into its own buffer) and asserts no fillet triangle
    /// is wound against its outward shading normal.
    #[test]
    fn pilot_hub_fillets_are_wound_consistently() {
        let hm = pilot_heightmap();
        let config = cfg(PILOT_ROAD_SEED);
        let (graph, sub, lo) = build_road_graph(&hm, &config).expect("pilot must trace");
        let dims = Dims::from_config(&config);
        let chains = extract_chains(&graph, &sub, &dims);
        let mut degree = vec![0u32; graph.nodes.len()];
        for e in &graph.edges {
            if e.active {
                degree[e.start as usize] += 1;
                degree[e.end as usize] += 1;
            }
        }
        let trims = compute_truncations(
            &chains,
            |nd| degree.get(nd).copied().unwrap_or(0) >= 3,
            &dims,
        );
        let world_offset = lo as f32 * sub.scale();
        let mut road_ends = Vec::new();
        let mut ribbon = RoadParts::default();
        for (ci, c) in chains.iter().enumerate() {
            let [s, e] = trims[ci];
            extrude_chain(
                c,
                s,
                e,
                &sub,
                world_offset,
                &dims,
                &degree,
                &mut road_ends,
                &mut ribbon,
            );
        }
        let mut hub = RoadParts::default();
        extrude_hubs(&road_ends, &sub, world_offset, &dims, &mut hub);

        let v = &hub.structure.vertices;
        let nrm = &hub.structure.normals;
        assert!(
            !hub.structure.indices.is_empty(),
            "pilot grew no hub fillets"
        );
        let mut backwound = 0;
        for tri in hub.structure.indices.chunks_exact(3) {
            let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let geo = normalize(cross(sub3(v[ib], v[ia]), sub3(v[ic], v[ia])));
            let avg = normalize([
                nrm[ia][0] + nrm[ib][0] + nrm[ic][0],
                nrm[ia][1] + nrm[ib][1] + nrm[ic][1],
                nrm[ia][2] + nrm[ib][2] + nrm[ic][2],
            ]);
            // Tolerance skips genuinely degenerate (zero-area) slivers; a real
            // back-wound triangle reads clearly negative.
            if dot(geo, avg) < -0.01 {
                backwound += 1;
            }
        }
        assert_eq!(
            backwound, 0,
            "{backwound} pilot hub-fillet triangles wound against their normal"
        );
    }

    /// #577 (review wf_55dafda9 MEDIUM): on an ASYMMETRIC hub (differing per-arm
    /// half-widths and pull-backs) the fillet arc must still start/end EXACTLY on
    /// each ribbon's outer-curb point — the old `centre = mid − bd·(r−sag)` drifted
    /// the endpoints (sub-metre notch) whenever `bd` was not perpendicular to the
    /// outer-curb chord, which is the norm off a symmetric Y.
    #[test]
    fn hub_fillet_endpoints_exact_on_asymmetric_hub() {
        let dims = Dims::from_config(&cfg(7));
        let (ct, cf) = (dims.curb_top_width, dims.chamfer_width);
        let hm = HeightMap::new(64, 64, 2.0);
        // Deliberately skewed: different angles, half-widths, pull-backs, heights.
        let arm = |ang_deg: f32, t: f32, hw: f32, deck_y: f32| {
            let a = ang_deg.to_radians();
            let (dx, dz) = (a.cos(), a.sin());
            RoadEnd {
                node: 0,
                cx: dx * t,
                cz: dz * t,
                rx: -dz,
                rz: dx,
                half_w: hw,
                deck_y,
                skirt_y: deck_y - 5.0,
            }
        };
        let ends = [
            arm(10.0, 3.0, 6.0, 1.0),
            arm(130.0, 9.0, 3.0, 2.5),
            arm(250.0, 5.0, 5.0, 1.5),
        ];
        let mut parts = RoadParts::default();
        extrude_hubs(&ends, &hm, 0.0, &dims, &mut parts);

        let near = |p: [f32; 3]| {
            parts.structure.vertices.iter().any(|v| {
                (v[0] - p[0]).abs() < 1.0e-3
                    && (v[1] - p[1]).abs() < 1.0e-3
                    && (v[2] - p[2]).abs() < 1.0e-3
            })
        };
        for e in &ends {
            let wo = e.half_w + ct + cf;
            for sgn in [-1.0_f32, 1.0] {
                let o = [e.cx + sgn * e.rx * wo, e.deck_y, e.cz + sgn * e.rz * wo];
                assert!(
                    near(o),
                    "asymmetric fillet missed the ribbon outer-curb point {o:?} (endpoint drift)"
                );
            }
        }
    }

    /// #579: a degree-1 dead-end gets a flat cross-section cap (closing the open
    /// hollow tube), facing outward; an UNclipped degree-2 end does NOT (a mid-run
    /// node, loop closure or used-edge break — perimeter clips are #582's job and
    /// carry `clip=true`, set false throughout here). The cap faces along the road
    /// tangent (±x for an x-running chain), HORIZONTAL — no ribbon face does (deck
    /// +y, curb/skirt ±z lateral) — so counting its ±x normals uniquely detects it.
    #[test]
    fn dead_end_gets_a_cross_section_cap() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(64, 64, 2.0); // flat at 0
        let chain = Chain {
            pts: vec![(20.0, 20.0), (30.0, 20.0), (50.0, 20.0)],
            half_w: dims.minor_half_width,
            end_nodes: [0, 1],
            clip: [false, false],
        };
        // Cap normals for a given per-node degree: horizontal (|n.y|≈0), facing ±x.
        let cap_x = |degree: &[u32]| -> Vec<f32> {
            let mut road_ends = Vec::new();
            let mut parts = RoadParts::default();
            extrude_chain(
                &chain,
                0.0,
                0.0,
                &hm,
                0.0,
                &dims,
                degree,
                &mut road_ends,
                &mut parts,
            );
            for v in &parts.structure.vertices {
                assert!(
                    v.iter().all(|c| c.is_finite()),
                    "non-finite cap vertex {v:?}"
                );
            }
            parts
                .structure
                .normals
                .iter()
                .filter(|n| n[0].abs() > 0.9 && n[1].abs() < 0.05)
                .map(|n| n[0])
                .collect()
        };
        // degree-1 START → one cap (10 verts) facing −x; degree-2 end → none.
        let start = cap_x(&[1, 2]);
        assert_eq!(
            start.len(),
            10,
            "degree-1 start: expected one capped cross-section"
        );
        assert!(
            start.iter().all(|&x| x < 0.0),
            "start cap must face −x (outward)"
        );
        // degree-1 END → one cap facing +x (exercises the slot-last path + sign).
        let end = cap_x(&[2, 1]);
        assert_eq!(
            end.len(),
            10,
            "degree-1 end: expected one capped cross-section"
        );
        assert!(
            end.iter().all(|&x| x > 0.0),
            "end cap must face +x (outward)"
        );
        // Both ends degree-2 (district clips) → no caps.
        assert_eq!(
            cap_x(&[2, 2]).len(),
            0,
            "district-edge clips wrongly capped"
        );
        // A junction end (≥3) is closed by its hub, not a cap.
        assert_eq!(cap_x(&[1, 3]).len(), 10, "only the degree-1 end caps");
    }

    /// #579 (review wf_aabe1626 HIGH): the cap's explicit triangulation must TILE
    /// the concave profile exactly — no gap, no overlap, no spill past the
    /// silhouette (the bug the apex-fan had). The cross-section is rigid, so the
    /// summed triangle areas must equal the profile polygon's shoelace area.
    #[test]
    fn dead_end_cap_triangulation_tiles_the_profile() {
        let dims = Dims::from_config(&cfg(7));
        let prof = profile(dims.minor_half_width, &dims);
        let tri_area = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
            ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs() * 0.5
        };
        // The exact triangulation push_end_cap emits (body + two curb wedges).
        let tris = [
            [7, 4, 5],
            [7, 5, 6],
            [1, 2, 3],
            [1, 3, 4],
            [7, 8, 9],
            [7, 9, 0],
        ];
        let tri_sum: f32 = tris
            .iter()
            .map(|t| tri_area(prof[t[0]], prof[t[1]], prof[t[2]]))
            .sum();
        let mut shoelace = 0.0_f32;
        for i in 0..prof.len() {
            let (a, b) = (prof[i], prof[(i + 1) % prof.len()]);
            shoelace += a.0 * b.1 - b.0 * a.1;
        }
        let poly = shoelace.abs() * 0.5;
        assert!(
            (tri_sum - poly).abs() < 1.0e-4,
            "cap triangulation does not tile the profile: triangles {tri_sum} vs polygon {poly}"
        );
        assert!(poly > 0.0, "degenerate profile");
    }

    /// #579 (review wf_aabe1626): the cap is a VERTICAL cross-section, so its normal
    /// must stay HORIZONTAL on sloped terrain — using the road tangent would tilt it
    /// by the longitudinal grade and mis-shade the cul-de-sac on a hill.
    #[test]
    fn dead_end_cap_normal_is_horizontal_on_a_slope() {
        let dims = Dims::from_config(&cfg(7));
        // A ramp in x → the deck/skirt grade is non-zero along the road.
        let mut hm = HeightMap::new(64, 64, 2.0);
        let w = hm.width();
        for z in 0..w {
            for x in 0..w {
                hm.set(x, z, x as f32 * 0.5);
            }
        }
        let chain = Chain {
            pts: vec![(20.0, 20.0), (35.0, 20.0), (60.0, 20.0)],
            half_w: dims.minor_half_width,
            end_nodes: [0, 1],
            clip: [false, false],
        };
        let degree = vec![1u32, 2u32];
        let mut road_ends = Vec::new();
        let mut parts = RoadParts::default();
        extrude_chain(
            &chain,
            0.0,
            0.0,
            &hm,
            0.0,
            &dims,
            &degree,
            &mut road_ends,
            &mut parts,
        );
        // Cap normals face ±x strongly; the grade-limited deck/curb never exceeds
        // |n.x| 0.5, so this isolates the cap.
        let caps: Vec<_> = parts
            .structure
            .normals
            .iter()
            .filter(|n| n[0].abs() > 0.5)
            .collect();
        assert!(!caps.is_empty(), "no cap emitted on sloped terrain");
        for n in caps {
            assert!(
                n[1].abs() < 1.0e-3,
                "cap normal not horizontal on a slope: {n:?}"
            );
            let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
            assert!((len2 - 1.0).abs() < 1.0e-3, "cap normal not unit: {n:?}");
        }
    }

    /// #582: a boundary-clip end (a road running off the network perimeter) is
    /// capped like a dead-end even though its node is degree-2 — the cap is driven
    /// by `chain.clip[slot]`, independent of degree. Same ±x-horizontal-normal
    /// signature as the #579 dead-end cap, so counting those isolates it.
    #[test]
    fn clip_end_emits_a_cap_cross_section() {
        let dims = Dims::from_config(&cfg(7));
        let hm = HeightMap::new(64, 64, 2.0); // flat at 0
        // Degree-2 at BOTH ends: no dead-end, no junction → caps depend purely on
        // the clip flags, isolating the #582 path from the #579 degree path.
        let degree = vec![2u32, 2u32];
        let cap_x = |clip: [bool; 2]| -> Vec<f32> {
            let chain = Chain {
                pts: vec![(20.0, 20.0), (30.0, 20.0), (50.0, 20.0)],
                half_w: dims.minor_half_width,
                end_nodes: [0, 1],
                clip,
            };
            let mut road_ends = Vec::new();
            let mut parts = RoadParts::default();
            extrude_chain(
                &chain,
                0.0,
                0.0,
                &hm,
                0.0,
                &dims,
                &degree,
                &mut road_ends,
                &mut parts,
            );
            for v in &parts.structure.vertices {
                assert!(
                    v.iter().all(|c| c.is_finite()),
                    "non-finite cap vertex {v:?}"
                );
            }
            parts
                .structure
                .normals
                .iter()
                .filter(|n| n[0].abs() > 0.9 && n[1].abs() < 0.05)
                .map(|n| n[0])
                .collect()
        };
        // clip START only → one cap (10 verts) facing −x (outward).
        let start = cap_x([true, false]);
        assert_eq!(
            start.len(),
            10,
            "clip start: expected one capped cross-section"
        );
        assert!(
            start.iter().all(|&x| x < 0.0),
            "clip start cap must face −x"
        );
        // clip END only → one cap facing +x (exercises slot-last + sign).
        let end = cap_x([false, true]);
        assert_eq!(end.len(), 10, "clip end: expected one capped cross-section");
        assert!(end.iter().all(|&x| x > 0.0), "clip end cap must face +x");
        // No clip, degree-2 both ends → still open (regression lock: the #582 bug
        // was exactly this end left uncapped). A loop closure / used-edge break
        // arrives here as clip=[false,false] and must stay open.
        assert_eq!(
            cap_x([false, false]).len(),
            0,
            "non-clip degree-2 ends must stay open"
        );
        // Both ends clipped (a fully-perimeter sliver) → two caps.
        assert_eq!(cap_x([true, true]).len(), 20, "both rim ends must cap");
    }

    /// #582 (mirrors the #579 review wf_aabe1626 finding): a clip cap is a VERTICAL
    /// cross-section, so its normal must stay HORIZONTAL on sloped terrain — the
    /// road tangent would tilt it by the longitudinal grade and mis-shade the
    /// perimeter end on a hill. Driven through the clip path (degree-2 end).
    #[test]
    fn clip_end_cap_normal_is_horizontal_on_a_slope() {
        let dims = Dims::from_config(&cfg(7));
        let mut hm = HeightMap::new(64, 64, 2.0);
        let w = hm.width();
        for z in 0..w {
            for x in 0..w {
                hm.set(x, z, x as f32 * 0.5); // ramp in x → non-zero deck grade
            }
        }
        let chain = Chain {
            pts: vec![(20.0, 20.0), (35.0, 20.0), (60.0, 20.0)],
            half_w: dims.minor_half_width,
            end_nodes: [0, 1],
            clip: [false, true], // far end runs off the rim
        };
        let degree = vec![2u32, 2u32];
        let mut road_ends = Vec::new();
        let mut parts = RoadParts::default();
        extrude_chain(
            &chain,
            0.0,
            0.0,
            &hm,
            0.0,
            &dims,
            &degree,
            &mut road_ends,
            &mut parts,
        );
        let caps: Vec<_> = parts
            .structure
            .normals
            .iter()
            .filter(|n| n[0].abs() > 0.5)
            .collect();
        assert!(!caps.is_empty(), "no clip cap emitted on sloped terrain");
        for n in caps {
            assert!(
                n[1].abs() < 1.0e-3,
                "clip cap normal not horizontal on a slope: {n:?}"
            );
            let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
            assert!(
                (len2 - 1.0).abs() < 1.0e-3,
                "clip cap normal not unit: {n:?}"
            );
        }
    }

    /// #582: when a run is cut because the next sampled node falls outside the
    /// district interior, that end is flagged `clip`; the run's other end, a real
    /// graph terminus, is not.
    #[test]
    fn push_interior_runs_marks_a_boundary_clip_end() {
        // Nodes 0,1,2 inside (x<100); node 3 outside → the run clips at the rim.
        let pos = |i: usize| (i as f32 * 40.0, 0.0); // 0, 40, 80, 120
        let inside = |x: f32, _z: f32| x < 100.0;
        let mut out = Vec::new();
        push_interior_runs(&[0, 1, 2, 3], &pos, &inside, 5.0, &mut out);
        assert_eq!(out.len(), 1, "one interior sub-run");
        assert_eq!(out[0].end_nodes, [0, 2]);
        assert_eq!(
            out[0].clip,
            [false, true],
            "start is a real terminus, end is clipped at the rim"
        );
    }

    /// #582 (review risk: re-entrant bookkeeping): a street that dips out of the
    /// interior and back in (in→out→in) yields two runs whose INNER ends — both at
    /// the rim — are clipped, while the outer ends stay real termini. `prev_outside`
    /// must be function-local so the second run picks up the clipped start.
    #[test]
    fn push_interior_runs_reentrant_street_marks_both_inner_ends() {
        let xs = [0.0_f32, 40.0, 120.0, 200.0, 240.0]; // node 2 outside the band
        let pos = move |i: usize| (xs[i], 0.0);
        let inside = |x: f32, _z: f32| x < 100.0 || x > 150.0;
        let mut out = Vec::new();
        push_interior_runs(&[0, 1, 2, 3, 4], &pos, &inside, 5.0, &mut out);
        assert_eq!(out.len(), 2, "two interior sub-runs straddling the gap");
        assert_eq!(out[0].end_nodes, [0, 1]);
        assert_eq!(out[0].clip, [false, true], "first run clips at its rim end");
        assert_eq!(out[1].end_nodes, [3, 4]);
        assert_eq!(
            out[1].clip,
            [true, false],
            "second run clips at its rim start"
        );
    }

    /// #582 (review risk: first-node-outside): a walked chain whose FIRST node is
    /// outside must not flush an empty run nor mis-set the flag, but the following
    /// run must still pick up the clipped start from `prev_outside`.
    #[test]
    fn push_interior_runs_leading_outside_marks_start_clip() {
        let xs = [120.0_f32, 0.0, 40.0, 80.0]; // node 0 outside, then 1,2,3 inside
        let pos = move |i: usize| (xs[i], 0.0);
        let inside = |x: f32, _z: f32| x < 100.0;
        let mut out = Vec::new();
        push_interior_runs(&[0, 1, 2, 3], &pos, &inside, 5.0, &mut out);
        assert_eq!(
            out.len(),
            1,
            "one interior sub-run after the leading-outside node"
        );
        assert_eq!(out[0].end_nodes, [1, 3]);
        assert_eq!(
            out[0].clip,
            [true, false],
            "leading-outside → clipped start, real end"
        );
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
            clip: [false, false],
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
            clip: [false, false],
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
                clip: [false, false],
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
            clip: [false, false],
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
            clip: [false, false],
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
                clip: [false, false],
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
            clip: [false, false],
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
