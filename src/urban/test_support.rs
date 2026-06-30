use bevy_symbios_ground::{FbmNoise, HeightMap, TerrainGenerator};
use symbios_tensor::{RoadGraph, RoadType};

use crate::pds::generator::RoadConfig;
use crate::urban::graph::active_adjacency;
use crate::urban::levelling::RawFrame;
use crate::urban::{
    Chain, ChainSample, Dims, RoadEnd, RoadGeometry, RoadParts, extrude_ribbon, level_chain,
    sample_chain,
};

/// Drape and extrude ONE chain with its natural (un-pinned) single-chain
/// levelling — the pre-#584 path, so cap / mouth / truncation tests read the
/// same per-chain geometry independent of the network levelling pass. Mirrors
/// the old `extrude_chain` signature so those tests need no changes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn extrude_chain(
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
    if let Some(s) = sample_chain(chain, start_trim, end_trim, hm, dims) {
        let floor: Vec<f32> = s.frames.iter().map(|r| r.floor).collect();
        let base_y = level_chain(&floor, &s.seg, [None, None]);
        extrude_ribbon(
            chain,
            &s,
            &base_y,
            world_offset,
            dims,
            degree,
            road_ends,
            parts,
        );
    }
}

/// A small heightmap with real slopes — tensor needs non-flat terrain for
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
/// seed) — big enough that the road network encloses real city blocks.
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

/// The pilot room's derived road seed (terrain seed ^ ROAD_SEED_SALT).
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
            ground: floor - 5.0,
        })
        .collect();
    let seg: Vec<f32> = frames
        .windows(2)
        .map(|w| (w[1].cx - w[0].cx).hypot(w[1].cz - w[0].cz).max(1.0e-3))
        .collect();
    ChainSample { frames, seg }
}
