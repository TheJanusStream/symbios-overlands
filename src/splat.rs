//! `SplatExtension` material type and type alias used by the terrain renderer.
//!
//! The actual texture generation and weight-map setup live in [`crate::terrain`].
//! This module contains only the Bevy asset / shader plumbing so it can be
//! referenced from both the [`crate::terrain`] module and the crate root
//! without creating a cycle.

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
///
/// Padded to 16 bytes so the UBO meets WebGL2's `min_uniform_buffer_offset_alignment`
/// /  binding-size constraint (DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED
/// is unsupported on WebGL2; the device-side validator rejects a struct sized 8).
/// Mirror the trailing `_pad` fields in `StainsUniforms` in `splat.wgsl`.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct StainsUniforms {
    /// World-space side length (m) the toroidal stains texture tiles
    /// over. Shader samples it at `fract(world.xz / world_period)`.
    pub world_period: f32,
    /// Non-zero enables the stains modulation; zero passes terrain
    /// through unchanged (backward-compat when no stamper is active).
    pub enabled: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

/// Texture bind slots the splat material's extension consumes: the weight map
/// plus the albedo + normal `2d_array`s (3), and on native the extra stains
/// overlay (4; dropped on wasm — see [`SplatExtension`] for the WebGL2
/// ceiling rationale). Surfaced as the `runtime.texture_bind_slots` gauge
/// (C-5) so the GUI can show headroom against the 16-slot ceiling.
#[cfg(not(target_arch = "wasm32"))]
pub const SPLAT_TEXTURE_BIND_SLOTS: u32 = 4;
#[cfg(target_arch = "wasm32")]
pub const SPLAT_TEXTURE_BIND_SLOTS: u32 = 3;

/// [`MaterialExtension`] that drives `splat.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100/101  weight map + sampler
/// - 102/103  albedo `texture_2d_array` (4 layers) + sampler
/// - 104/105  normal `texture_2d_array` (4 layers) + sampler
/// - 106      [`SplatUniforms`] uniform
/// - 107/108  stains overlay (RGBA: wet/dust/footprint) + sampler (native only — see below)
/// - 109      [`StainsUniforms`] uniform (native only — see below)
///
/// The stains overlay (bindings 107/108/109) is disabled on `wasm32`
/// because wgpu-hal's GLES backend caps each fragment shader at
/// `MAX_TEXTURE_SLOTS = 16` (matches WebGL2's `MAX_TEXTURE_IMAGE_UNITS`).
/// Counting Bevy's view-group shadow + IBL textures plus `StandardMaterial`'s
/// PBR textures, the splat material already sits right at that ceiling; the
/// stains overlay was the +1 that pushed pipeline creation into a panic at
/// wgpu-hal-27.0.4/src/gles/device.rs:87 (`self.sampler_map[16]` overflow).
/// Native + WebGPU keep the feature; WebGL2 simply skips the avatar-stains
/// overlay (terrain renders as it did before #245 landed).
///
/// The corresponding bindings in `assets/shaders/splat.wgsl` are guarded
/// by `#ifdef STAINS_BINDING`, and [`SplatExtension::specialize`] emits
/// that shader-def only on non-wasm targets.
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
    #[cfg(not(target_arch = "wasm32"))]
    #[texture(107)]
    #[sampler(108)]
    pub stains_tex: Handle<Image>,

    #[cfg(not(target_arch = "wasm32"))]
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

    // `descriptor` is only touched on native (the stains shader-def gate).
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
    fn specialize(
        _pipeline: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        // Gate the stains bindings in `splat.wgsl` so they only exist
        // on targets that have the corresponding Rust-side fields.
        #[cfg(not(target_arch = "wasm32"))]
        {
            use bevy::shader::ShaderDefVal;
            let def: ShaderDefVal = "STAINS_BINDING".into();
            descriptor.vertex.shader_defs.push(def.clone());
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push(def);
            }
        }
        Ok(())
    }
}

/// Convenience alias used throughout the terrain module.
pub type SplatTerrainMaterial = ExtendedMaterial<StandardMaterial, SplatExtension>;
