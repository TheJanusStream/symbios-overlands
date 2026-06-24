//! Shared, platform-agnostic generation jobs for the compute-offload layer.
//!
//! A [`GenJob`] is a self-contained, serialisable description of a CPU-heavy
//! generation task; [`GenJob::run`] executes it **purely** (no Bevy, no I/O,
//! no globals — deterministic from the job's seed alone) and returns
//! serialisable [`GenResult`] data. The same `run()` is invoked by the app's
//! native `AsyncComputeTaskPool` backend and inside the wasm Web Worker, so
//! native and worker execution are byte-identical (the determinism invariant
//! the terrain pipeline already relies on).
//!
//! This crate deliberately depends only on the **Bevy-free** `symbios-ground`
//! core (which `bevy_symbios_ground` merely re-exports), so the worker `.wasm`
//! that links it stays tiny (~16 KB gzipped) instead of pulling the engine.

use serde::{Deserialize, Serialize};
use symbios_ground::{
    DiamondSquare, FbmNoise, HeightMap, HydraulicErosion, TerrainGenerator, ThermalErosion,
    VoronoiTerracing,
};

/// Base terrain algorithm. Mirrors the app's `SovereignGeneratorKind` (kept
/// independent so this crate stays free of the app and Bevy).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorKind {
    FbmNoise,
    DiamondSquare,
    VoronoiTerracing,
}

/// Plain, serialisable inputs for a heightmap generation job — the distilled
/// generation-relevant subset of the app's terrain config (no material/splat
/// fields, which are a separate generation concern).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HeightmapParams {
    pub grid_size: u32,
    pub cell_scale: f32,
    pub height_scale: f32,
    pub generator_kind: GeneratorKind,
    pub seed: u64,
    pub octaves: u32,
    pub persistence: f32,
    pub lacunarity: f32,
    pub base_frequency: f32,
    pub ds_roughness: f32,
    pub voronoi_num_seeds: u32,
    pub voronoi_num_terraces: u32,
    pub erosion_enabled: bool,
    pub erosion_drops: u32,
    pub inertia: f32,
    pub erosion_rate: f32,
    pub deposition_rate: f32,
    pub evaporation_rate: f32,
    pub capacity_factor: f32,
    pub thermal_enabled: bool,
    pub thermal_iterations: u32,
    pub thermal_talus_angle: f32,
}

/// Generated heightmap data — plain row-major `f32` heights plus the dimensions
/// needed to rebuild a `symbios_ground::HeightMap` on the consuming side. The
/// `data` buffer is the large payload transferred (zero-copy) back from the
/// worker on wasm.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HeightmapData {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub data: Vec<f32>,
}

/// A self-contained generation job. New offloadable hotspots (texture/audio
/// bake, mesh gen) get added as further variants.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum GenJob {
    Heightmap(HeightmapParams),
}

/// The output of a [`GenJob`], paired by variant with the job that produced it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum GenResult {
    Heightmap(HeightmapData),
}

// Hydraulic-erosion tuning fixed by the app (mirror of
// `config::terrain::hydraulic`). Kept here as constants so the job stays fully
// self-contained — these are engine-fixed, not per-request inputs.
const HYDRAULIC_MAX_STEPS: u32 = 64;
const HYDRAULIC_MIN_SLOPE: f32 = 0.01;
const HYDRAULIC_WATER_LEVEL: f32 = 0.0;

impl GenJob {
    /// Execute the job purely on the current thread.
    pub fn run(self) -> GenResult {
        match self {
            GenJob::Heightmap(p) => GenResult::Heightmap(run_heightmap(p)),
        }
    }
}

/// Faithful port of the app's `terrain::heightmap::generate_terrain`, operating
/// on plain params and returning plain data. Reproducible from `seed` alone.
fn run_heightmap(p: HeightmapParams) -> HeightmapData {
    let grid = (p.grid_size as usize).max(2);
    let mut hm = HeightMap::new(grid, grid, p.cell_scale.max(0.01));

    match p.generator_kind {
        GeneratorKind::FbmNoise => {
            FbmNoise {
                seed: p.seed,
                octaves: p.octaves.clamp(1, 32),
                persistence: p.persistence,
                lacunarity: p.lacunarity,
                base_frequency: p.base_frequency,
            }
            .generate(&mut hm);
            hm.normalize();
        }
        GeneratorKind::DiamondSquare => {
            DiamondSquare::new(p.seed, p.ds_roughness).generate(&mut hm);
            hm.normalize();
        }
        GeneratorKind::VoronoiTerracing => {
            VoronoiTerracing::new(
                p.seed,
                p.voronoi_num_seeds.max(1) as usize,
                p.voronoi_num_terraces.max(1) as usize,
            )
            .generate(&mut hm);
            // Voronoi already emits bounded [0, 1] output.
        }
    }

    for v in hm.data_mut() {
        *v *= p.height_scale;
    }

    if p.erosion_enabled {
        HydraulicErosion {
            seed: p.seed,
            num_drops: p.erosion_drops,
            max_steps: HYDRAULIC_MAX_STEPS,
            inertia: p.inertia,
            erosion_rate: p.erosion_rate,
            deposition_rate: p.deposition_rate,
            evaporation_rate: p.evaporation_rate,
            capacity_factor: p.capacity_factor,
            min_slope: HYDRAULIC_MIN_SLOPE,
            water_level: HYDRAULIC_WATER_LEVEL,
            ..HydraulicErosion::new(p.seed)
        }
        .erode(&mut hm);
    }

    if p.thermal_enabled {
        ThermalErosion::new()
            .with_iterations(p.thermal_iterations)
            .with_talus_angle(p.thermal_talus_angle)
            .erode(&mut hm);
    }

    HeightmapData {
        width: hm.width() as u32,
        height: hm.height() as u32,
        scale: hm.scale(),
        data: hm.data().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(seed: u64) -> HeightmapParams {
        HeightmapParams {
            grid_size: 16,
            cell_scale: 1.0,
            height_scale: 10.0,
            generator_kind: GeneratorKind::FbmNoise,
            seed,
            octaves: 4,
            persistence: 0.5,
            lacunarity: 2.0,
            base_frequency: 0.05,
            ds_roughness: 0.5,
            voronoi_num_seeds: 8,
            voronoi_num_terraces: 2,
            erosion_enabled: true,
            erosion_drops: 200,
            inertia: 0.05,
            erosion_rate: 0.3,
            deposition_rate: 0.3,
            evaporation_rate: 0.02,
            capacity_factor: 4.0,
            thermal_enabled: true,
            thermal_iterations: 5,
            thermal_talus_angle: 0.7,
        }
    }

    fn run(p: HeightmapParams) -> HeightmapData {
        match GenJob::Heightmap(p).run() {
            GenResult::Heightmap(d) => d,
        }
    }

    /// The whole offload design relies on native and the wasm worker producing
    /// byte-identical output from the same seed (cross-peer determinism).
    #[test]
    fn heightmap_is_deterministic_from_seed() {
        let a = run(params(1337));
        let b = run(params(1337));
        assert_eq!(a, b);
        assert_eq!(a.data.len(), (a.width * a.height) as usize);
        assert_eq!((a.width, a.height), (16, 16));
    }

    #[test]
    fn distinct_seeds_differ() {
        assert_ne!(run(params(1)).data, run(params(2)).data);
    }
}
