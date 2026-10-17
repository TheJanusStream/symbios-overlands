//! The grass's material (#1675): a `StandardMaterial`, lit as every other
//! surface is - the sun and its shadows, the cloud shade, the sky light,
//! the fog - with the vertex shader that stands each blade on the ground,
//! `assets/shaders/grass.wgsl`.
//!
//! The shader reads the ground through [`GroundCover`]'s three textures,
//! all in the vertex stage: the height texture by `textureLoad` (a 32-bit
//! float cannot be filtered on WebGL2), the splat weights and the cover
//! linearly; and the bodies that part the grass from a fourth, a row of
//! 32-bit float texels, by `textureLoad` too. On the WebGL2 build every
//! texture in a pipeline's layout takes one of the GLES backend's 16 slots
//! whatever stage reads it: the view's five, `StandardMaterial`'s six and
//! these four make 15. One is left; a texture more is a panic at pipeline
//! creation on WebGL2. Room past that means a material of its own instead
//! of an extended `StandardMaterial`, which would take its six slots back.
//! WebGPU counts by stage, and these four are the vertex stage's.
//!
//! The bodies are a texture, not uniforms, because they move most frames:
//! writing the material rebuilds its bind group and re-specializes every
//! tile, where a texture written at the same size is copied into the one
//! the material already binds.
//!
//! The grass casts no shadow and stays out of the depth prepass: a field
//! of blades in the shadow maps costs more than it shows, and the prepass
//! would need the vertex shader again for nothing the water's shore foam
//! (the prepass's reader) misses.
//!
//! [`GroundCover`]: crate::ground_cover::GroundCover

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

const GRASS_SHADER_PATH: &str = "shaders/grass.wgsl";

/// The grass's material: a `StandardMaterial` and [`GrassExtension`].
pub type GrassMaterial = ExtendedMaterial<StandardMaterial, GrassExtension>;

/// The uniform block `grass.wgsl` reads, mirrored there as
/// `GrassUniforms`: every member a `vec4`, so the two layouts cannot
/// disagree about padding, and 256 bytes, a multiple of 16 as WebGL2
/// requires. Colours are linear.
#[derive(Clone, Debug, Default, PartialEq, ShaderType)]
pub struct GrassUniforms {
    /// A blade's colour at its root, and in `w` its height (m).
    pub base: Vec4,
    /// Its colour at its tip, and in `w` its width at the root (m).
    pub tip: Vec4,
    /// A dry blade's colour, and in `w` the share of blades that are dry.
    pub dry: Vec4,
    /// Up to four flower colours, `w` 1 for each in use.
    pub flowers: [Vec4; 4],
    /// How thickly grass grows on each of the four splat layers.
    pub layer_density: Vec4,
    /// Each splat layer's colour from a distance, which the field's far
    /// blades turn to.
    pub ground: [Vec4; 4],
    /// The ground: its half-width (m), its cell (m) and its vertices a
    /// side, and in `w` the share of blades that are flowers.
    pub frame: Vec4,
    /// The wind on XZ, its speed (m/s), and in `w` how far a tip leans at
    /// the reference speed (m).
    pub wind: Vec4,
    /// Where the field thins (m): it keeps every blade out to `x`, the
    /// middle share by `y` and the far share by `z`; `w` is its reach.
    pub reach: Vec4,
    /// The middle and far shares, and where the field starts to fade out
    /// (m); `w` how stormy (#1684), for the gust fronts.
    pub shares: Vec4,
    /// The weather on the ground (#1683), as the ground's shader has it: how
    /// wet, how much snow lies where it has settled, the snow line (world
    /// Y) and its edge (m).
    pub weather: Vec4,
}

/// [`MaterialExtension`] that drives `grass.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100 [`GrassUniforms`]
/// - 101 the ground's height texture (`R32Float`, unfilterable)
/// - 102/103 the splat weight map + sampler
/// - 104/105 the cover + sampler
/// - 106 the bodies (`Rgba32Float`, unfilterable): each one's middle on XZ,
///   its underside on Y and how far around it the grass parts, nearest the
///   camera first, the unused texels 0
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct GrassExtension {
    #[uniform(100)]
    pub uniforms: GrassUniforms,
    #[texture(101, visibility(vertex), sample_type = "float", filterable = false)]
    pub height: Handle<Image>,
    #[texture(102, visibility(vertex))]
    #[sampler(103, visibility(vertex))]
    pub weights: Handle<Image>,
    #[texture(104, visibility(vertex))]
    #[sampler(105, visibility(vertex))]
    pub cover: Handle<Image>,
    #[texture(106, visibility(vertex), sample_type = "float", filterable = false)]
    pub benders: Handle<Image>,
}

impl MaterialExtension for GrassExtension {
    fn vertex_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// The `StandardMaterial` the blades are lit as: white, so the colour the
/// shader writes is the colour lit; matte; both faces drawn (no culling)
/// and lit alike - not `double_sided`, which would have the PBR fragment
/// turn a back face's normal away, where the shader's already faces the
/// camera and leans toward the sky.
pub fn grass_base() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.8,
        reflectance: 0.25,
        cull_mode: None,
        double_sided: false,
        ..default()
    }
}
