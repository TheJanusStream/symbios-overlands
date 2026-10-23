//! Shared fixtures for the urban road tests, compiled only under `cfg(test)`.
//! The heightmaps are real generated terrain rather than flat planes - tensor
//! needs crossing slopes before its major/minor directions enclose any blocks at
//! all - and the pilot seed is frozen from the era when seeded urban rooms
//! derived one, so the geometry the fixtures assert against stays byte-identical.
//! `extrude_chain` keeps the pre-#584 single-chain levelling path alive so cap /
//! mouth / truncation tests can read one chain's geometry without the network
//! levelling pass in the way.

mod mesh;

/// The water every urban test hands the tracer, by name.
pub(crate) use crate::water::bodies::RoomWater;
pub(crate) use mesh::*;

use bevy_symbios_ground::{FbmNoise, HeightMap, TerrainGenerator};
use symbios_tensor::{RoadGraph, RoadType};

use crate::pds::generator::RoadConfig;
use crate::urban::graph::active_adjacency;
use crate::urban::levelling::RawFrame;
use crate::urban::truncation::{ChainEnds, Hub};
use crate::urban::{
    Chain, ChainSample, Dims, RoadEnd, RoadGeometry, RoadParts, extrude_ribbon, level_chain,
    sample_chain,
};

/// Drape and extrude ONE chain with its natural (un-pinned) single-chain
/// levelling - the pre-#584 path, so cap / mouth / truncation tests read the
/// same per-chain geometry independent of the network levelling pass. Mirrors
/// the old `extrude_chain` signature so those tests need no changes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn extrude_chain(
    chain: &Chain,
    start_trim: f32,
    end_trim: f32,
    hm: &HeightMap,
    world_offset: [f32; 2],
    dims: &Dims,
    degree: &[u32],
    road_ends: &mut Vec<RoadEnd>,
    parts: &mut RoadParts,
) {
    if let Some(s) = sample_chain(chain, start_trim, end_trim, hm, dims) {
        let floor: Vec<f32> = s.frames.iter().map(|r| r.floor).collect();
        let base_y = level_chain(&floor, &s.seg, [None, None]);
        extrude_ribbon(
            chain,
            &s,
            &base_y,
            world_offset,
            dims,
            node_ends(chain, degree, [start_trim, end_trim]),
            road_ends,
            parts,
        );
    }
}

/// How `chain`'s ends close when every junction node (degree ≥ 3) is its
/// own hub, numbered by the node, and a degree-1 or clipped end is capped -
/// the plan of a network with no junction cluster, for tests that drive one
/// chain at a time. Pair it with [`node_hubs`].
pub(crate) fn node_ends(chain: &Chain, degree: &[u32], trim: [f32; 2]) -> ChainEnds {
    let deg = |slot: usize| degree.get(chain.end_nodes[slot]).copied().unwrap_or(0);
    ChainEnds {
        hub: [0, 1].map(|slot| (deg(slot) >= 3).then_some(chain.end_nodes[slot])),
        cap: [0, 1].map(|slot| deg(slot) == 1 || chain.clip[slot]),
        trim,
    }
}

/// A single-node hub at `at` holding node 0 - for tests that build its
/// road ends by hand.
pub(crate) fn hub_at(at: (f32, f32)) -> Hub {
    Hub {
        nodes: vec![0],
        points: vec![at],
        links: Vec::new(),
        centre: at,
        arms: Vec::new(),
    }
}

/// A small heightmap with real slopes - tensor needs non-flat terrain for
/// the major/minor directions to cross and enclose blocks.
pub(crate) fn sloped_heightmap() -> HeightMap {
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
pub(crate) fn cfg(seed: u64) -> RoadConfig {
    RoadConfig {
        seed,
        ..RoadConfig::default()
    }
}

/// The three surface buffers, for tests that sweep every emitted vertex.
pub(crate) fn surfaces(p: &RoadParts) -> [&RoadGeometry; 3] {
    [&p.deck, &p.structure, &p.neon]
}

/// The pilot room's heightmap at real ~1 km scale (256², cyberpunk terrain
/// seed) - big enough that the road network encloses real city blocks.
pub(crate) fn pilot_heightmap() -> HeightMap {
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

/// The pilot room's road seed - frozen from the era when seeded urban rooms
/// derived one (terrain seed ^ the then road-seed salt), so the fixture
/// geometry these tests assert against stays byte-identical.
pub(crate) const PILOT_ROAD_SEED: u64 = 4167901772298833237_u64 ^ 0xA0D5_EED5_A170_0001;

/// #583 weld scenario helper: build a graph from XZ node positions and minor
/// edges (by node index). Uses glam 0.30 (a dev-dependency matching tensor) so
/// the `Vec2` type lines up with `RoadGraph::add_node`.
pub(crate) fn weld_graph(nodes: &[(f32, f32)], edges: &[(u32, u32)]) -> RoadGraph {
    let mut g = RoadGraph::default();
    for &(x, z) in nodes {
        g.add_node(glam::Vec2::new(x, z));
    }
    for &(s, e) in edges {
        g.add_edge(s, e, RoadType::Minor);
    }
    g
}

/// A graph from XZ node positions (window metres) and `(start, end,
/// major)` edges.
pub(crate) fn typed_graph(nodes: &[(f32, f32)], edges: &[(u32, u32, bool)]) -> RoadGraph {
    let mut g = RoadGraph::default();
    for &(x, z) in nodes {
        g.add_node(glam::Vec2::new(x, z));
    }
    for &(s, e, major) in edges {
        g.add_edge(
            s,
            e,
            if major {
                RoadType::Major
            } else {
                RoadType::Minor
            },
        );
    }
    g
}

/// Active degree of every node (built from `active` edges), for weld assertions.
pub(crate) fn active_degrees(g: &RoadGraph) -> Vec<usize> {
    active_adjacency(g).iter().map(Vec::len).collect()
}

/// Build a ChainSample from frame centres + per-frame terrain floor (the only
/// fields the levelling reads), for #584 levelling tests.
pub(crate) fn mk_sample(centres: &[(f32, f32)], floors: &[f32]) -> ChainSample {
    let frames: Vec<RawFrame> = centres
        .iter()
        .zip(floors)
        .map(|(&(cx, cz), &floor)| RawFrame {
            cx,
            cz,
            rx: 0.0,
            rz: 1.0,
            scale: 1.0,
            arc: 0.0,
            floor,
        })
        .collect();
    let seg: Vec<f32> = frames
        .windows(2)
        .map(|w| (w[1].cx - w[0].cx).hypot(w[1].cz - w[0].cz).max(1.0e-3))
        .collect();
    ChainSample { frames, seg }
}

/// The pilot room's water with a lake in `config`'s district (#1727): over
/// its lowest ground there, at the height a third of the pilot's cells stand
/// under, its rectangle twice the world's so it reaches as far as its ground
/// lets it - and the sea a metre below all of the ground, so the tracer's one
/// water line, the sea's, sees none of it on its own.
pub(crate) fn pilot_lake(hm: &HeightMap, config: &RoadConfig) -> RoomWater {
    let (lo, side) =
        crate::urban::graph::district_window(hm, config).expect("the pilot district has a window");
    let mut heights: Vec<f32> = hm.data().to_vec();
    heights.sort_by(f32::total_cmp);
    let level = heights[heights.len() / 3];
    let lowest = (lo[1]..lo[1] + side)
        .flat_map(|z| (lo[0]..lo[0] + side).map(move |x| (x, z)))
        .min_by(|a, b| hm.get(a.0, a.1).total_cmp(&hm.get(b.0, b.1)))
        .expect("a window has cells");
    let (x, z) =
        crate::water::bodies::CellGrid::of(hm).world_of((lowest.0 as u32, lowest.1 as u32));
    let layer = bevy::prelude::Transform::from_xyz(x, level, z)
        .with_scale(bevy::prelude::Vec3::new(2.0, 1.0, 2.0));
    let lake =
        crate::water::bodies::lake(hm, &layer).expect("the district's lowest ground holds a lake");
    RoomWater {
        sea: Some(hm.data().iter().copied().fold(f32::INFINITY, f32::min) - 1.0),
        bodies: vec![lake],
    }
}
