//! `SplatExtension` material type and type alias used by the terrain renderer.
//!
//! The actual texture generation and weight-map setup live in [`crate::terrain`].
//! This module contains only the Bevy asset / shader plumbing so it can be
//! referenced from both `terrain.rs` and the crate root without creating a cycle.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const SPLAT_SHADER_PATH: &str = "shaders/splat.wgsl";

/// GPU uniform block shared with `splat.wgsl`.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct SplatUniforms {
    /// How many times the tiling textures repeat across the terrain.
    pub tile_scale: f32,
    /// Non-zero enables splat blending; zero passes through the base colour.
    pub enabled: u32,
    /// World-space UV scale for the Rock triplanar projection.
    /// Set to `tile_scale / world_extent` so rock density matches the top-down layers.
    pub triplanar_scale: f32,
    /// Blend sharpness for triplanar axis transitions (4 is a good default).
    pub triplanar_sharpness: f32,
}

/// GPU uniform block for the avatar-interaction stains overlay
/// (Phase 3, #245), shared with `splat.wgsl`.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct StainsUniforms {
    /// World-space side length (m) the toroidal stains texture tiles
    /// over. Shader samples it at `fract(world.xz / world_period)`.
    pub world_period: f32,
    /// Non-zero enables the stains modulation; zero passes terrain
    /// through unchanged (backward-compat when no stamper is active).
    pub enabled: u32,
}

/// [`MaterialExtension`] that drives `splat.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100/101  weight map + sampler
/// - 102/103  albedo `texture_2d_array` (4 layers) + sampler
/// - 104/105  normal `texture_2d_array` (4 layers) + sampler
/// - 106      [`SplatUniforms`] uniform
/// - 107/108  stains overlay (RGBA: wet/dust/footprint) + sampler
/// - 109      [`StainsUniforms`] uniform
#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct SplatExtension {
    #[texture(100)]
    #[sampler(101)]
    pub weight_map: Handle<Image>,

    #[texture(102, dimension = "2d_array")]
    #[sampler(103)]
    pub albedo_array: Handle<Image>,

    #[texture(104, dimension = "2d_array")]
    #[sampler(105)]
    pub normal_array: Handle<Image>,

    #[uniform(106)]
    pub uniforms: SplatUniforms,

    /// Avatar-interaction stains overlay (#245). Defaults to Bevy's
    /// 1×1 white image; `stains.enabled` stays 0 until the stamper
    /// binds the real texture, so terrain renders unchanged meanwhile.
    #[texture(107)]
    #[sampler(108)]
    pub stains_tex: Handle<Image>,

    #[uniform(109)]
    pub stains: StainsUniforms,
}

impl MaterialExtension for SplatExtension {
    fn fragment_shader() -> ShaderRef {
        SPLAT_SHADER_PATH.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        SPLAT_SHADER_PATH.into()
    }
}

/// Convenience alias used throughout the terrain module.
pub type SplatTerrainMaterial = ExtendedMaterial<StandardMaterial, SplatExtension>;
