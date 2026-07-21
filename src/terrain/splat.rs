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
use bevy_symbios_texture::{TextureMap, map_to_images_with_usages};

use crate::config::terrain as tcfg;
#[cfg(not(target_arch = "wasm32"))]
use crate::config::terrain::stains as scfg;
use crate::interaction::{StainsImage, TerrainSurfaceQuery};
use crate::offload::{GenJob, GenResult};
use crate::pds::{SovereignTerrainConfig, SovereignTextureConfig};
use crate::splat::SplatTerrainMaterial;
use crate::state::LiveRoomRecord;

use super::referenced::spawn_splat_layer_fetch;
use super::{
    FinishedHeightMap, SplatMaterialHandle, TerrainSplatState, TextureLayerIndex,
    TextureTasksStarted,
};

/// Dispatch four procedural splat-layer texture bakes (one per layer), pulling
/// the configs from the active `RoomRecord`'s terrain generator and routing each
/// through [`crate::offload`] so the heavy pattern synthesis runs off the
/// schedule (native: `AsyncComputeTaskPool`; wasm: a Web Worker — the texture
/// crate's own rayon pool is a no-op on wasm and would run inline on the render
/// frame, which is the gap this closes). Each bake is parked on a
/// [`SplatTexTask`]; a `TextureTasksStarted` marker makes this a one-shot inside
/// the Loading-phase scheduler loop.
///
/// For `SovereignTextureConfig::Referenced` layers, an additional
/// `PendingSplatLayerFetch` task is spawned alongside the procedural
/// placeholder. The placeholder fills the splat array immediately (so
/// the player isn't staring at flat ground while the fetch is in
/// flight); when the fetch lands [`super::referenced::poll_splat_layer_fetches`]
/// overrides the layer's albedo handle and triggers an atlas rebuild.
pub(super) fn start_texture_tasks(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    let mat = crate::pds::find_terrain_config(&record.0)
        .map(|c| c.material.clone())
        .unwrap_or_default();

    let texture_size = mat.texture_size.max(16);
    let spawned_at = time.elapsed_secs_f64();

    for (i, layer) in mat.layers.iter().enumerate() {
        let task = crate::offload::offload(GenJob::TextureBake {
            job: texture_bake_job(layer),
            width: texture_size,
            height: texture_size,
        });
        // Offload-lifecycle mark (#631). Each of the four concurrent bakes gets
        // a DISTINCT job name — a shared name would let a fast layer's
        // completion satisfy a stalled sibling's `OffloadJobStarted` and mask a
        // real stall. Gated on `not(TextureTasksStarted)`, so once per gen.
        session_log.info(
            spawned_at,
            crate::diagnostics::event::EventPayload::OffloadJobStarted {
                job: format!("texture_layer_{i}"),
            },
        );
        commands.spawn((
            SplatTexTask {
                index: i,
                task,
                spawned_at,
            },
            TextureLayerIndex,
        ));

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

/// In-flight procedural bake of one splat layer's texture, dispatched through
/// [`crate::offload`]. `index` is the layer slot (0–3) the resolved images are
/// stored into; the entity also carries a [`TextureLayerIndex`] marker that the
/// terrain lifecycle systems use to sweep pending bakes on logout / regen.
#[derive(Component)]
pub(super) struct SplatTexTask {
    index: usize,
    task: bevy::tasks::Task<GenResult>,
    /// Session-relative seconds at dispatch, for the E-4 completion latency.
    spawned_at: f64,
}

/// Build a [`gen_jobs::TextureBakeJob`] from any [`SovereignTextureConfig`]
/// variant — the offload-layer mirror of the procedural splat config. Unknown /
/// None / Referenced / particle-sprite variants fall back to a default ground
/// config so all four splat layers always bake a tileable surface (the splat
/// shader samples all four unconditionally).
fn texture_bake_job(layer: &SovereignTextureConfig) -> gen_jobs::TextureBakeJob {
    use gen_jobs::TextureBakeJob as Job;
    match layer {
        SovereignTextureConfig::Ground(c) => Job::Ground(c.to_native()),
        SovereignTextureConfig::Rock(c) => Job::Rock(c.to_native()),
        SovereignTextureConfig::Bark(c) => Job::Bark(c.to_native()),
        SovereignTextureConfig::Brick(c) => Job::Brick(c.to_native()),
        SovereignTextureConfig::Plank(c) => Job::Plank(c.to_native()),
        SovereignTextureConfig::Shingle(c) => Job::Shingle(c.to_native()),
        SovereignTextureConfig::Stucco(c) => Job::Stucco(c.to_native()),
        SovereignTextureConfig::Concrete(c) => Job::Concrete(c.to_native()),
        SovereignTextureConfig::Metal(c) => Job::Metal(c.to_native()),
        SovereignTextureConfig::Pavers(c) => Job::Pavers(c.to_native()),
        SovereignTextureConfig::Ashlar(c) => Job::Ashlar(c.to_native()),
        SovereignTextureConfig::Cobblestone(c) => Job::Cobblestone(c.to_native()),
        SovereignTextureConfig::Thatch(c) => Job::Thatch(c.to_native()),
        SovereignTextureConfig::Marble(c) => Job::Marble(c.to_native()),
        SovereignTextureConfig::Corrugated(c) => Job::Corrugated(c.to_native()),
        SovereignTextureConfig::Asphalt(c) => Job::Asphalt(c.to_native()),
        SovereignTextureConfig::Wainscoting(c) => Job::Wainscoting(c.to_native()),
        SovereignTextureConfig::Encaustic(c) => Job::Encaustic(c.to_native()),
        // Additional tileable surfaces — usable as biome splat layers (sand
        // for desert, snow for tundra, lava for volcanic crust).
        SovereignTextureConfig::Fabric(c) => Job::Fabric(c.to_native()),
        SovereignTextureConfig::Sand(c) => Job::Sand(c.to_native()),
        SovereignTextureConfig::Snow(c) => Job::Snow(c.to_native()),
        SovereignTextureConfig::Ice(c) => Job::Ice(c.to_native()),
        SovereignTextureConfig::Lava(c) => Job::Lava(c.to_native()),
        // A tileable succulent skin — usable as a splat layer like any surface.
        SovereignTextureConfig::CactusSkin(c) => Job::CactusSkin(c.to_native()),
        SovereignTextureConfig::Leaf(c) => Job::Leaf(c.to_native()),
        SovereignTextureConfig::Twig(c) => Job::Twig(c.to_native()),
        SovereignTextureConfig::Window(c) => Job::Window(c.to_native()),
        SovereignTextureConfig::StainedGlass(c) => Job::StainedGlass(c.to_native()),
        SovereignTextureConfig::IronGrille(c) => Job::IronGrille(c.to_native()),
        SovereignTextureConfig::ChainLink(c) => Job::ChainLink(c.to_native()),
        SovereignTextureConfig::LogEnd(c) => Job::LogEnd(c.to_native()),
        // None / Unknown / Referenced — fall back to an opaque placeholder
        // via GroundConfig default so the splat array always has four live
        // textures to sample. For Referenced the fallback is what the splat
        // shows BEFORE the resolver paints the fetched image in; once the
        // fetch lands the layer's albedo handle is swapped over the placeholder.
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
        | SovereignTextureConfig::Flower(_)
        | SovereignTextureConfig::GrassTuft(_)
        | SovereignTextureConfig::Frond(_) => {
            Job::Ground(crate::pds::SovereignGroundConfig::default().to_native())
        }
    }
}

/// Poll the in-flight [`SplatTexTask`] bakes; when one resolves, rebuild its
/// per-layer albedo + normal `Image`s from the returned [`GenResult::Texture`]
/// pixel buffers and store them by layer index.
///
/// The bake ran the procedural generator off the schedule (native:
/// `AsyncComputeTaskPool`; wasm: a Web Worker — see [`crate::offload`]),
/// returning RGBA buffers mip-chained inside the job.
/// [`map_to_images_with_usages`] stores them `MAIN_WORLD`-only (no GPU upload
/// — these per-layer images are only read back on the CPU by
/// [`build_texture_array`], never bound), so it can stack them unchanged
/// before [`apply_splat_textures`] drops them.
pub(super) fn collect_texture_results(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut SplatTexTask)>,
    mut state: ResMut<TerrainSplatState>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    for (entity, mut pending) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut pending.task))
        else {
            continue;
        };
        let now = time.elapsed_secs_f64();
        let spawned_at = pending.spawned_at;
        commands.entity(entity).despawn();

        // A texture-bake job only ever yields a texture; count an unexpected
        // variant as an offload error (E-4) and skip the layer rather than panic.
        let GenResult::Texture(data) = result else {
            crate::diagnostics::samplers::offload_job_error(&mut metrics);
            // Pairs with this layer's `OffloadJobStarted` (#631).
            session_log.error(
                now,
                crate::diagnostics::event::EventPayload::OffloadJobFailed {
                    job: format!("texture_layer_{}", pending.index),
                    reason: "offload job yielded a non-texture result".into(),
                },
            );
            warn!(
                "texture-bake offload job yielded an unexpected result — skipping layer {}",
                pending.index
            );
            continue;
        };
        // Success only: record the bake latency (E-4) — a skipped/failed layer
        // above never reaches here, so it can't pollute the latency histogram.
        crate::diagnostics::samplers::texture_bake_latency_secs(&mut metrics, now - spawned_at);
        session_log.info(
            now,
            crate::diagnostics::event::EventPayload::OffloadJobCompleted {
                job: format!("texture_layer_{}", pending.index),
                duration_secs: now - spawned_at,
            },
        );

        let map = TextureMap {
            albedo: data.albedo,
            normal: data.normal,
            roughness: data.roughness,
            emissive: data.emissive,
            // The worker mip-chains inside the job (gen-jobs runs
            // `TextureMap::with_mips`), so the count must ride along — the
            // upload treats `1` as "base only" and would box-filter a
            // wrong-length buffer otherwise. Payloads from an older worker
            // deserialise the field to `1` and still mip-chain here.
            mip_level_count: data.mip_level_count,
            width: data.width,
            height: data.height,
        };
        // `MAIN_WORLD`-only: these per-layer images are never bound to a
        // material — `build_texture_array` reads their CPU bytes to assemble
        // the two splat arrays — so skip the GPU upload entirely and keep
        // `Image::data` resident for that read. `apply_splat_textures` drops
        // them once the arrays are built.
        let handles = map_to_images_with_usages(map, RenderAssetUsages::MAIN_WORLD, &mut images);
        state.layer_albedo[pending.index] = Some(handles.albedo);
        state.layer_normal[pending.index] = Some(handles.normal);
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

    // The four per-layer source images have now been concatenated into the two
    // RENDER_WORLD splat arrays bound above, so their bytes are fully redundant.
    // Drop the handles to free their MAIN_WORLD CPU copies (~11 MiB at the 512
    // splat resolution) — except when a Referenced layer is present, since a
    // late blob fetch can re-trigger this system, which needs all four sources
    // to rebuild the arrays.
    let has_referenced = record
        .as_ref()
        .and_then(|r| crate::pds::find_terrain_config(&r.0))
        .is_some_and(|c| {
            c.material
                .layers
                .iter()
                .any(|l| matches!(l, SovereignTextureConfig::Referenced { .. }))
        });
    if !has_referenced {
        state.layer_albedo = Default::default();
        state.layer_normal = Default::default();
    }

    state.applied = true;
}

/// Free the four per-layer source images of a *Referenced* room once every layer
/// fetch has resolved (#642). `apply_splat_textures` drops them the same frame
/// for procedural rooms, but deliberately skips the free while any Referenced
/// layer is configured — a late blob fetch flips `state.applied = false` and
/// needs all four sources to rebuild the arrays. Once no `PendingSplatLayerFetch`
/// entity remains, no rebuild can be re-triggered (the sole trigger is a
/// resolving fetch), so the retained ~11 MiB of MAIN_WORLD CPU bytes is pure
/// dead weight and is released here. Self-guarded (a no-op for procedural rooms,
/// whose slots are already `None`) and idempotent.
pub(super) fn free_referenced_splat_sources(
    mut state: ResMut<TerrainSplatState>,
    pending: Query<(), With<super::referenced::PendingSplatLayerFetch>>,
) {
    if state.applied && pending.is_empty() && state.layer_albedo.iter().any(Option::is_some) {
        state.layer_albedo = Default::default();
        state.layer_normal = Default::default();
    }
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
