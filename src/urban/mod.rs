//! Tensor-field urban layout — deterministic, terrain-conforming road networks
//! for urban-themed rooms (cyberpunk pilot).
//!
//! `symbios-tensor` is used purely as a road-**topology** generator: we take
//! its tensor-field [`symbios_tensor::RoadGraph`] and build our own road geometry that *drapes*
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
//! skirt that drops a fixed depth below the deck and is capped by a textured
//! bottom — so where the road runs out high over a dip the underside floats
//! clear as a bridge, not a hollow strip.
//!
//! Generation is localized to a district window around spawn (the seeded
//! settlement only reaches ~140 m) and clipped to the district interior so no
//! street runs off to the visible edge. Everything is deterministic in the
//! room's terrain seed and recomputed at load, never stored — like the
//! heightmap itself.
//!
//! `symbios-tensor` consumes a `symbios_ground::HeightMap`; overlands' own
//! [`bevy_symbios_ground::HeightMap`] is the same crate/type — both crates
//! resolve to the same `symbios-ground` version, so the heightmap passes
//! straight through with no conversion.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;
use symbios_tensor::{LotConfig, extract_blocks, extract_lots};

use crate::pds::generator::RoadConfig;

mod chains;
mod diagnostics;
mod graph;
mod hubs;
mod levelling;
mod math;
mod ribbon;
#[cfg(test)]
pub(crate) mod test_support;
mod truncation;

use crate::urban::graph::build_road_graph;
use crate::urban::math::normalize;

pub(crate) use crate::urban::chains::{Chain, extract_chains};
pub use crate::urban::diagnostics::{RoadDiagnostics, RoadGraphStats, road_graph_diagnostics};
pub(crate) use crate::urban::hubs::{RoadEnd, extrude_hubs};
pub(crate) use crate::urban::levelling::{
    ChainSample, junction_mouth_spreads, level_chain, level_network, sample_chain,
};
pub(crate) use crate::urban::ribbon::{
    RIBBON_STEP_M, UV_TILE_M, densify, extrude_ribbon, frame_right, quad_normal,
};
pub(crate) use crate::urban::truncation::{compute_truncations, trim_polyline};

// --- Tuning -----------------------------------------------------------------
//
// The authorable knobs (district extent, road spacing/widths, curb + skirt
// dimensions, layout seed) live on [`RoadConfig`] in the room record. The
// constants below are pure *rendering* details with no gameplay/aesthetic
// reason to vary per room, so they stay in code.

/// Lift (m) of the deck above the sampled terrain — keeps the deck clear of the
/// ground and the curb framing it proud.
pub(crate) const ROAD_DEPTH_BIAS_M: f32 = 0.2;
/// Drop edges whose endpoints fall beyond this fraction of the district
/// half-extent, so the network ends in the interior, not at the visible edge.
pub(crate) const ROAD_INTERIOR_FRACTION: f32 = 0.88;

/// Resolved per-room road dimensions, pulled out of [`RoadConfig`]'s fixed-point
/// fields once so the geometry builders take plain `f32`s.
#[derive(Clone, Copy)]
pub(crate) struct Dims {
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
    /// Street (chain) count of the built network — for the editor's stats
    /// readout (#888), not the geometry.
    pub chains: usize,
    /// Junction (active degree ≥ 3) count — see [`Self::chains`].
    pub junctions: usize,
}

impl RoadParts {
    /// Total vertex count across the three surfaces — the editor's
    /// mesh-weight readout (#888).
    pub fn vertex_count(&self) -> usize {
        self.deck.vertices.len() + self.structure.vertices.len() + self.neon.vertices.len()
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
    let world_offset = [lo[0] as f32 * sub.scale(), lo[1] as f32 * sub.scale()];

    // Sample each chain's terrain ONCE (the only heightmap-sampling site), then
    // resolve flat junction heights + the per-chain deck heights network-wide
    // (#584). The mesh pass consumes the cached sample + resolved heights, so the
    // pre-pass and the ribbon agree to the bit (no floor-drift seam at the mouths).
    let samples: Vec<Option<ChainSample>> = chains
        .iter()
        .zip(&trims)
        .map(|(chain, &[s, e])| sample_chain(chain, s, e, &sub))
        .collect();
    let base_ys = level_network(&chains, &samples, &degree, &sub);

    // Each chain extrudes its ribbon and records its end-frames at junctions, so
    // the hubs can be built to meet every incident road at its exact (levelled) mouth.
    let mut road_ends: Vec<RoadEnd> = Vec::new();
    for (ci, chain) in chains.iter().enumerate() {
        if let Some(sample) = &samples[ci] {
            extrude_ribbon(
                chain,
                sample,
                &base_ys[ci],
                world_offset,
                &dims,
                &degree,
                &mut road_ends,
                &mut parts,
            );
        }
    }
    extrude_hubs(&road_ends, &sub, world_offset, &dims, &mut parts);
    // Editor stats (#888): streets = chains, junctions = active degree ≥ 3.
    parts.chains = chains.len();
    parts.junctions = degree.iter().filter(|&&d| d >= 3).count();
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
    // footprint placed there lands exactly on its street. Per-axis since the
    // district centre offset (#889) can shift the window asymmetrically.
    let scale = sub.scale();
    let half = hm.width().saturating_sub(1) as f32 * scale * 0.5;
    let shift = [lo[0] as f32 * scale - half, lo[1] as f32 * scale - half];
    lots.into_iter()
        .map(|l| BuildingLot {
            position: [l.position.x + shift[0], l.position.y + shift[1]],
            // tensor measures the lot's rotation in the XZ (top-down) plane;
            // placement yaw is around +Y, the opposite winding sense.
            yaw: -l.rotation,
            width: l.width,
            depth: l.depth,
        })
        .collect()
}

/// A street-furniture spot (#893) in the room placement frame: a point just
/// outside a street's curb line, facing the road.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnitureSpot {
    /// Prop centre, room-centred XZ.
    pub position: [f32; 2],
    /// Yaw (radians, around +Y) turning the prop's authored front (+Z)
    /// toward the street.
    pub yaw: f32,
}

/// Clearance (m) between the outer curb footprint and a furniture prop, so
/// lamps never merge into the chamfer (the coplanar z-fight lesson).
const FURNITURE_CURB_CLEARANCE_M: f32 = 0.6;

/// Extract street-furniture spots (#893): a point every `spacing` metres of
/// arc along each chain, alternating sides, offset outside the curb's outer
/// footprint. Deterministic in the config (pure geometry — no RNG here; the
/// injector's seeded stream picks *which* prop stands at each spot).
pub fn extract_furniture_spots(hm: &HeightMap, config: &RoadConfig) -> Vec<FurnitureSpot> {
    if !config.furniture.enabled {
        return Vec::new();
    }
    let Some((graph, sub, lo)) = build_road_graph(hm, config) else {
        return Vec::new();
    };
    let dims = Dims::from_config(config);
    let chains = extract_chains(&graph, &sub, &dims);
    let spacing = config.furniture.spacing.0.max(1.0);

    let scale = sub.scale();
    let half = hm.width().saturating_sub(1) as f32 * scale * 0.5;
    let shift = [lo[0] as f32 * scale - half, lo[1] as f32 * scale - half];

    let mut spots = Vec::new();
    for chain in &chains {
        // Lateral stand-off: outside the curb's outer footprint.
        let stand_off =
            chain.half_w + dims.curb_top_width + dims.chamfer_width + FURNITURE_CURB_CLEARANCE_M;
        let mut next_at = spacing * 0.5; // start mid-interval, clear of junctions
        let mut walked = 0.0_f32;
        let mut side = 1.0_f32;
        for w in chain.pts.windows(2) {
            let (dx, dz) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
            let seg = (dx * dx + dz * dz).sqrt();
            if seg < 1.0e-4 {
                continue;
            }
            let (ux, uz) = (dx / seg, dz / seg);
            // Right-hand lateral, matching the ribbon's frame convention.
            let (rx, rz) = (-uz, ux);
            while next_at <= walked + seg {
                let t = next_at - walked;
                let (cx, cz) = (w[0].0 + ux * t, w[0].1 + uz * t);
                let (px, pz) = (cx + rx * stand_off * side, cz + rz * stand_off * side);
                // Face the road: the prop's +Z front turns toward the deck.
                let (vx, vz) = (-rx * side, -rz * side);
                spots.push(FurnitureSpot {
                    position: [px + shift[0], pz + shift[1]],
                    yaw: vx.atan2(vz),
                });
                side = -side;
                next_at += spacing;
            }
            walked += seg;
        }
    }
    spots
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
    use crate::urban::test_support::*;

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

    #[test]
    fn produces_a_network_at_room_scale_for_the_pilot_seed() {
        // The pilot room at real scale + its derived road seed. Guards against
        // the windowed path yielding an empty network there.
        let parts = build_road_geometry(&pilot_heightmap(), &cfg(PILOT_ROAD_SEED))
            .expect("room-scale build for the pilot seed must produce roads");
        assert!(!parts.deck.is_empty());
    }

    #[test]
    fn furniture_spots_line_the_streets() {
        // #893: enabled furniture yields spots at the authored spacing,
        // room-framed, deterministic; disabled yields none.
        let hm = pilot_heightmap();
        let mut c = cfg(PILOT_ROAD_SEED);
        assert!(
            extract_furniture_spots(&hm, &c).is_empty(),
            "furniture is opt-in"
        );
        c.furniture.enabled = true;
        let spots = extract_furniture_spots(&hm, &c);
        assert!(!spots.is_empty(), "no furniture spots on the pilot network");
        let again = extract_furniture_spots(&hm, &c);
        assert_eq!(spots, again, "spots must be deterministic");
        let half = hm.world_width() * 0.5;
        for s in &spots {
            assert!(
                s.position[0].abs() <= half && s.position[1].abs() <= half,
                "spot {s:?} outside the room"
            );
            assert!(s.yaw.is_finite());
        }
        // Wider spacing → fewer props.
        c.furniture.spacing.0 = 120.0;
        let sparse = extract_furniture_spots(&hm, &c);
        assert!(sparse.len() < spots.len(), "spacing must thin the props");
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
}
