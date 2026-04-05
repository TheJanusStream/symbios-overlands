//! Water surface `MaterialExtension` for the animated water shader.
//!
//! Extends Bevy's `StandardMaterial` with a custom WGSL fragment shader
//! (`assets/shaders/water.wgsl`) that provides animated wave displacement.
//! The base material is configured with alpha blending and low roughness for
//! a translucent, reflective water appearance.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

/// `MaterialExtension` that replaces the fragment shader with an animated
/// water surface effect. Currently carries no additional uniforms or textures;
/// the wave animation is driven entirely by built-in Bevy globals in the shader.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct WaterExtension {}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }
}

/// Convenience alias for the full extended-material type used by the water volume.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;
