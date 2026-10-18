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
    /// World Y of the room's water surface - the datum the damp-ground
    /// darkening measures from (#913). Sourced from
    /// `world_builder::compile::drawn_water_level` (Berlin's level in a
    /// geodata region, #1586), the same single definition the scatter
    /// sampler's riparian band uses, so the darkened margin and the reeds
    /// standing in it agree.
    pub water_y: f32,
    /// Height above `water_y` (m) over which the darkening eases out.
    pub moisture_depth: f32,
    /// Fraction removed from the albedo at the water line. Zero disables
    /// the effect entirely and restores the pre-#913 terrain exactly.
    pub moisture_strength: f32,
    /// View distance (m) where the albedo starts cross-fading to each
    /// layer's mean colour (#1320). The shader fades only while
    /// `albedo_fade_far > albedo_fade_near`, so the zeroed `Default` renders
    /// the pre-#1320 terrain.
    pub albedo_fade_near: f32,
    /// View distance (m) where the albedo is fully each layer's mean colour.
    pub albedo_fade_far: f32,
    /// The weather on the ground (#1683): how wet it is, 0 dry to 1 soaked
    /// (darker, glossy, puddled on the flat).
    pub wetness: f32,
    /// How much snow lies where it has settled, 0 to 1.
    pub snow: f32,
    /// The snow line (world Y): the snow lies over it, its line falling
    /// from over the highest ground to under the lowest as the snow
    /// settles (`terrain::splat::ground_weather`).
    pub snow_line: f32,
    /// The snow line's edge (m), either side of it.
    pub snow_feather: f32,
    /// How strongly the ground shows the grass growing on it (#1695): 1
    /// while the grass grows, 0 while it is off - and in the zeroed
    /// `Default`, so a ground without grass renders as before.
    pub grass_carpet: f32,
    /// The share of the room's blades that are dry.
    pub grass_dry_share: f32,
    /// How stormy the wind is, 0 to 1 (#1684): a storm's gust fronts lean
    /// the grass harder, and the ground shows them.
    pub storm: f32,
    /// The room's blades' colour at the root, linear RGB in `xyz`
    /// (`grass::profile::GrassProfile`). The three scalars above bring the
    /// block to 64 bytes, where this one starts on its 16-byte boundary.
    pub grass_base: Vec4,
    /// Their colour at the tip, linear RGB in `xyz`.
    pub grass_tip: Vec4,
    /// A dry blade's colour, linear RGB in `xyz`.
    pub grass_dry: Vec4,
    /// How thickly the grass grows on each splat layer, 0 to 1.
    pub grass_layer_density: Vec4,
    /// The wind the grass leans in: its direction in `xy`, its speed (m/s)
    /// in `z`.
    pub wind: Vec4,
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
/// plus the albedo + normal `2d_array`s (3), and off WebGL2 the extra stains
/// overlay (4; dropped on the WebGL2 build - see [`SplatExtension`] for the
/// ceiling). Surfaced as the `runtime.texture_bind_slots` gauge (C-5). It is
/// the extension's own share: on the WebGL2 build Bevy's view and
/// `StandardMaterial` put eleven more in the same pipeline (#1672, #1680),
/// twelve off it with the depth prepass's.
#[cfg(not(webgl2))]
pub const SPLAT_TEXTURE_BIND_SLOTS: u32 = 4;
#[cfg(webgl2)]
pub const SPLAT_TEXTURE_BIND_SLOTS: u32 = 3;

/// [`MaterialExtension`] that drives `splat.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100/101  weight map + sampler
/// - 102/103  albedo `texture_2d_array` (4 layers) + sampler
/// - 104/105  normal `texture_2d_array` (4 layers) + sampler
/// - 106      [`SplatUniforms`] uniform
/// - 107/108  stains overlay (RGBA: wet/dust/footprint) + sampler (not on WebGL2 - see below)
/// - 109      [`StainsUniforms`] uniform (not on WebGL2 - see below)
///
/// The stains overlay (bindings 107/108/109) is disabled on the WebGL2
/// build because wgpu-hal's GLES backend caps each fragment shader at
/// `MAX_TEXTURE_SLOTS = 16` (matches WebGL2's `MAX_TEXTURE_IMAGE_UNITS`), and
/// when the overlay came the splat material sat right at that ceiling: it was
/// the +1 that pushed pipeline creation into a panic at
/// wgpu-hal-27.0.4/src/gles/device.rs:87 (`self.sampler_map[16]` overflow).
///
/// Measured again on Bevy 0.19 (#1672), the ceiling is no longer that close.
/// The GLES backend gives every texture in a pipeline's layout a slot, used
/// or not, and the WebGL2 terrain took 12: the view's three (two shadow maps
/// and the transmission texture - the camera is HDR, for its bloom; without
/// HDR Bevy tonemaps in the shader and binds its LUT, a fourth),
/// `StandardMaterial`'s six (none of Bevy's optional PBR texture features is
/// on) and the extension's three. The sky light's environment map (#1680)
/// has since added its two, for 14; a WebGL2 build with the map and this
/// overlay (+1, 15) ran with the terrain lit by the map (2026-10-16). The
/// overlay stays off there until it is turned back on deliberately (#1681).
/// `tests/shader_guards.rs` holds these counts to their budgets.
///
/// Native and the WebGPU build keep it (#1670): WebGPU counts sampled
/// textures and samplers apart, and a world with the overlay and the depth
/// prepass drew whole under WebGPU's minimum limits
/// (`WGPU_SETTINGS_PRIO=webgpu`, 16 of each a stage) - and again with the
/// sky light's two maps (#1680), at exactly 16 sampled textures: the view's
/// six, `StandardMaterial`'s six and these four. Another texture on the
/// terrain must come out of these, not be added beside them. So the gate is the
/// backend each build is made for, which a build knows. #1131 asked for a
/// runtime check on the adapter instead, on the premise that the one web
/// binary could land on either backend; it never could - without Bevy's
/// `webgpu` feature wgpu is asked for `Backends::GL` alone - and each
/// backend now has a binary of its own.
///
/// The corresponding bindings in `assets/shaders/splat.wgsl` are guarded
/// by `#ifdef STAINS_BINDING`, and [`SplatExtension::specialize`] emits
/// that shader-def everywhere but the WebGL2 build.
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
    #[cfg(not(webgl2))]
    #[texture(107)]
    #[sampler(108)]
    pub stains_tex: Handle<Image>,

    #[cfg(not(webgl2))]
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

    // `descriptor` is only touched off WebGL2 (the stains shader-def gate).
    #[cfg_attr(webgl2, allow(unused_variables))]
    fn specialize(
        _pipeline: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        // Gate the stains bindings in `splat.wgsl` so they only exist
        // on builds that have the corresponding Rust-side fields.
        #[cfg(not(webgl2))]
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

#[cfg(test)]
mod uniform_layout_tests {
    //! WebGL2 rejects a uniform block whose size is not a multiple of 16
    //! bytes (`DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED` is
    //! unsupported there), and the device-side validator is the only thing
    //! that catches it - so on native everything looks fine and the wasm
    //! deploy fails at pipeline creation with a message that names neither
    //! the struct nor the field that broke it.
    //!
    //! The stains block carries explicit `_pad` fields for that reason, and
    //! the splat block fills its sixteenth scalar before its vectors start.
    //! These tests are what keeps the padding honest when a field is added:
    //! they fail on the machine doing the adding rather than in a browser
    //! later.
    use super::*;
    use bevy::render::render_resource::ShaderType;

    /// `min_size` is what the UBO binding is validated against.
    fn block_size<T: ShaderType>() -> u64 {
        T::min_size().get()
    }

    #[test]
    fn splat_uniforms_block_is_16_byte_aligned() {
        let size = block_size::<SplatUniforms>();
        assert_eq!(
            size % 16,
            0,
            "SplatUniforms is {size} bytes - WebGL2 needs a multiple of 16. \
             Add or adjust a `_pad` field, and mirror it in splat.wgsl."
        );
    }

    #[test]
    fn stains_uniforms_block_is_16_byte_aligned() {
        let size = block_size::<StainsUniforms>();
        assert_eq!(size % 16, 0, "StainsUniforms is {size} bytes");
    }

    /// Ordered `(name, type)` pairs of a `struct <name> { .. }` block in
    /// the shader source, ignoring comments and blank lines.
    fn wgsl_fields(src: &str, struct_name: &str) -> Vec<(String, String)> {
        let head = format!("struct {struct_name} {{");
        let start = src.find(&head).expect("struct not found in shader") + head.len();
        let body = &src[start..start + src[start..].find('}').expect("unterminated struct")];
        body.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//") && l.contains(':'))
            .map(|l| {
                let (name, ty) = l.split_once(':').expect("filtered on ':'");
                (
                    name.trim().to_string(),
                    ty.trim().trim_end_matches(',').to_string(),
                )
            })
            .collect()
    }

    /// The Rust and WGSL declarations of a uniform block are two hand-written
    /// copies of one layout, and nothing in the build compiles the shader -
    /// WGSL is loaded at runtime. So adding a field to one and forgetting the
    /// other produces no error anywhere: the GPU simply reads the block at the
    /// wrong offsets and the terrain renders subtly wrong. Each block's
    /// fields are pinned here in order, with their types, and its size to
    /// what those fields lay out to.
    #[test]
    fn wgsl_uniform_blocks_mirror_the_rust_ones() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/shaders/splat.wgsl"
        ))
        .expect("splat.wgsl is a tracked asset");
        let field = |name: &'static str, ty: &'static str| (name, ty);
        let splat = [
            field("tile_scale", "f32"),
            field("enabled", "u32"),
            field("triplanar_scale", "f32"),
            field("triplanar_sharpness", "f32"),
            field("water_y", "f32"),
            field("moisture_depth", "f32"),
            field("moisture_strength", "f32"),
            field("albedo_fade_near", "f32"),
            field("albedo_fade_far", "f32"),
            field("wetness", "f32"),
            field("snow", "f32"),
            field("snow_line", "f32"),
            field("snow_feather", "f32"),
            field("grass_carpet", "f32"),
            field("grass_dry_share", "f32"),
            field("storm", "f32"),
            field("grass_base", "vec4<f32>"),
            field("grass_tip", "vec4<f32>"),
            field("grass_dry", "vec4<f32>"),
            field("grass_layer_density", "vec4<f32>"),
            field("wind", "vec4<f32>"),
        ];
        let stains = [
            field("world_period", "f32"),
            field("enabled", "u32"),
            field("_pad0", "u32"),
            field("_pad1", "u32"),
        ];
        for (name, expected, size) in [
            ("SplatUniforms", &splat[..], block_size::<SplatUniforms>()),
            (
                "StainsUniforms",
                &stains[..],
                block_size::<StainsUniforms>(),
            ),
        ] {
            let actual = wgsl_fields(&src, name);
            let actual: Vec<(&str, &str)> = actual
                .iter()
                .map(|(n, t)| (n.as_str(), t.as_str()))
                .collect();
            assert_eq!(
                actual, expected,
                "splat.wgsl's {name} drifted from the Rust block"
            );
            // Four-byte scalars first, sixteen of them in the splat block,
            // then the vectors.
            let laid_out: u64 = expected
                .iter()
                .map(|(_, ty)| if ty.starts_with("vec4") { 16 } else { 4 })
                .sum();
            assert_eq!(size, laid_out, "{name}");
        }
    }

    /// The Rust block's bytes lie as the WGSL block reads them: a block
    /// whose every field holds its place in the WGSL order (counting a
    /// vector's components one each) writes, as the GPU is given it, its
    /// four-byte words in that order. Swapping two fields of one type in the
    /// Rust struct - a size the WGSL mirror test cannot tell from right -
    /// puts their numbers in each other's places.
    #[test]
    fn the_rust_block_lays_out_as_the_wgsl_one() {
        use bevy::render::render_resource::encase::UniformBuffer;
        let v = |first: f32| Vec4::new(first, first + 1.0, first + 2.0, first + 3.0);
        let block = SplatUniforms {
            tile_scale: 1.0,
            enabled: 2,
            triplanar_scale: 3.0,
            triplanar_sharpness: 4.0,
            water_y: 5.0,
            moisture_depth: 6.0,
            moisture_strength: 7.0,
            albedo_fade_near: 8.0,
            albedo_fade_far: 9.0,
            wetness: 10.0,
            snow: 11.0,
            snow_line: 12.0,
            snow_feather: 13.0,
            grass_carpet: 14.0,
            grass_dry_share: 15.0,
            storm: 16.0,
            grass_base: v(17.0),
            grass_tip: v(21.0),
            grass_dry: v(25.0),
            grass_layer_density: v(29.0),
            wind: v(33.0),
        };
        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(&block).expect("the block writes");
        let bytes = buffer.into_inner();
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/shaders/splat.wgsl"
        ))
        .expect("splat.wgsl is a tracked asset");
        let mut place = 0u32;
        let mut expected: Vec<u32> = Vec::new();
        for (_, ty) in wgsl_fields(&src, "SplatUniforms") {
            let words = if ty.starts_with("vec4") { 4 } else { 1 };
            for _ in 0..words {
                place += 1;
                expected.push(if ty == "u32" {
                    place
                } else {
                    (place as f32).to_bits()
                });
            }
        }
        let words: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|w| u32::from_le_bytes(w.try_into().expect("four bytes")))
            .collect();
        assert_eq!(words, expected);
    }

    /// The damp-ground effect (#913) must be inert by default, so a room
    /// with no water generator - and every frame before the splat pass
    /// resolves a water line - renders exactly the pre-#913 terrain.
    #[test]
    fn moisture_defaults_to_disabled() {
        let u = SplatUniforms::default();
        assert_eq!(
            u.moisture_strength, 0.0,
            "a zero strength is what makes the tint a no-op"
        );
    }
}
