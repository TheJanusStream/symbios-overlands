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
    RationalizeConfig, RoadGraph, RoadType, TensorConfig, generate_roads, rationalize_graph,
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
/// Drop edges whose endpoints fall beyond this fraction of the district
/// half-extent, so the network ends in the interior, not at the visible edge.
const ROAD_INTERIOR_FRACTION: f32 = 0.88;
/// World metres per UV tile, both along the road and around the cross-section.
const UV_TILE_M: f32 = 6.0;

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

/// Engine-agnostic road geometry (Y-up), built CPU-side in the terrain task and
/// uploaded by the caller. Per-face flat normals (matches the low-poly look).
#[derive(Default)]
pub struct RoadGeometry {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

/// One continuous road run (between intersections / endpoints), as an ordered
/// XZ polyline plus the deck half-width for its road class.
struct Chain {
    pts: Vec<(f32, f32)>,
    half_w: f32,
}

/// Build terrain-conforming road geometry from a [`RoadConfig`], or `None` if
/// the config is disabled or the tracer can't produce a network. Deterministic
/// in `config.seed`. Does **not** modify `hm` — the road drapes over the
/// natural terrain. Which rooms *get* a road config is the seeding layer's
/// policy ([`crate::pds::room`]); this just renders whatever it's handed.
pub fn build_road_geometry(hm: &HeightMap, config: &RoadConfig) -> Option<RoadGeometry> {
    if !config.enabled {
        return None;
    }
    let dims = Dims::from_config(config);
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

    let chains = extract_chains(&graph, &sub, &dims);
    let mut geo = RoadGeometry::default();
    let world_offset = lo as f32 * scale;
    for chain in &chains {
        extrude_chain(chain, &sub, world_offset, &dims, &mut geo);
    }
    // Fill the intersections the chains leave open.
    extrude_junctions(&graph, &sub, world_offset, &dims, &mut geo);
    (!geo.vertices.is_empty()).then_some(geo)
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
    let mut run: Vec<(f32, f32)> = Vec::new();
    let flush = |run: &mut Vec<(f32, f32)>, out: &mut Vec<Chain>| {
        if run.len() >= 2 {
            out.push(Chain {
                pts: std::mem::take(run),
                half_w,
            });
        } else {
            run.clear();
        }
    };
    for &nd in nodes {
        let (x, z) = pos(nd);
        if inside(x, z) {
            run.push((x, z));
        } else {
            flush(&mut run, out);
        }
    }
    flush(&mut run, out);
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

/// Extrude the curb/skirt profile along one chain into `geo`, draping the deck
/// over the terrain (`hm`) and shifting into the full-terrain frame by
/// `world_offset`.
fn extrude_chain(
    chain: &Chain,
    hm: &HeightMap,
    world_offset: f32,
    dims: &Dims,
    geo: &mut RoadGeometry,
) {
    let pts = densify(&chain.pts, RIBBON_STEP_M);
    if pts.len() < 2 {
        return;
    }
    let prof = profile(chain.half_w, dims);

    // Per-vertex frame. The deck *banks* with the terrain cross-slope: we
    // sample both deck edges and store a base height + lateral slope, so the
    // cross-section tilts to hug the ground (a flat-across deck floats its
    // uphill edge into the hillside). `base_y` is the height at the centreline,
    // `slope_u` the rise per unit lateral offset.
    struct Frame {
        cx: f32,
        cz: f32,
        rx: f32,
        rz: f32,
        scale: f32,
        base_y: f32,
        slope_u: f32,
        arc: f32,
    }
    let mut frames = Vec::with_capacity(pts.len());
    let mut arc = 0.0;
    for i in 0..pts.len() {
        let (cx, cz) = pts[i];
        if i > 0 {
            arc += (cx - pts[i - 1].0).hypot(cz - pts[i - 1].1);
        }
        let (rx, rz, scale) = frame_right(&pts, i);
        // Sample terrain at the two deck edges (±half_w along the lateral axis).
        let (ex, ez) = (rx * chain.half_w * scale, rz * chain.half_w * scale);
        let h_l = hm.get_height_at(cx - ex, cz - ez);
        let h_r = hm.get_height_at(cx + ex, cz + ez);
        frames.push(Frame {
            cx,
            cz,
            rx,
            rz,
            scale,
            base_y: (h_l + h_r) * 0.5 + ROAD_DEPTH_BIAS_M,
            slope_u: (h_r - h_l) / (2.0 * chain.half_w),
            arc,
        });
    }

    // Cumulative cross-section perimeter, for the U coordinate.
    let mut u = [0.0_f32; 10];
    for j in 1..10 {
        let (a, b) = (prof[j - 1], prof[j]);
        u[j] = u[j - 1] + (b.0 - a.0).hypot(b.1 - a.1);
    }

    let world = |f: &Frame, p: (f32, f32)| {
        [
            f.cx + f.rx * (p.0 * f.scale) + world_offset,
            f.base_y + f.slope_u * p.0 + p.1,
            f.cz + f.rz * (p.0 * f.scale) + world_offset,
        ]
    };

    for j in 0..10 {
        let k = (j + 1) % 10;
        let (uj, uk) = (u[j] / UV_TILE_M, u[k] / UV_TILE_M);
        for i in 0..frames.len() - 1 {
            let (f0, f1) = (&frames[i], &frames[i + 1]);
            let a = world(f0, prof[j]);
            let b = world(f0, prof[k]);
            let c = world(f1, prof[j]);
            let d = world(f1, prof[k]);

            // Flat per-face normal, flipped to point away from the beam axis.
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let mut nrm = cross(e1, e2);
            let axis = [
                (f0.cx + f1.cx) * 0.5 + world_offset,
                (f0.base_y + f1.base_y) * 0.5 - dims.skirt_depth * 0.5,
                (f0.cz + f1.cz) * 0.5 + world_offset,
            ];
            let fc = [
                (a[0] + b[0] + c[0] + d[0]) * 0.25,
                (a[1] + b[1] + c[1] + d[1]) * 0.25,
                (a[2] + b[2] + c[2] + d[2]) * 0.25,
            ];
            let outward = [fc[0] - axis[0], fc[1] - axis[1], fc[2] - axis[2]];
            if dot(nrm, outward) < 0.0 {
                nrm = [-nrm[0], -nrm[1], -nrm[2]];
            }
            let nrm = normalize(nrm);

            let (vi, vi1) = (f0.arc / UV_TILE_M, f1.arc / UV_TILE_M);
            let base = geo.vertices.len() as u32;
            geo.vertices.extend_from_slice(&[a, b, c, d]);
            geo.uvs
                .extend_from_slice(&[[uj, vi], [uk, vi], [uj, vi1], [uk, vi1]]);
            for _ in 0..4 {
                geo.normals.push(nrm);
            }
            // a=base, b=base+1, c=base+2, d=base+3 → quad (a,b,d,c).
            geo.indices
                .extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
        }
    }
}

/// Fill 3+-way intersections with a small terrain-draped polygon, so chains
/// that end at a junction don't leave wedge gaps where their flat end-caps meet
/// at an angle. Degree-1/2 nodes are covered by the chains themselves.
fn extrude_junctions(
    graph: &RoadGraph,
    hm: &HeightMap,
    world_offset: f32,
    dims: &Dims,
    geo: &mut RoadGeometry,
) {
    let center = hm.width() as f32 * hm.scale() * 0.5;
    let interior_r2 = (center * ROAD_INTERIOR_FRACTION).powi(2);

    let n = graph.nodes.len();
    let mut degree = vec![0u32; n];
    let mut max_hw = vec![0.0_f32; n];
    for e in &graph.edges {
        if !e.active {
            continue;
        }
        let hw = match &e.road_type {
            RoadType::Major => dims.major_half_width,
            RoadType::Minor => dims.minor_half_width,
        };
        for &nd in &[e.start as usize, e.end as usize] {
            degree[nd] += 1;
            max_hw[nd] = max_hw[nd].max(hw);
        }
    }

    const RING: u32 = 14;
    for i in 0..n {
        if degree[i] < 3 {
            continue;
        }
        let p = graph.nodes[i].position;
        let (cx, cz) = (p.x, p.y);
        let (dx, dz) = (cx - center, cz - center);
        if dx * dx + dz * dz > interior_r2 {
            continue;
        }
        // Cover the incident curbs; sit a hair above the deck so the fan wins
        // the depth test over the abutting chain ends rather than z-fighting.
        let radius = max_hw[i] + dims.curb_top_width + dims.chamfer_width;
        let lift = ROAD_DEPTH_BIAS_M + 0.03;
        let base = geo.vertices.len() as u32;
        geo.vertices.push([
            cx + world_offset,
            hm.get_height_at(cx, cz) + lift,
            cz + world_offset,
        ]);
        geo.normals.push([0.0, 1.0, 0.0]);
        geo.uvs.push([0.5, 0.5]);
        for k in 0..=RING {
            let a = k as f32 / RING as f32 * std::f32::consts::TAU;
            let (px, pz) = (cx + a.cos() * radius, cz + a.sin() * radius);
            geo.vertices.push([
                px + world_offset,
                hm.get_height_at(px, pz) + lift,
                pz + world_offset,
            ]);
            geo.normals.push([0.0, 1.0, 0.0]);
            geo.uvs.push([a.cos() * 0.5 + 0.5, a.sin() * 0.5 + 0.5]);
        }
        for k in 0..RING {
            geo.indices
                .extend_from_slice(&[base, base + 1 + k, base + 2 + k]);
        }
    }
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

    #[test]
    fn default_config_actually_produces_a_network() {
        // Regression guard: the other tests tolerate `None`; this asserts the
        // shipped default config genuinely yields road geometry on sloped
        // terrain, so a config/clip change can't silently render nothing.
        let geo = build_road_geometry(&sloped_heightmap(), &cfg(7))
            .expect("default road config must produce a network on sloped terrain");
        assert!(!geo.vertices.is_empty());
        assert!(!geo.indices.is_empty());
    }

    #[test]
    fn produces_a_network_at_room_scale_for_the_pilot_seed() {
        // The pilot room (terrain seed 4167901772298833237, cyberpunk) at the
        // real ~1 km scale + its derived road seed (room ^ ROAD_SEED_SALT).
        // Guards against the windowed path yielding an empty network there.
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
        let road_seed = 4167901772298833237_u64 ^ 0xA0D5_EED5_A170_0001;
        let geo = build_road_geometry(&hm, &cfg(road_seed))
            .expect("room-scale build for the pilot seed must produce roads");
        assert!(!geo.vertices.is_empty());
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
                assert_eq!(x.vertices, y.vertices, "road geometry non-deterministic");
                assert_eq!(x.indices, y.indices, "road topology non-deterministic");
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

    /// Every emitted vertex must be finite — a NaN from a degenerate miter or
    /// normalize would poison the mesh.
    #[test]
    fn geometry_is_finite() {
        if let Some(geo) = build_road_geometry(&sloped_heightmap(), &cfg(7)) {
            assert!(!geo.vertices.is_empty());
            for v in &geo.vertices {
                assert!(v.iter().all(|c| c.is_finite()), "non-finite vertex {v:?}");
            }
            for nrm in &geo.normals {
                assert!(nrm.iter().all(|c| c.is_finite()), "non-finite normal");
            }
        }
    }
}
