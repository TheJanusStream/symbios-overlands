//! Four-layer splat texturing: procedural texture tasks, the
//! texture-array atlas, the height/slope weight map, and the material
//! flip from flat placeholder to triplanar splat blending. Also
//! publishes the CPU terrain mirror
//! ([`TerrainSurfaceQuery`]) the contact classifier probes.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_symbios_ground::{SplatMapper, SplatRule, TerrainQuery};
use bevy_symbios_texture::async_gen::{PendingTexture, TextureReady};

use crate::config::terrain as tcfg;
use crate::config::terrain::stains as scfg;
use crate::interaction::{StainsImage, TerrainSurfaceQuery};
use crate::pds::{SovereignTerrainConfig, SovereignTextureConfig};
use crate::splat::SplatTerrainMaterial;
use crate::state::LiveRoomRecord;

use super::referenced::spawn_splat_layer_fetch;
use super::{
    FinishedHeightMap, SplatMaterialHandle, TerrainSplatState, TextureLayerIndex,
    TextureTasksStarted,
};

/// Spawn four `PendingTexture` entities (one per splat layer), pulling the
/// procedural configs from the active `RoomRecord`'s terrain generator.
/// `SymbiosTexturePlugin` polls them every frame and attaches `TextureReady`
/// when done. A `TextureTasksStarted` marker is inserted to make this a
/// one-shot inside the Loading-phase scheduler loop.
///
/// For `SovereignTextureConfig::Referenced` layers, an additional
/// `PendingSplatLayerFetch` task is spawned alongside the procedural
/// placeholder. The placeholder fills the splat array immediately (so
/// the player isn't staring at flat ground while the fetch is in
/// flight); when the fetch lands [`super::referenced::poll_splat_layer_fetches`]
/// overrides the layer's albedo handle and triggers an atlas rebuild.
pub(super) fn start_texture_tasks(mut commands: Commands, record: Res<LiveRoomRecord>) {
    let mat = crate::pds::find_terrain_config(&record.0)
        .map(|c| c.material.clone())
        .unwrap_or_default();

    let texture_size = mat.texture_size.max(16);

    for (i, layer) in mat.layers.iter().enumerate() {
        let pending = pending_texture_from_sovereign(layer, texture_size);
        commands.spawn((pending, TextureLayerIndex(i)));

        // Referenced layers ALSO trigger an HTTP / ATProto-blob fetch.
        // The decoded image overrides the procedural placeholder once
        // bytes arrive — until then the placeholder ground texture is
        // what the splat shader samples.
        if let SovereignTextureConfig::Referenced { source } = layer {
            spawn_splat_layer_fetch(&mut commands, i, source, texture_size);
        }
    }

    commands.insert_resource(TextureTasksStarted);
}

/// Build a [`PendingTexture`] from any [`SovereignTextureConfig`] variant.
/// Unknown / None variants fall back to a default ground config so all four
/// terrain splat layers always produce a baked texture — the splat shader
/// samples all four unconditionally.
fn pending_texture_from_sovereign(layer: &SovereignTextureConfig, size: u32) -> PendingTexture {
    match layer {
        SovereignTextureConfig::Ground(c) => PendingTexture::ground(c.to_native(), size, size),
        SovereignTextureConfig::Rock(c) => PendingTexture::rock(c.to_native(), size, size),
        SovereignTextureConfig::Bark(c) => PendingTexture::bark(c.to_native(), size, size),
        SovereignTextureConfig::Brick(c) => PendingTexture::brick(c.to_native(), size, size),
        SovereignTextureConfig::Plank(c) => PendingTexture::plank(c.to_native(), size, size),
        SovereignTextureConfig::Shingle(c) => PendingTexture::shingle(c.to_native(), size, size),
        SovereignTextureConfig::Stucco(c) => PendingTexture::stucco(c.to_native(), size, size),
        SovereignTextureConfig::Concrete(c) => PendingTexture::concrete(c.to_native(), size, size),
        SovereignTextureConfig::Metal(c) => PendingTexture::metal(c.to_native(), size, size),
        SovereignTextureConfig::Pavers(c) => PendingTexture::pavers(c.to_native(), size, size),
        SovereignTextureConfig::Ashlar(c) => PendingTexture::ashlar(c.to_native(), size, size),
        SovereignTextureConfig::Cobblestone(c) => {
            PendingTexture::cobblestone(c.to_native(), size, size)
        }
        SovereignTextureConfig::Thatch(c) => PendingTexture::thatch(c.to_native(), size, size),
        SovereignTextureConfig::Marble(c) => PendingTexture::marble(c.to_native(), size, size),
        SovereignTextureConfig::Corrugated(c) => {
            PendingTexture::corrugated(c.to_native(), size, size)
        }
        SovereignTextureConfig::Asphalt(c) => PendingTexture::asphalt(c.to_native(), size, size),
        SovereignTextureConfig::Wainscoting(c) => {
            PendingTexture::wainscoting(c.to_native(), size, size)
        }
        SovereignTextureConfig::Encaustic(c) => {
            PendingTexture::encaustic(c.to_native(), size, size)
        }
        // Additional tileable surfaces — usable as biome splat layers (sand
        // for desert, snow for tundra, lava for volcanic crust).
        SovereignTextureConfig::Fabric(c) => PendingTexture::fabric(c.to_native(), size, size),
        SovereignTextureConfig::Sand(c) => PendingTexture::sand(c.to_native(), size, size),
        SovereignTextureConfig::Snow(c) => PendingTexture::snow(c.to_native(), size, size),
        SovereignTextureConfig::Ice(c) => PendingTexture::ice(c.to_native(), size, size),
        SovereignTextureConfig::Lava(c) => PendingTexture::lava(c.to_native(), size, size),
        SovereignTextureConfig::Leaf(c) => PendingTexture::leaf(c.to_native(), size, size),
        SovereignTextureConfig::Twig(c) => PendingTexture::twig(c.to_native(), size, size),
        SovereignTextureConfig::Window(c) => PendingTexture::window(c.to_native(), size, size),
        SovereignTextureConfig::StainedGlass(c) => {
            PendingTexture::stained_glass(c.to_native(), size, size)
        }
        SovereignTextureConfig::IronGrille(c) => {
            PendingTexture::iron_grille(c.to_native(), size, size)
        }
        SovereignTextureConfig::ChainLink(c) => {
            PendingTexture::chain_link(c.to_native(), size, size)
        }
        SovereignTextureConfig::LogEnd(c) => PendingTexture::log_end(c.to_native(), size, size),
        // None / Unknown / Referenced — fall back to an opaque placeholder
        // via GroundConfig default so the splat array always has four live
        // textures to sample. For Referenced the fallback is what the splat
        // shows BEFORE the resolver paints the fetched image in; once
        // BlobImageCache delivers the handle the layer's StandardMaterial
        // base_color_texture is swapped over the placeholder.
        //
        // The particle sprite cards share the SovereignTextureConfig dropdown
        // but are alpha-silhouette billboards, not tileable surfaces — tiling
        // one across terrain would repeat its transparent holes. They fall
        // back to the ground placeholder here; they're meant for the particle
        // texture slot, not terrain layers.
        SovereignTextureConfig::None
        | SovereignTextureConfig::Unknown
        | SovereignTextureConfig::Referenced { .. }
        | SovereignTextureConfig::SoftDisc(_)
        | SovereignTextureConfig::Spark(_)
        | SovereignTextureConfig::Snowflake(_)
        | SovereignTextureConfig::Puff(_)
        | SovereignTextureConfig::Ring(_)
        | SovereignTextureConfig::Petal(_)
        | SovereignTextureConfig::Shard(_)
        | SovereignTextureConfig::LeafSprite(_)
        | SovereignTextureConfig::Flame(_)
        | SovereignTextureConfig::Flower(_) => PendingTexture::ground(
            crate::pds::SovereignGroundConfig::default().to_native(),
            size,
            size,
        ),
    }
}

/// Consume `TextureReady` components attached by `SymbiosTexturePlugin` and
/// store the GPU handles by layer index.
pub(super) fn collect_texture_results(
    mut commands: Commands,
    ready: Query<(Entity, &TextureLayerIndex, &TextureReady)>,
    mut state: ResMut<TerrainSplatState>,
) {
    for (entity, idx, ready) in &ready {
        state.layer_albedo[idx.0] = Some(ready.0.albedo.clone());
        state.layer_normal[idx.0] = Some(ready.0.normal.clone());
        commands.entity(entity).despawn();
    }
}

/// Once all four layers are ready, build the texture arrays, generate the
/// weight map, and enable splat blending on the terrain material.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_splat_textures(
    mut commands: Commands,
    mut state: ResMut<TerrainSplatState>,
    hm_res: Option<Res<FinishedHeightMap>>,
    splat_mat: Option<Res<SplatMaterialHandle>>,
    record: Option<Res<LiveRoomRecord>>,
    stains: Option<Res<StainsImage>>,
    mut materials: ResMut<Assets<SplatTerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if state.applied || !state.all_ready() {
        return;
    }
    let (Some(hm_res), Some(splat_mat)) = (hm_res, splat_mat) else {
        return;
    };

    // Collect layer pixel data into flat buffers (immutable phase).
    let Some(albedo_img) = build_texture_array(&state.layer_albedo, &images) else {
        return;
    };
    let Some(normal_img) = build_texture_array(&state.layer_normal, &images) else {
        return;
    };

    // Mutable phase — add new assets after all immutable reads are done.
    let albedo_array = images.add(albedo_img);
    let normal_array = images.add(normal_img);

    // Generate the RGBA weight map from the heightmap (one texel per cell).
    let hm = &hm_res.0;
    let world_extent = (hm.width() - 1) as f32 * hm.scale();

    // Pull splat rules from the active record when present — this is what
    // lets the world editor re-balance biomes without a recompile. Falls
    // back to the canonical defaults if the record lacks a terrain gen.
    let (rules_src, hs) = record
        .as_ref()
        .and_then(|r| {
            crate::pds::find_terrain_config(&r.0).map(|c| (c.material.rules, c.height_scale.0))
        })
        .unwrap_or_else(|| {
            (
                SovereignTerrainConfig::default().material.rules,
                tcfg::HEIGHT_SCALE,
            )
        });

    let mapper = SplatMapper::new([
        // R — Grass
        SplatRule::new(
            (
                hs * rules_src[0].height_min.0,
                hs * rules_src[0].height_max.0,
            ),
            (rules_src[0].slope_min.0, rules_src[0].slope_max.0),
            rules_src[0].sharpness.0,
        ),
        // G — Dirt
        SplatRule::new(
            (
                hs * rules_src[1].height_min.0,
                hs * rules_src[1].height_max.0,
            ),
            (rules_src[1].slope_min.0, rules_src[1].slope_max.0),
            rules_src[1].sharpness.0,
        ),
        // B — Rock
        SplatRule::new(
            (
                hs * rules_src[2].height_min.0,
                hs * rules_src[2].height_max.0,
            ),
            (rules_src[2].slope_min.0, rules_src[2].slope_max.0),
            rules_src[2].sharpness.0,
        ),
        // A — Snow
        SplatRule::new(
            (
                hs * rules_src[3].height_min.0,
                hs * rules_src[3].height_max.0,
            ),
            (rules_src[3].slope_min.0, rules_src[3].slope_max.0),
            rules_src[3].sharpness.0,
        ),
    ]);
    let weight_map = mapper.generate(hm);

    // CPU mirror for the avatar-world interaction classifier (#245):
    // the same heightmap + splat rules the GPU sees, queryable for
    // ground height / normal / splat weights at any world XZ. The
    // heightfield collider *is* this heightmap, so this is the terrain
    // analogue of `WaterSurfaces` (a CPU analytic query, not a physics
    // raycast). `mapper` is moved in (unused after `generate`); the
    // heightmap is deep-cloned once per terrain build (~1 MiB at the
    // default 512 grid). Overwrites any prior query on regenerate.
    commands.insert_resource(TerrainSurfaceQuery::new(
        TerrainQuery::new(hm.clone(), mapper),
        world_extent * 0.5,
    ));

    // Build the weight-map image manually so we can use RENDER_WORLD-only
    // storage — the CPU bytes are never needed again after upload.
    let wm_bytes: Vec<u8> = weight_map
        .data
        .iter()
        .flat_map(|pixel| pixel.iter().copied())
        .collect();
    let mut wm_image = Image::new(
        Extent3d {
            width: weight_map.width as u32,
            height: weight_map.height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        wm_bytes,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    wm_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    let wm_handle = images.add(wm_image);

    if let Some(mat) = materials.get_mut(&splat_mat.0) {
        mat.base.base_color = Color::WHITE;
        mat.base.perceptual_roughness = tcfg::splat::MATERIAL_ROUGHNESS;
        mat.base.metallic = tcfg::splat::MATERIAL_METALLIC;
        mat.extension.weight_map = wm_handle;
        mat.extension.albedo_array = albedo_array;
        mat.extension.normal_array = normal_array;
        mat.extension.uniforms.enabled = 1;
        mat.extension.uniforms.triplanar_scale = tcfg::TILE_SCALE / world_extent.max(1.0);

        // Bind the avatar-interaction stains overlay (#245). The image
        // is allocated zeroed at startup, so enabling it now is inert
        // until the stamper writes contacts — backward-compatible.
        //
        // wasm32 (WebGL2) skips this binding entirely — see the note on
        // `SplatExtension::stains_tex` in `crate::splat`: the GLES backend
        // caps fragment shaders at 16 texture slots and the splat material
        // already sits at that ceiling, so the stains overlay is gated off
        // and the consumer assignments below are unreachable on wasm.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(stains) = stains.as_ref() {
            mat.extension.stains_tex = stains.handle.clone();
            mat.extension.stains.world_period = scfg::WORLD_PERIOD;
            mat.extension.stains.enabled = 1;
        }
        #[cfg(target_arch = "wasm32")]
        let _ = stains;
    }

    state.applied = true;
}

/// Concatenate the four layer images into a single `texture_2d_array` `Image`.
///
/// Returns `None` if any handle is missing or the image data is not yet
/// resident (will be retried next frame by `apply_splat_textures`).
fn build_texture_array(
    handles: &[Option<Handle<Image>>; 4],
    images: &Assets<Image>,
) -> Option<Image> {
    let first = images.get(handles[0].as_ref()?.id())?;
    let w = first.texture_descriptor.size.width;
    let h = first.texture_descriptor.size.height;
    let format = first.texture_descriptor.format;
    let mip_count = first.texture_descriptor.mip_level_count;
    let bytes_per_layer = first.data.as_ref()?.len();

    // Concatenate every layer's full mipchain in order.
    let mut merged: Vec<u8> = Vec::with_capacity(bytes_per_layer * 4);
    for handle_opt in handles {
        let img = images.get(handle_opt.as_ref()?.id())?;
        merged.extend_from_slice(img.data.as_ref()?);
    }

    // Each procedural layer image carries a full mipchain (see
    // `bevy_symbios_texture`'s `generate_mipmaps`), so `merged` is
    // mip-inclusive. `Image::new` `debug_assert`s `merged.len()` against
    // the descriptor's *default* `mip_level_count = 1` and panics — the
    // post-hoc `mip_level_count` assignment used to land too late for
    // that assert (and in release the check is compiled out, leaving a
    // descriptor that mismatches the data). Build via `new_uninit`,
    // which performs no length check, then set the real mip count and
    // attach the data. The per-layer concatenation above is layer-major,
    // matching the default `TextureDataOrder::LayerMajor` the uninit
    // descriptor carries.
    let mut array_img = Image::new_uninit(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 4,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    array_img.texture_descriptor.mip_level_count = mip_count;
    array_img.data = Some(merged);
    array_img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    });

    Some(array_img)
}
