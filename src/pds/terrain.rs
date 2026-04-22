//! Terrain generator payload (ported from `symbios-ground-lab`): algorithm
//! selection, erosion tuning, and the four-layer splat/material configuration
//! used by the ground compiler.

use super::texture::{SovereignGroundConfig, SovereignRockConfig, SovereignTextureConfig};
use super::types::{Fp, Fp3, Fp64, u64_as_string};
use serde::{Deserialize, Serialize};

/// Which base terrain algorithm to run. Mirrors `ground-lab::GeneratorKind`
/// but tagged for lexicon-safe serde.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignGeneratorKind {
    FbmNoise,
    DiamondSquare,
    #[default]
    VoronoiTerracing,
}

/// Full terrain configuration stored inside a `Generator::Terrain` variant.
/// This is a serialisable mirror of `ground-lab::TerrainConfig` — all `f32`
/// fields are wrapped in [`Fp`] so the record stays DAG-CBOR compliant.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignTerrainConfig {
    // Grid / world
    pub grid_size: u32,
    pub cell_scale: Fp,
    pub height_scale: Fp,

    // Algorithm selection
    pub generator_kind: SovereignGeneratorKind,
    #[serde(with = "u64_as_string")]
    pub seed: u64,

    // FBM params
    pub octaves: u32,
    pub persistence: Fp,
    pub lacunarity: Fp,
    pub base_frequency: Fp,

    // Diamond Square params
    pub ds_roughness: Fp,

    // Voronoi params
    pub voronoi_num_seeds: u32,
    pub voronoi_num_terraces: u32,

    // Hydraulic erosion
    pub erosion_enabled: bool,
    pub erosion_drops: u32,
    pub inertia: Fp,
    pub erosion_rate: Fp,
    pub deposition_rate: Fp,
    pub evaporation_rate: Fp,
    pub capacity_factor: Fp,

    // Thermal erosion
    pub thermal_enabled: bool,
    pub thermal_iterations: u32,
    pub thermal_talus_angle: Fp,

    // Material (splat) config
    pub material: SovereignMaterialConfig,
}

impl Default for SovereignTerrainConfig {
    fn default() -> Self {
        Self {
            grid_size: 512,
            cell_scale: Fp(2.0),
            height_scale: Fp(50.0),

            generator_kind: SovereignGeneratorKind::VoronoiTerracing,
            seed: 42,

            octaves: 6,
            persistence: Fp(0.5),
            lacunarity: Fp(2.0),
            base_frequency: Fp(4.0),

            ds_roughness: Fp(0.5),

            voronoi_num_seeds: 1000,
            voronoi_num_terraces: 2,

            erosion_enabled: true,
            erosion_drops: 50_000,
            inertia: Fp(0.05),
            erosion_rate: Fp(0.3),
            deposition_rate: Fp(0.3),
            evaporation_rate: Fp(0.02),
            capacity_factor: Fp(8.0),

            thermal_enabled: true,
            thermal_iterations: 30,
            thermal_talus_angle: Fp(0.05),

            material: SovereignMaterialConfig::default(),
        }
    }
}

/// Splat rule for a single texture layer. `[0, 1]` normalised height; slope
/// is raw gradient magnitude in `[0, ∞)` (1.0 ≈ 45°).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SovereignSplatRule {
    pub height_min: Fp,
    pub height_max: Fp,
    pub slope_min: Fp,
    pub slope_max: Fp,
    pub sharpness: Fp,
}

/// Four-layer splat/texture configuration for a terrain generator.
///
/// `rules[i]` controls where layer `i` appears on the terrain (altitude and
/// slope bands); `layers[i]` is the procedural texture generator config that
/// bakes that layer's albedo/normal/ORM maps. Any
/// [`SovereignTextureConfig`] variant may appear in any slot — the canonical
/// defaults are Grass / Dirt / Rock / Snow (Ground / Ground / Rock / Ground),
/// but a room can swap any layer for e.g. `Brick`, `Cobblestone`, `Thatch`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignMaterialConfig {
    pub texture_size: u32,
    pub tile_scale: Fp,
    /// Splat rules for channels R, G, B, A — one per layer.
    pub rules: [SovereignSplatRule; 4],
    /// Procedural texture configs for channels R, G, B, A.
    pub layers: [SovereignTextureConfig; 4],
}

impl Default for SovereignMaterialConfig {
    fn default() -> Self {
        Self {
            texture_size: 512,
            tile_scale: Fp(90.0),
            rules: [
                // R — Grass
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(0.45),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.30),
                    sharpness: Fp(0.5),
                },
                // G — Dirt
                SovereignSplatRule {
                    height_min: Fp(0.30),
                    height_max: Fp(0.65),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.55),
                    sharpness: Fp(0.5),
                },
                // B — Rock
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.25),
                    slope_max: Fp(1.0),
                    sharpness: Fp(0.5),
                },
                // A — Snow
                SovereignSplatRule {
                    height_min: Fp(0.88),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.0),
                    slope_max: Fp(1.0),
                    sharpness: Fp(4.0),
                },
            ],
            layers: [
                // R — Grass
                SovereignTextureConfig::Ground(SovereignGroundConfig {
                    seed: 1,
                    macro_scale: Fp64(2.5),
                    macro_octaves: 4,
                    micro_scale: Fp64(10.0),
                    micro_octaves: 3,
                    micro_weight: Fp64(0.3),
                    color_dry: Fp3([0.07, 0.12, 0.03]),
                    color_moist: Fp3([0.03, 0.07, 0.01]),
                    normal_strength: Fp(4.5),
                }),
                // G — Dirt
                SovereignTextureConfig::Ground(SovereignGroundConfig::default()),
                // B — Rock
                SovereignTextureConfig::Rock(SovereignRockConfig::default()),
                // A — Snow
                SovereignTextureConfig::Ground(SovereignGroundConfig {
                    seed: 99,
                    macro_scale: Fp64(4.0),
                    macro_octaves: 3,
                    micro_scale: Fp64(12.0),
                    micro_octaves: 3,
                    micro_weight: Fp64(0.4),
                    color_dry: Fp3([0.95, 0.95, 0.98]),
                    color_moist: Fp3([0.80, 0.82, 0.88]),
                    normal_strength: Fp(0.8),
                }),
            ],
        }
    }
}
