//! Water surface `MaterialExtension` for the animated water shader.
//!
//! Extends Bevy's `StandardMaterial` with a custom WGSL fragment shader
//! (`assets/shaders/water.wgsl`) that computes Gerstner-wave displacement,
//! Fresnel-driven alpha / reflection, scrolling detail normals, foam, and
//! sun-glitter. Every knob that drives the shader flows through the
//! `WaterUniforms` block on this extension — a mix of per-volume parameters
//! (authored on [`crate::pds::WaterSurface`]) and room-wide parameters
//! (authored on [`crate::pds::Environment`]).

use bevy::{
    math::{Vec2, Vec4},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const WATER_SHADER_PATH: &str = "shaders/water.wgsl";

/// GPU uniform block shared with `water.wgsl`. Field ordering is chosen so
/// `Vec4`s lead (16-byte aligned), `Vec2` sits where 8-byte alignment is
/// cheapest, and scalars bring up the rear — the `ShaderType` derive still
/// inserts any padding needed to round the struct up to 16 bytes.
///
/// `scatter_color` is stored as a `Vec4` rather than `Vec3` to avoid the
/// 12-vs-16-byte alignment pitfall that otherwise requires explicit padding
/// members; the alpha channel is unused by the shader.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct WaterUniforms {
    /// Per-volume: sRGBA tint at head-on view (low alpha = transparent).
    pub shallow_color: Vec4,
    /// Per-volume: sRGBA tint at grazing view (high alpha = opaque).
    pub deep_color: Vec4,
    /// Global: subsurface-scatter tint added to wave crests. (rgb used, a=0)
    pub scatter_color: Vec4,
    /// Per-volume: prevailing wave direction in world XZ.
    pub wave_direction: Vec2,
    /// Per-volume: global amplitude multiplier on the Gerstner waves.
    pub wave_scale: f32,
    /// Per-volume: time multiplier. `0` freezes the surface.
    pub wave_speed: f32,
    /// Per-volume: Gerstner steepness in `[0, 1]`.
    pub wave_choppiness: f32,
    /// Per-volume: PBR perceptual roughness override.
    pub roughness: f32,
    /// Per-volume: PBR metallic override.
    pub metallic: f32,
    /// Per-volume: Schlick F0 reflectance at head-on view.
    pub reflectance: f32,
    /// Per-volume: strength of the procedural foam on wave crests.
    pub foam_amount: f32,
    /// Global: close-distance detail normal tiling (1/world-m).
    pub normal_scale_near: f32,
    /// Global: far-distance detail normal tiling (1/world-m).
    pub normal_scale_far: f32,
    /// Global: reserved for screen-space refraction distortion.
    pub refraction_strength: f32,
    /// Global: specular sun-glitter intensity.
    pub sun_glitter: f32,
    /// Global: shoreline foam band width (m). Reserved.
    pub shore_foam_width: f32,
}

/// [`MaterialExtension`] that drives `water.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100 [`WaterUniforms`] uniform
///
/// Prepass is disabled — water is transparent so it must not write depth in
/// the prepass pass, otherwise a shoreline would occlude every fragment the
/// main pass would try to blend underneath it.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default, Debug)]
pub struct WaterExtension {
    #[uniform(100)]
    pub uniforms: WaterUniforms,
}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER_PATH.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Convenience alias for the full extended-material type used by the water volume.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;
