//! Tensor-field urban layout - deterministic, terrain-conforming road networks
//! for rooms whose author adds a `RoadNetwork` generator.
//!
//! `symbios-tensor` is used purely as a road-**topology** generator: we take
//! its tensor-field [`symbios_tensor::RoadGraph`] and build our own road geometry that *drapes*
//! over overlands' existing terrain (sampling the heightmap per vertex), rather
//! than the crate's carve-and-bridge path - which regrades the heightmap into
//! flat road shelves and shatters the natural relief of our ~1 km rooms.
//! Nothing is carved here; the terrain stays natural and the road conforms to
//! its surface.
//!
//! The road is built by extracting continuous **chains** (runs of connected
//! nodes between intersections) from the graph and extruding a closed
//! cross-section profile along each, with **miter joins** at the bends (so
//! curves have no gaps), sharp bends rounded into arcs first and any line of
//! the profile a bend still runs backwards collapsed, so no face folds over,
//! and continuous arc-length UVs (so a texture flows down the street). The profile is a chamfered curb framing a flat deck, over a
//! skirt that drops a fixed depth below the deck and is capped by a textured
//! bottom - so where the road runs out high over a dip the underside floats
//! clear as a bridge, not a hollow strip.
//!
//! Roads are editor-opt-in (#775): a seeded room grows none - the graph trace
//! plus its lot-building layer are too heavy for a good default room on wasm -
//! so everything here serves records that carry a `RoadNetwork` child (or that
//! were saved back when roads were still seeded). Generation is localized to
//! that network's district window (`district_half_extent`, 170 m by default,
//! centred on spawn until `center` moves it off the room origin) and clipped
//! to the district interior so no street runs off to the visible edge; a room
//! may run up to [`MAX_ROAD_NETWORKS`](crate::pds::room::MAX_ROAD_NETWORKS) of
//! them at once (#895). Everything is deterministic in the network's own
//! layout seed - a sub-stream kept separate from the terrain seed, so streets
//! can be re-rolled without disturbing the land - and recomputed at load,
//! never stored, like the heightmap itself.
//!
//! The field the streets follow is the land's own - contour lines for the
//! major streets, fall lines for the minor ones - unless the network carries
//! a street field ([`RoadConfig::field`], #1556): a smoothing scale that reads
//! the directions off a blurred copy of the relief, designer basis fields
//! summed with the land's (ring roads round a point, a grid turned to a
//! compass bearing), and keep-out discs no street enters and no lot grows a
//! building in. Its centres are authored in room metres; [`graph`] moves them
//! into the district window the trace runs in, by the inverse of the shift
//! the lots and the street furniture come back out by.
//!
//! What the street graph and the lots are derived with is the network's
//! layout revision ([`RoadConfig::layout_revision`], #1558): 0, every network
//! saved before it, is the original pipeline byte for byte; 1 tidies the
//! graph ([`graph`]) and keeps every lot clear of every street
//! ([`extract_building_lots`]); 2 derives all of that with portable maths
//! (#1563) - the `libm` crate's `sin`, `cos`, `tan`, `acos`, `atan2` and
//! `hypot` in symbios-tensor's trace, fillets, blocks and lots
//! ([`symbios_tensor::MathMode::Portable`]) and in this module's own graph
//! clean-up and street-prop yaws - where 0 and 1 take the platform's, which
//! a native client and the web one answer differently in the last bit, and
//! turn into different lots. The lots are saved, so a derivation change
//! bumps the revision. How the graph is meshed is not part of it: every
//! client meshes the plan afresh and nothing of the mesh is saved.
//!
//! `symbios-tensor` consumes a `symbios_ground::HeightMap`; overlands' own
//! [`bevy_symbios_ground::HeightMap`] is the same crate/type - both crates
//! resolve to the same `symbios-ground` version, so the heightmap passes
//! straight through with no conversion.
//!
//! ## Sub-module map
//!
//! * [`graph`] - tensor-field trace and rationalisation (the street field
//!   moved into the district window first), then the sanitation pass that
//!   welds coincident nodes and drops the degenerate edges whose unstable
//!   direction spikes a miter, and at layout revision 1 the tidy that merges
//!   junction clusters and drops doubled streets, loops and stubs.
//! * [`chains`] - the graph as drawn (cut to the district interior), and
//!   extraction of continuous runs of connected nodes between intersections.
//! * [`truncation`] - per-end pull-back, so a ribbon stops at the
//!   intersection boundary instead of overlapping into the hub, and which
//!   junctions a few metres apart are drawn as one hub.
//! * [`levelling`] - the single heightmap-sampling pass, and the
//!   network-wide resolve of flat hub heights and per-chain deck heights,
//!   so the pre-pass and the ribbon agree to the bit.
//! * [`bends`] - how a ribbon gets round a bend without folding: sharp
//!   bends rounded into arcs before sampling, and any line of the profile
//!   still running backwards collapsed onto the corner it crosses itself at.
//! * [`ribbon`] - cross-section extrusion along a levelled chain: miter
//!   frames, arc-length UVs, and the deck / curb / skirt / bottom strips.
//! * [`hubs`] - junction decks outlined by the streets' own curb lines,
//!   meeting every incident road at its exact levelled mouth.
//! * [`diagnostics`] - the `render --road-dump` topology and geometry-risk
//!   report (degree histogram, dead-end spurs, spike-risk bends, the hubs
//!   drawn).
//! * [`math`] - small vector helpers shared across the builders.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;
use symbios_tensor::{LotConfig, RoadGraph, extract_blocks, extract_lots};

use crate::pds::generator::RoadConfig;

mod bends;
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

use crate::urban::graph::{build_road_graph, window_to_room_shift};
use crate::urban::math::{cross, dot, sub3};

pub(crate) use crate::urban::chains::{Chain, active_degree, drawn_graph, extract_chains};
pub use crate::urban::diagnostics::{RoadDiagnostics, RoadGraphStats, road_graph_diagnostics};
pub(crate) use crate::urban::hubs::{RoadEnd, extrude_hubs};
pub(crate) use crate::urban::levelling::{
    ChainSample, junction_mouth_spreads, level_chain, level_network, sample_chain,
};
pub(crate) use crate::urban::ribbon::{
    RIBBON_STEP_M, UV_TILE_M, densify, extrude_ribbon, frame_right, quad_normal,
};
pub(crate) use crate::urban::truncation::{plan_junctions, trim_polyline};

// --- Tuning -----------------------------------------------------------------
//
// The authorable knobs (district extent, road spacing/widths, curb + skirt
// dimensions, layout seed) live on [`RoadConfig`] in the room record. The
// constants below are pure *rendering* details with no gameplay/aesthetic
// reason to vary per room, so they stay in code.

/// Lift (m) of the deck above the sampled terrain - keeps the deck clear of the
/// ground and the curb framing it proud.
pub(crate) const ROAD_DEPTH_BIAS_M: f32 = 0.2;
/// Below this squared cross product (m⁴, four times a triangle's squared
/// area) a road triangle encloses nothing and is not emitted.
const DEGENERATE_AREA2: f32 = 1.0e-12;
/// Diagonals of a road quad within this fraction of each other in squared
/// length are a tie, which [`quad_split`] settles the way every quad was
/// split before #1567.
const DIAGONAL_TIE: f32 = 1.0e-3;

/// Whether a road quad - `q` holding its left and right edge points at one
/// frame, then at the next (`a, b, c, d`) - is split along its `a`-`d`
/// diagonal rather than `b`-`c`: whichever is shorter, so a quad a grade
/// twists bends where it bends least, and a street draws the same whichever
/// way it turns (#1567). A tie keeps `a`-`d`, every quad's split before.
pub(crate) fn splits_along_ad(q: &[[f32; 3]; 4]) -> bool {
    let [a, b, c, d] = *q;
    let (ad, bc) = (sub3(d, a), sub3(c, b));
    dot(bc, bc) >= dot(ad, ad) * (1.0 - DIAGONAL_TIE)
}

/// The two triangles road quad `q` is drawn as (see [`splits_along_ad`]),
/// as indices from `base`, where its four corners were pushed in order -
/// every one wound like `a→b→d`.
fn quad_split(q: &[[f32; 3]; 4], base: u32) -> [[u32; 3]; 2] {
    let [a, b, c, d] = [base, base + 1, base + 2, base + 3];
    if splits_along_ad(q) {
        [[a, b, d], [a, d, c]]
    } else {
        [[a, b, c], [b, d, c]]
    }
}
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
/// accumulated up-facing triangle normals, except at each mouth, whose corners
/// share the ribbon's own mouth normal (see [`extrude_hubs`]).
#[derive(Default)]
pub struct RoadGeometry {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl RoadGeometry {
    /// True when no faces were emitted - the caller skips spawning a mesh.
    /// Counted in triangles: a surface can hold vertices whose every
    /// triangle had no area and was left out (#1567).
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// The triangles this surface draws (#1554).
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Append one quad (corners `a,b,c,d`, wound `a→b→d→c`) with a shared flat
    /// `nrm` and the four corner UVs, split as [`quad_split`] says; a half
    /// with no area is left out.
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
        for tri in quad_split(&[a, b, c, d], base) {
            self.push_triangle(tri);
        }
    }

    /// Append one quad strip for a single profile face: `left[i]`/`right[i]`
    /// are the face's two edges at frame `i` and `rows[i]` the normal both
    /// carry - the ribbon smooths them ALONG the strip, so it shades as one
    /// surface while staying a hard crease against the neighbouring face (a
    /// separate strip). `uv_u` is the lateral U of the two edges; `v[i]` the
    /// along-road V. A triangle with no area - where a fold was collapsed
    /// onto a point (#1567) - is left out.
    fn push_strip(
        &mut self,
        left: &[[f32; 3]],
        right: &[[f32; 3]],
        rows: &[[f32; 3]],
        uv_u: (f32, f32),
        v: &[f32],
    ) {
        let n = left.len();
        if n < 2 {
            return;
        }
        let base = self.vertices.len() as u32;
        for i in 0..n {
            self.vertices.push(left[i]);
            self.vertices.push(right[i]);
            self.normals.push(rows[i]);
            self.normals.push(rows[i]);
            self.uvs.push([uv_u.0, v[i]]);
            self.uvs.push([uv_u.1, v[i]]);
        }
        for i in 0..n - 1 {
            let q = [left[i], right[i], left[i + 1], right[i + 1]];
            for tri in quad_split(&q, base + (i as u32) * 2) {
                self.push_triangle(tri);
            }
        }
    }

    /// Append the triangle on three of this surface's vertices, unless its
    /// corners enclose no area.
    fn push_triangle(&mut self, tri: [u32; 3]) {
        let [a, b, c] = tri.map(|i| self.vertices[i as usize]);
        let area = cross(sub3(b, a), sub3(c, a));
        if dot(area, area) > DEGENERATE_AREA2 {
            self.indices.extend_from_slice(&tri);
        }
    }
}

/// The road split into its material surfaces, so the caller can give each the
/// look it needs - a dark wet-asphalt **deck**, a concrete/metal **structure**
/// (curb + skirt + bottom cap) and emissive neon **edge-lines** - without
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
    /// Street (chain) count of the built network - the chains drawn as
    /// ribbons - for the editor's stats readout (#888), not the geometry.
    pub chains: usize,
    /// Junction count: the hubs three or more streets meet at (#1558) - see
    /// [`Self::chains`].
    pub junctions: usize,
}

impl RoadParts {
    /// Total vertex count across the three surfaces - the editor's
    /// mesh-weight readout (#888).
    pub fn vertex_count(&self) -> usize {
        self.deck.vertices.len() + self.structure.vertices.len() + self.neon.vertices.len()
    }

    /// What the network costs to draw (#1554): its triangles, and its parts -
    /// one mesh entity per surface that emitted faces, as
    /// `terrain::roads::spawn_road_meshes` spawns them.
    pub fn draw_cost(&self) -> (usize, usize) {
        let surfaces = [&self.deck, &self.structure, &self.neon];
        (
            surfaces.iter().map(|g| g.triangle_count()).sum(),
            surfaces.iter().filter(|g| !g.is_empty()).count(),
        )
    }
}

/// Build terrain-conforming road geometry from a [`RoadConfig`], or `None` if
/// the config is disabled or the tracer can't produce a network. Deterministic
/// in `config.seed` (and, for a network that avoids water, `water_level`: the
/// room's water line, `None` for a dry room - see [`build_road_graph`]). Does
/// **not** modify `hm` - the road drapes over the natural terrain. Which rooms
/// *get* a road config is the seeding layer's policy ([`crate::pds::room`]);
/// this just renders whatever it's handed.
pub fn build_road_geometry(
    hm: &HeightMap,
    config: &RoadConfig,
    water_level: Option<f32>,
) -> Option<RoadParts> {
    let (graph, sub, lo) = build_road_graph(hm, config, water_level)?;
    let parts = mesh_road_graph(&graph, &sub, lo, &Dims::from_config(config));
    (!parts.deck.is_empty() || !parts.structure.is_empty()).then_some(parts)
}

/// Mesh a traced road graph over its district window `sub` (lower cell `lo`
/// in the full heightmap): the drawn graph's chains, truncated and grouped
/// into hubs by the junction plan, levelled network-wide, extruded, and
/// closed by their hubs. Split out of [`build_road_geometry`] so a test can
/// mesh a hand-built graph.
pub(crate) fn mesh_road_graph(
    graph: &symbios_tensor::RoadGraph,
    sub: &HeightMap,
    lo: [usize; 2],
    dims: &Dims,
) -> RoadParts {
    // Only the streets the player sees count (#1558): an arm the district
    // clip drops no longer makes its node a junction.
    let drawn = drawn_graph(graph, sub);
    let chains = extract_chains(&drawn, sub, dims);
    let degree = active_degree(&drawn);

    // Pull-back per chain end abutting a hub, so each ribbon stops at the
    // intersection boundary instead of overlapping into it (#575), and which
    // junctions are drawn as one hub (#1558). Computed once, ahead of
    // extrusion.
    let plan = plan_junctions(&chains, &degree, dims);

    let mut parts = RoadParts::default();
    let world_offset = [lo[0] as f32 * sub.scale(), lo[1] as f32 * sub.scale()];

    // Sample each chain's terrain ONCE (the only heightmap-sampling site), then
    // resolve flat hub heights + the per-chain deck heights network-wide
    // (#584). The mesh pass consumes the cached sample + resolved heights, so the
    // pre-pass and the ribbon agree to the bit (no floor-drift seam at the mouths).
    // A chain swallowed by its hub grows no ribbon.
    let samples: Vec<Option<ChainSample>> = chains
        .iter()
        .enumerate()
        .map(|(ci, chain)| {
            let [s, e] = plan.trims[ci];
            (!plan.internal[ci])
                .then(|| sample_chain(chain, s, e, sub, dims))
                .flatten()
        })
        .collect();
    let ground = hub_grounds(&chains, &samples, &plan, dims);
    let base_ys = level_network(&chains, &samples, &plan, &ground, sub);

    // Each chain extrudes its ribbon and records its end-frames at hubs, so
    // the hubs can be built to meet every incident road at its exact (levelled) mouth.
    let mut road_ends: Vec<RoadEnd> = Vec::new();
    for (ci, chain) in chains.iter().enumerate() {
        if let Some(sample) = &samples[ci] {
            extrude_ribbon(
                chain,
                sample,
                &base_ys[ci],
                world_offset,
                dims,
                plan.chain_ends(ci),
                &mut road_ends,
                &mut parts,
            );
        }
    }
    extrude_hubs(&road_ends, &plan.hubs, world_offset, dims, &mut parts);
    // Editor stats (#888): streets = the chains drawn as ribbons, junctions =
    // the hubs three or more of them meet at.
    parts.chains = plan.internal.iter().filter(|&&i| !i).count();
    parts.junctions = plan.hubs.iter().filter(|h| h.arms.len() >= 3).count();
    parts
}

/// Every hub's deck outline in the district window (#1558), read from the
/// sampled mouths before any deck height exists: the ground its flat deck
/// must clear, so no corner of a hub buries.
pub(crate) fn hub_grounds(
    chains: &[Chain],
    samples: &[Option<ChainSample>],
    plan: &truncation::JunctionPlan,
    dims: &Dims,
) -> Vec<Vec<[f32; 2]>> {
    let mut mouths: Vec<Vec<RoadEnd>> = (0..plan.hubs.len()).map(|_| Vec::new()).collect();
    for (ci, chain) in chains.iter().enumerate() {
        if let Some(sample) = &samples[ci] {
            for m in ribbon::sample_mouths(chain, sample, plan.chain_ends(ci)) {
                mouths[m.hub].push(m);
            }
        }
    }
    mouths
        .iter()
        .zip(&plan.hubs)
        .map(|(ends, hub)| {
            let ends: Vec<&RoadEnd> = ends.iter().collect();
            if ends.len() < 2 {
                return Vec::new();
            }
            hubs::hub_outline(&ends, hub, dims)
        })
        .collect()
}

/// A building footprint extracted from the road network's enclosed city blocks,
/// in the **room placement frame** - XZ centred on spawn, matching the road
/// mesh's `-half` spawn offset - so each maps straight onto a
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
/// A lot whose centre lies inside one of the network's keep-out discs
/// (#1556) is left out: the tracer keeps every street out of a disc, but the
/// block of streets round it still encloses it, and its lots would stand a
/// building on the plaza the disc was drawn to keep clear.
///
/// At layout revision 1 (#1558) every lot is pushed clear of every street
/// ([`clear_lots`]): symbios-tensor cuts lots from the street CENTRELINES and
/// sets them back 3 m at the front and 1.5 m at the sides, while a street's
/// curb reaches up to 4.12 m out, so a revision-0 lot's front and a corner
/// lot's side stand on the curb.
///
/// This is the seed for the lot-based building layer ([`crate::terrain`]'s
/// load-time populate-lots system): it shares [`build_road_graph`] with the
/// road mesh, so every footprint sits on a street the player can see.
pub fn extract_building_lots(
    hm: &HeightMap,
    config: &RoadConfig,
    water_level: Option<f32>,
) -> Vec<BuildingLot> {
    let mut lots = traced_lots(hm, config, water_level);
    // Room frame on both sides: the lots were shifted out of the window
    // and the discs are authored in room metres.
    lots.retain(|lot| {
        !config
            .field
            .keep_out
            .iter()
            .any(|disc| disc.contains(lot.position))
    });
    lots
}

/// Every lot the network's blocks subdivide into, in the room placement
/// frame: [`extract_building_lots`] before it drops the lots in a keep-out
/// disc.
fn traced_lots(hm: &HeightMap, config: &RoadConfig, water_level: Option<f32>) -> Vec<BuildingLot> {
    let Some((mut graph, mut sub, lo)) = build_road_graph(hm, config, water_level) else {
        return Vec::new();
    };
    // Enclosed faces → blocks → recursively subdivided, street-aligned lots.
    extract_blocks(&mut graph);
    let mut lots = extract_lots(&graph, &mut sub, &lot_config(config, water_level));
    // Layout revision 1 (#1558): every lot clears every street.
    if config.tidies_layout() {
        lots = clear_lots(
            lots,
            &street_footprints(&graph, config, LOT_STREET_MARGIN_M),
        );
    }

    // Sub-window XZ (origin at the window's lower corner) → room-centred frame:
    // the road mesh draws window coord `p` at world `p + lo*scale - half`, so a
    // footprint placed there lands exactly on its street. Per-axis since the
    // district centre offset (#889) can shift the window asymmetrically.
    let shift = window_to_room_shift(hm, lo);
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

/// The lot subdivision settings for `config`. A network that stops at the
/// shore grows no lot that touches the water either (#1552): the default
/// `WaterPolicy::Skip` drops a lot whose centre or a corner stands at or
/// below the line, where the placement's own water walk would slide the
/// building along its bearing and could stand it on a street. Its streets
/// already keep the lots off the lake; this catches a flooded hollow inside
/// a block whose streets all stand dry.
///
/// Blocks split down to the network's own largest lot area (#1555), which
/// defaults to symbios-tensor's 400 m2, so a network that never set it
/// subdivides exactly as before.
fn lot_config(config: &RoadConfig, water_level: Option<f32>) -> LotConfig {
    let mut lots = LotConfig {
        max_lot_area: config.lots.lot_area.0,
        ..LotConfig::default()
    };
    if config.avoid_water
        && let Some(level) = water_level
    {
        lots.water_level = level;
    }
    lots
}

/// Clearance (m) every lot of a tidied layout (revision 1, #1558) keeps from
/// every street's outer footprint - deck, curb and chamfer: a sidewalk, on
/// which the street furniture stands.
pub(crate) const LOT_STREET_MARGIN_M: f32 = 2.0;
/// The narrowest a cleared lot may become (m) before it is dropped -
/// symbios-tensor's own least lot width and depth.
const LOT_MIN_SIDE_M: f32 = 6.0;
/// Bisection steps when a lot's side is pushed back off a street: the push
/// lands within a millionth of the lot's side.
const LOT_PUSH_STEPS: usize = 20;

/// A street's footprint as the lots must clear it: the capsule within
/// `radius` of the segment `a`-`b` (a disc where they coincide), window
/// frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Footprint {
    pub(crate) a: [f32; 2],
    pub(crate) b: [f32; 2],
    pub(crate) radius: f32,
}

/// Every street of `graph` as the lots must clear it (#1558): each active
/// edge's centreline grown by its road class's outer half-width (deck, curb
/// and chamfer) plus `margin`, and every bend between two edges by the mitre
/// a ribbon took there before it rounded its sharp bends (#1567): its corner
/// reaches `1/cos(turn/2)` times as far, clamped at three. The rounded ribbon
/// keeps inside that footprint. Derived from the graph and the network's
/// dimensions alone - not from the mesher, which may change - so the lots of
/// one layout revision stay the same lots.
pub(crate) fn street_footprints(
    graph: &RoadGraph,
    config: &RoadConfig,
    margin: f32,
) -> Vec<Footprint> {
    let outer = config.curb_top_width.0 + config.chamfer_width.0;
    let wo = |t: &symbios_tensor::RoadType| match t {
        symbios_tensor::RoadType::Major => config.major_half_width.0 + outer,
        symbios_tensor::RoadType::Minor => config.minor_half_width.0 + outer,
    };
    let pos = |i: u32| {
        let p = graph.nodes[i as usize].position;
        [p.x, p.y]
    };
    let mut out = Vec::new();
    let mut at: Vec<Vec<(usize, [f32; 2])>> = vec![Vec::new(); graph.nodes.len()];
    for (ei, e) in graph.edges.iter().enumerate() {
        if !e.active {
            continue;
        }
        let (a, b) = (pos(e.start), pos(e.end));
        out.push(Footprint {
            a,
            b,
            radius: wo(&e.road_type) + margin,
        });
        at[e.start as usize].push((ei, b));
        at[e.end as usize].push((ei, a));
    }
    for (n, spokes) in at.iter().enumerate() {
        if spokes.len() != 2 {
            continue;
        }
        let p = pos(n as u32);
        let unit = |q: [f32; 2]| {
            let d = [q[0] - p[0], q[1] - p[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1.0e-6);
            [d[0] / l, d[1] / l]
        };
        let (u, v) = (unit(spokes[0].1), unit(spokes[1].1));
        // Half the angle between the two arms; a straight run is half a turn.
        let cos_half = ((1.0 - (u[0] * v[0] + u[1] * v[1])) * 0.5).max(0.0).sqrt();
        let mitre = (1.0 / cos_half.max(1.0e-6)).min(3.0);
        if mitre > 1.0 + 1.0e-4 {
            let w = wo(&graph.edges[spokes[0].0].road_type)
                .max(wo(&graph.edges[spokes[1].0].road_type));
            out.push(Footprint {
                a: p,
                b: p,
                radius: w * mitre + margin,
            });
        }
    }
    out
}

/// The distance from a segment to an axis-aligned rectangle
/// `[u0, u1] × [v0, v1]`, both in the rectangle's own frame: 0 when they
/// touch.
fn segment_rect_distance(p: [f32; 2], q: [f32; 2], r: [f32; 4]) -> f32 {
    let [u0, u1, v0, v1] = r;
    // Liang-Barsky: does the segment enter the rectangle?
    let d = [q[0] - p[0], q[1] - p[1]];
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    let mut enters = true;
    for (den, num) in [
        (-d[0], p[0] - u0),
        (d[0], u1 - p[0]),
        (-d[1], p[1] - v0),
        (d[1], v1 - p[1]),
    ] {
        if den.abs() < 1.0e-12 {
            if num < 0.0 {
                enters = false;
                break;
            }
        } else {
            let t = num / den;
            if den < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if enters && t0 <= t1 {
        return 0.0;
    }
    let to_rect = |x: [f32; 2]| {
        let dx = (u0 - x[0]).max(0.0).max(x[0] - u1);
        let dz = (v0 - x[1]).max(0.0).max(x[1] - v1);
        (dx * dx + dz * dz).sqrt()
    };
    let to_seg = |x: [f32; 2]| {
        let l2 = d[0] * d[0] + d[1] * d[1];
        let t = if l2 < 1.0e-12 {
            0.0
        } else {
            (((x[0] - p[0]) * d[0] + (x[1] - p[1]) * d[1]) / l2).clamp(0.0, 1.0)
        };
        let (dx, dz) = (x[0] - p[0] - d[0] * t, x[1] - p[1] - d[1] * t);
        (dx * dx + dz * dz).sqrt()
    };
    [
        to_rect(p),
        to_rect(q),
        to_seg([u0, v0]),
        to_seg([u0, v1]),
        to_seg([u1, v0]),
        to_seg([u1, v1]),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min)
}

/// Push every lot's sides back off the streets (#1558): while a footprint
/// still reaches into a lot, the side whose push frees it of that footprint
/// for the least area moves back - each push the least that frees it - so a
/// side facing a street clears it and a side facing a neighbouring lot keeps
/// the setback the subdivision gave it. A lot left narrower than
/// [`LOT_MIN_SIDE_M`] either way is dropped. Rotation is kept; the centre
/// moves with the sides.
///
/// What is and is not the same on every peer: this step, and
/// [`street_footprints`] before it, are basic IEEE arithmetic (square roots,
/// no platform transcendental) and the `libm` crate's sine and cosine, so
/// given the same traced lots and graph every peer clears them to the same
/// bits. The lots and graph it is given are the same on every peer at
/// layout revision 2 (#1563). At revisions 0 and 1 they are not: the
/// tracer's `rationalize` (`acosf`, `tanf`), `extract_blocks` (`atan2f`) and
/// `extract_lots` (`cosf`, `sinf`, `atan2f`) go through the platform's libm,
/// whose last bits differ between a native glibc and the wasm build - so a
/// lot can differ in its last bits between peers, and where a lot's side
/// lands on a clearance or least-width threshold it can be kept on one and
/// dropped on another. Once saved, the record's buildings are what every
/// peer shows.
fn clear_lots(
    lots: Vec<symbios_tensor::BuildingLot>,
    streets: &[Footprint],
) -> Vec<symbios_tensor::BuildingLot> {
    lots.into_iter()
        .filter_map(|lot| clear_lot(lot, streets))
        .collect()
}

/// One lot of [`clear_lots`], or `None` when it is dropped.
fn clear_lot(
    mut lot: symbios_tensor::BuildingLot,
    streets: &[Footprint],
) -> Option<symbios_tensor::BuildingLot> {
    let (sin, cos) = (libm::sinf(lot.rotation), libm::cosf(lot.rotation));
    let (u, v) = ([cos, sin], [-sin, cos]);
    let c = [lot.position.x, lot.position.y];
    let local = |p: [f32; 2]| {
        let d = [p[0] - c[0], p[1] - c[1]];
        [d[0] * u[0] + d[1] * u[1], d[0] * v[0] + d[1] * v[1]]
    };
    let reach = (lot.width * lot.width + lot.depth * lot.depth).sqrt() * 0.5;
    // The footprints that can reach the lot at all, in its frame.
    let near: Vec<([f32; 2], [f32; 2], f32)> = streets
        .iter()
        .filter_map(|f| {
            let (a, b) = (local(f.a), local(f.b));
            let d = segment_rect_distance(a, b, [-reach, reach, -reach, reach]);
            (d < f.radius).then_some((a, b, f.radius))
        })
        .collect();
    let mut r = [
        -lot.width * 0.5,
        lot.width * 0.5,
        -lot.depth * 0.5,
        lot.depth * 0.5,
    ];
    for _ in 0..4 * near.len().max(1) {
        // The deepest footprint still in the lot (the first, on a tie).
        let Some((a, b, radius)) = near
            .iter()
            .map(|&(a, b, rad)| (segment_rect_distance(a, b, r) - rad, (a, b, rad)))
            .filter(|(gap, _)| *gap < 0.0)
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, f)| f)
        else {
            break;
        };
        // Each side's least push that frees the lot of it, and what it costs.
        let mut best: Option<(f32, usize, f32)> = None; // (area lost, side, push)
        for side in 0..4 {
            let along = if side < 2 { r[1] - r[0] } else { r[3] - r[2] };
            let across = if side < 2 { r[3] - r[2] } else { r[1] - r[0] };
            let room = along - LOT_MIN_SIDE_M;
            if room <= 0.0 {
                continue;
            }
            let pushed = |p: f32| {
                let mut q = r;
                match side {
                    0 => q[0] += p,
                    1 => q[1] -= p,
                    2 => q[2] += p,
                    _ => q[3] -= p,
                }
                q
            };
            if segment_rect_distance(a, b, pushed(room)) < radius {
                continue; // even pushed to the least lot, it still reaches in
            }
            let (mut lo, mut hi) = (0.0_f32, room);
            for _ in 0..LOT_PUSH_STEPS {
                let mid = (lo + hi) * 0.5;
                if segment_rect_distance(a, b, pushed(mid)) < radius {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let cost = hi * across;
            if best.is_none_or(|(c0, _, _)| cost < c0) {
                best = Some((cost, side, hi));
            }
        }
        let (_, side, push) = best?;
        match side {
            0 => r[0] += push,
            1 => r[1] -= push,
            2 => r[2] += push,
            _ => r[3] -= push,
        }
    }
    if near
        .iter()
        .any(|&(a, b, rad)| segment_rect_distance(a, b, r) < rad - 1.0e-3)
    {
        return None; // still on a street after every push
    }
    let (w, d) = (r[1] - r[0], r[3] - r[2]);
    if w < LOT_MIN_SIDE_M || d < LOT_MIN_SIDE_M {
        return None;
    }
    let (mu, mv) = ((r[0] + r[1]) * 0.5, (r[2] + r[3]) * 0.5);
    lot.position.x = c[0] + u[0] * mu + v[0] * mv;
    lot.position.y = c[1] + u[1] * mu + v[1] * mv;
    lot.width = w;
    lot.depth = d;
    Some(lot)
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
/// footprint. Deterministic in the config (pure geometry - no RNG here; the
/// injector's seeded stream picks *which* prop stands at each spot). At
/// layout revision 1 (#1558) a spot that falls on another street - beside a
/// junction - is left out.
pub fn extract_furniture_spots(
    hm: &HeightMap,
    config: &RoadConfig,
    water_level: Option<f32>,
) -> Vec<FurnitureSpot> {
    if !config.furniture.enabled {
        return Vec::new();
    }
    let Some((graph, sub, lo)) = build_road_graph(hm, config, water_level) else {
        return Vec::new();
    };
    let dims = Dims::from_config(config);
    let chains = extract_chains(&graph, &sub, &dims);
    let spacing = config.furniture.spacing.0.max(1.0);
    // A prop's yaw is saved with it: portable at layout revision 2 (#1563).
    let math = graph.math;

    let shift = window_to_room_shift(hm, lo);
    // Layout revision 1 (#1558): no prop stands on a street - near a
    // junction a spot beside one street can fall on the next.
    let streets = config
        .tidies_layout()
        .then(|| street_footprints(&graph, config, 0.0));
    let clear = |p: [f32; 2]| {
        streets.as_ref().is_none_or(|streets| {
            streets.iter().all(|f| {
                segment_rect_distance(f.a, f.b, [p[0], p[0], p[1], p[1]])
                    >= f.radius + FURNITURE_CURB_CLEARANCE_M - 0.05
            })
        })
    };

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
                if clear([px, pz]) {
                    spots.push(FurnitureSpot {
                        position: [px + shift[0], pz + shift[1]],
                        yaw: math.atan2(vx, vz),
                    });
                }
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
        let parts = build_road_geometry(&sloped_heightmap(), &cfg(7), None)
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
        let parts = build_road_geometry(&pilot_heightmap(), &cfg(PILOT_ROAD_SEED), None)
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
            extract_furniture_spots(&hm, &c, None).is_empty(),
            "furniture is opt-in"
        );
        c.furniture.enabled = true;
        let spots = extract_furniture_spots(&hm, &c, None);
        assert!(!spots.is_empty(), "no furniture spots on the pilot network");
        let again = extract_furniture_spots(&hm, &c, None);
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
        let sparse = extract_furniture_spots(&hm, &c, None);
        assert!(sparse.len() < spots.len(), "spacing must thin the props");
    }

    #[test]
    fn extracts_building_lots_at_room_scale() {
        // The lot layer's load-bearing guard: the pilot network must enclose
        // blocks that subdivide into real footprints, all inside the district
        // window (room-centred) with positive, finite extents.
        let hm = pilot_heightmap();
        let lots = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED), None);
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
        let a = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED), None);
        let b = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED), None);
        assert_eq!(a, b, "building lots non-deterministic for identical input");
    }

    /// #1552: a network that stops at the shore grows no lot whose centre
    /// stands in the water. The control is the same network with the switch
    /// off over the same lake, which does grow lots on the lake bed. Here the
    /// street trace alone keeps the lots dry (a block bounded by dry streets
    /// has dry corners); the lot rule has its own test below.
    #[test]
    fn a_network_that_avoids_water_grows_no_lot_in_it() {
        let hm = pilot_heightmap();
        let mut heights: Vec<f32> = hm.data().to_vec();
        heights.sort_by(f32::total_cmp);
        let level = heights[heights.len() / 3];
        // Room-centred placement frame -> heightmap coordinates, as the
        // extraction shifts them.
        let half = hm.width().saturating_sub(1) as f32 * hm.scale() * 0.5;
        let drowned = |lots: &[BuildingLot]| {
            lots.iter()
                .filter(|l| hm.get_height_at(l.position[0] + half, l.position[1] + half) <= level)
                .count()
        };

        let shore = RoadConfig {
            avoid_water: true,
            ..cfg(PILOT_ROAD_SEED)
        };
        let lots = extract_building_lots(&hm, &shore, Some(level));
        assert!(!lots.is_empty(), "the shore network still grows lots");
        assert_eq!(drowned(&lots), 0, "a lot stands in the water");

        let plain = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED), Some(level));
        assert!(
            drowned(&plain) > 0,
            "the control: with the switch off, lots grow on the lake bed"
        );
    }

    /// #1552: the lot subdivision is handed the water line exactly when
    /// the network avoids water. The street trace already keeps lots off
    /// the lake (the test above passes without this), so this is the rule's
    /// own test: a flooded hollow inside a dry block is caught only here.
    #[test]
    fn only_a_network_that_avoids_water_hands_the_line_to_its_lots() {
        let shore = RoadConfig {
            avoid_water: true,
            ..RoadConfig::default()
        };
        assert_eq!(lot_config(&shore, Some(7.0)).water_level, 7.0);
        assert_eq!(
            lot_config(&shore, None).water_level,
            f32::NEG_INFINITY,
            "a dry room has no line"
        );
        assert_eq!(
            lot_config(&RoadConfig::default(), Some(7.0)).water_level,
            f32::NEG_INFINITY,
            "a network that ignores the water lots as it always did"
        );
    }

    /// #1555: blocks split down to the network's own lot area, and a
    /// network that never set it splits by symbios-tensor's default, as
    /// every network did before the field. Bigger lots grow fewer of them.
    #[test]
    fn the_lot_area_is_the_networks_own_and_defaults_to_the_old_split() {
        assert_eq!(
            lot_config(&RoadConfig::default(), None).max_lot_area,
            LotConfig::default().max_lot_area,
            "the default split is the one every saved network was grown by"
        );
        let mut downtown = cfg(PILOT_ROAD_SEED);
        downtown.lots.lot_area.0 = 2400.0;
        assert_eq!(lot_config(&downtown, None).max_lot_area, 2400.0);
        let hm = pilot_heightmap();
        let small = extract_building_lots(&hm, &cfg(PILOT_ROAD_SEED), None);
        let big = extract_building_lots(&hm, &downtown, None);
        assert!(
            !big.is_empty() && big.len() * 2 < small.len(),
            "a bigger lot area grows fewer, larger lots: {} vs {}",
            big.len(),
            small.len()
        );
    }

    /// #1556: a keep-out disc keeps the lots out too. The tracer keeps the
    /// streets out of it, but the block of streets round the disc still
    /// encloses it, and that block's lots would stand buildings on the
    /// plaza the disc was drawn to keep clear. The disc is given in room
    /// metres in a district moved off the room origin, and the lots are
    /// read in the room frame they are placed in. The control is the same
    /// trace before the lot rule, which does grow lots in the disc.
    #[test]
    fn a_keep_out_disc_keeps_the_lots_out_of_its_room_disc() {
        use crate::pds::generator::{RoadField, RoadKeepOut};
        use crate::pds::types::{Fp, Fp2};
        let hm = pilot_heightmap();
        let (centre, radius) = ([90.0_f32, -10.0_f32], 50.0_f32);
        let config = RoadConfig {
            center: Fp2([60.0, -40.0]),
            field: RoadField {
                keep_out: vec![RoadKeepOut {
                    center: Fp2(centre),
                    radius: Fp(radius),
                }],
                ..RoadField::default()
            },
            ..cfg(PILOT_ROAD_SEED)
        };
        let in_disc = |lots: &[BuildingLot]| {
            lots.iter()
                .filter(|l| (l.position[0] - centre[0]).hypot(l.position[1] - centre[1]) < radius)
                .count()
        };
        let traced = traced_lots(&hm, &config, None);
        let lots = extract_building_lots(&hm, &config, None);
        assert!(
            in_disc(&traced) > 0,
            "the control: the block round the disc grows lots in it"
        );
        assert_eq!(in_disc(&lots), 0, "a lot grows in the keep-out disc");
        assert_eq!(
            lots.len() + in_disc(&traced),
            traced.len(),
            "only the lots in the disc are dropped"
        );
    }

    /// A tensor lot centred on `(x, z)`, `width` along X (rotation 0) and
    /// `depth` along Z.
    fn tensor_lot(x: f32, z: f32, width: f32, depth: f32) -> symbios_tensor::BuildingLot {
        symbios_tensor::BuildingLot {
            position: glam::Vec2::new(x, z),
            frontage_center: glam::Vec2::new(x, z),
            rotation: 0.0,
            width,
            depth,
            is_shoreline: false,
        }
    }

    /// #1558: a corner lot pushed off the major street it fronts and the
    /// minor street down its side clears both by the sidewalk margin past
    /// their full footprints (deck, curb and chamfer), while its two sides
    /// facing neighbouring lots keep the setbacks the subdivision gave them.
    /// The lot reaches 3 m from the major's centreline - the old front
    /// setback - and 2 m short of the minor's.
    #[test]
    fn a_corner_lot_clears_both_streets_and_keeps_its_inner_setbacks() {
        let c = RoadConfig::default();
        let graph = typed_graph(
            &[(-60.0, 0.0), (60.0, 0.0), (20.0, -60.0), (20.0, 60.0)],
            &[(0, 1, true), (2, 3, false)],
        );
        let streets = street_footprints(&graph, &c, LOT_STREET_MARGIN_M);
        let outer = c.curb_top_width.0 + c.chamfer_width.0;
        let (major, minor) = (c.major_half_width.0 + outer, c.minor_half_width.0 + outer);
        // x in [-5, 18], z in [3, 17].
        let lot = clear_lot(tensor_lot(6.5, 10.0, 23.0, 14.0), &streets).expect("the lot survives");
        let (x0, x1) = (
            lot.position.x - lot.width * 0.5,
            lot.position.x + lot.width * 0.5,
        );
        let (z0, z1) = (
            lot.position.y - lot.depth * 0.5,
            lot.position.y + lot.depth * 0.5,
        );
        assert!(
            z0 >= major + LOT_STREET_MARGIN_M - 1.0e-3,
            "the front {z0} is within {} of the major street",
            major + LOT_STREET_MARGIN_M
        );
        assert!(
            x1 <= 20.0 - minor - LOT_STREET_MARGIN_M + 1.0e-3,
            "the side {x1} is within {} of the minor street",
            minor + LOT_STREET_MARGIN_M
        );
        assert!((x0 + 5.0).abs() < 1.0e-3, "the inner side moved: {x0}");
        assert!((z1 - 17.0).abs() < 1.0e-3, "the rear moved: {z1}");
        assert_eq!(lot.rotation, 0.0, "the lot keeps its street alignment");
        // A lot the streets leave no room in goes.
        assert!(
            clear_lot(tensor_lot(16.0, 4.0, 6.0, 6.0), &streets).is_none(),
            "a lot under both streets is dropped"
        );
    }

    /// Whether an XZ triangle comes within `gap` of a lot's footprint.
    fn near_lot(lot: &BuildingLot, tri: [[f32; 3]; 3], gap: f32) -> bool {
        // The lot's frame: X along its frontage, Z across it.
        let yaw = -lot.yaw;
        let (u, v) = ([yaw.cos(), yaw.sin()], [-yaw.sin(), yaw.cos()]);
        let local = |p: [f32; 3]| {
            let d = [p[0] - lot.position[0], p[2] - lot.position[1]];
            [d[0] * u[0] + d[1] * u[1], d[0] * v[0] + d[1] * v[1]]
        };
        let t = tri.map(local);
        let r = [
            -lot.width * 0.5,
            lot.width * 0.5,
            -lot.depth * 0.5,
            lot.depth * 0.5,
        ];
        // A lot corner inside the triangle, or any side within the gap.
        let inside = |p: [f32; 2]| {
            let s = |a: [f32; 2], b: [f32; 2]| {
                (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
            };
            let (d1, d2, d3) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
            (d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0) || (d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0)
        };
        [[r[0], r[2]], [r[0], r[3]], [r[1], r[2]], [r[1], r[3]]]
            .into_iter()
            .any(inside)
            || (0..3).any(|k| segment_rect_distance(t[k], t[(k + 1) % 3], r) < gap)
    }

    /// #1558: at layout revision 1 no lot of the pilot network comes within
    /// the sidewalk margin of anything the road mesh draws - deck, curb,
    /// chamfer, junction - checked against the mesh's own triangles, not the
    /// footprints the lots were cleared by. The control is the original
    /// plan, whose lots reach into the streets.
    #[test]
    fn at_layout_revision_1_no_lot_touches_a_street() {
        let hm = pilot_heightmap();
        let half = hm.width().saturating_sub(1) as f32 * hm.scale() * 0.5;
        let touching = |config: &RoadConfig, gap: f32| {
            let parts = build_road_geometry(&hm, config, None).expect("the pilot meshes");
            let lots = extract_building_lots(&hm, config, None);
            let tris: Vec<[[f32; 3]; 3]> = [&parts.deck, &parts.structure]
                .iter()
                .flat_map(|g| {
                    g.indices.as_chunks::<3>().0.iter().map(|t| {
                        t.map(|i| {
                            let v = g.vertices[i as usize];
                            [v[0] - half, v[1], v[2] - half] // room frame, as the lots
                        })
                    })
                })
                .collect();
            let hit = lots
                .iter()
                .filter(|lot| {
                    let reach = lot.width.hypot(lot.depth) * 0.5 + gap + 1.0;
                    tris.iter().any(|t| {
                        t.iter().any(|p| {
                            (p[0] - lot.position[0]).abs() < reach + 30.0
                                && (p[2] - lot.position[1]).abs() < reach + 30.0
                        }) && near_lot(lot, *t, gap)
                    })
                })
                .count();
            (hit, lots.len())
        };
        let (original_hits, original_lots) = touching(&cfg(PILOT_ROAD_SEED), 0.0);
        assert!(
            original_hits > 0,
            "the control: the original plan's lots reach into the streets ({original_hits} of {original_lots})"
        );
        let mut config = cfg(PILOT_ROAD_SEED);
        config.layout_revision = 1;
        let (hits, lots) = touching(&config, LOT_STREET_MARGIN_M - 0.05);
        assert!(lots > 0, "the tidied pilot still grows lots");
        assert_eq!(
            hits, 0,
            "{hits} of {lots} lots come within the margin of a street"
        );
    }

    /// #1558: at layout revision 1 no street-furniture spot stands on a
    /// street: each keeps its clearance from every street's footprint, the
    /// next street's near a junction too. The original plan's do not.
    #[test]
    fn at_layout_revision_1_no_street_prop_stands_on_a_street() {
        let hm = pilot_heightmap();
        let on_street = |config: &RoadConfig| {
            let (graph, _sub, lo) = build_road_graph(&hm, config, None).expect("traces");
            let shift = window_to_room_shift(&hm, lo);
            let streets = street_footprints(&graph, config, 0.0);
            extract_furniture_spots(&hm, config, None)
                .iter()
                .filter(|s| {
                    let p = [s.position[0] - shift[0], s.position[1] - shift[1]];
                    streets.iter().any(|f| {
                        segment_rect_distance(f.a, f.b, [p[0], p[0], p[1], p[1]]) < f.radius
                    })
                })
                .count()
        };
        let mut config = cfg(PILOT_ROAD_SEED);
        config.furniture.enabled = true;
        assert!(
            on_street(&config) > 0,
            "the control: the original plan's props stand on streets"
        );
        config.layout_revision = 1;
        assert!(!extract_furniture_spots(&hm, &config, None).is_empty());
        assert_eq!(on_street(&config), 0, "a prop stands on a street");
    }

    #[test]
    fn disabled_network_extracts_no_lots() {
        let c = RoadConfig {
            enabled: false,
            ..cfg(PILOT_ROAD_SEED)
        };
        assert!(extract_building_lots(&pilot_heightmap(), &c, None).is_empty());
    }

    #[test]
    fn disabled_config_grows_no_roads() {
        let c = RoadConfig {
            enabled: false,
            ..cfg(7)
        };
        assert!(build_road_geometry(&sloped_heightmap(), &c, None).is_none());
    }

    /// The record-build ↔ client-render contract rests on the layout being
    /// deterministic from the seed: identical input must yield identical road
    /// geometry, vertex-for-vertex.
    #[test]
    fn road_geometry_is_deterministic() {
        let a = sloped_heightmap();
        let b = sloped_heightmap();
        match (
            build_road_geometry(&a, &cfg(7), None),
            build_road_geometry(&b, &cfg(7), None),
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

    /// Draping must not touch the terrain - the heightmap is rendered as-is.
    #[test]
    fn draping_leaves_the_heightmap_untouched() {
        let original = sloped_heightmap();
        let mut probe = sloped_heightmap();
        let _ = build_road_geometry(&probe, &cfg(7), None);
        assert_eq!(
            original.data(),
            probe.data_mut(),
            "build_road_geometry must not carve the terrain"
        );
    }
}
