//! Four-layer splat texturing: procedural texture tasks, the
//! texture-array atlas, the height/slope weight map, and the material
//! flip from flat placeholder to triplanar splat blending. Also
//! publishes the CPU terrain mirror
//! ([`TerrainSurfaceQuery`]) the contact classifier probes.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension};
use bevy_symbios_ground::{SplatMapper, SplatRule, TerrainQuery};
use bevy_symbios_texture::{TextureMap, map_to_images_with_usages};

use crate::config::terrain as tcfg;
use crate::config::terrain::stains as scfg;
use crate::interaction::{StainsImage, TerrainSurfaceQuery};
use crate::offload::{GenJob, GenResult};
use crate::pds::{SovereignTerrainConfig, SovereignTextureConfig};
use crate::splat::SplatTerrainMaterial;
use crate::state::LiveRoomRecord;
use crate::weather::WeatherNow;

use super::referenced::{ReferencedLayerStatus, spawn_splat_layer_fetch};
use super::{
    FinishedHeightMap, SplatMaterialHandle, TerrainSplatState, TextureLayerIndex,
    TextureTasksStarted,
};

/// Dispatch four procedural splat-layer texture bakes (one per layer), pulling
/// the configs from the active `RoomRecord`'s terrain generator and routing each
/// through [`crate::offload`] so the heavy pattern synthesis runs off the
/// schedule (native: `AsyncComputeTaskPool`; wasm: a Web Worker - the texture
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
    mut layer_status: ResMut<ReferencedLayerStatus>,
    mut splat_state: ResMut<TerrainSplatState>,
    settings: Res<crate::state::LocalSettings>,
) {
    let mat = crate::pds::find_terrain_config(&record.0)
        .map(|c| c.material.clone())
        .unwrap_or_default();

    let texture_size = mat.texture_size.max(16);
    let spawned_at = time.elapsed_secs_f64();

    for (i, layer) in mat.layers.iter().enumerate() {
        let task = crate::offload::offload(GenJob::TextureBake {
            job: Box::new(texture_bake_job(layer)),
            width: texture_size,
            height: texture_size,
        });
        // Offload-lifecycle mark (#631). Each of the four concurrent bakes gets
        // a DISTINCT job name - a shared name would let a fast layer's
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
        // bytes arrive - until then the placeholder ground texture is
        // what the splat shader samples.
        if let SovereignTextureConfig::Referenced { source } = layer {
            // Not unconditional any more (#1246 f347 / #1247 f346): a
            // layer already resolved at this size reuses its handles, one
            // still inside its failure backoff issues nothing, and only a
            // genuinely new or due reference reaches the network.
            spawn_splat_layer_fetch(
                &mut commands,
                &mut layer_status,
                &mut splat_state,
                i,
                source,
                texture_size,
                spawned_at,
                settings.load_external_assets,
            );
        }
    }

    commands.insert_resource(TextureTasksStarted);
}

/// In-flight procedural bake of one splat layer's texture, dispatched through
/// [`crate::offload`]. `index` is the layer slot (0-3) the resolved images are
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
/// variant - the offload-layer mirror of the procedural splat config. Unknown /
/// None / Referenced / particle-sprite variants fall back to a default ground
/// config so all four splat layers always bake a tileable surface (the splat
/// shader samples all four unconditionally).
fn texture_bake_job(layer: &SovereignTextureConfig) -> gen_jobs::TextureBakeJob {
    use gen_jobs::TextureBakeJob as Job;
    match layer {
        SovereignTextureConfig::Ground(c) => Job::Ground(c.to_native()),
        SovereignTextureConfig::CrackedEarth(c) => Job::CrackedEarth(c.to_native()),
        SovereignTextureConfig::Gravel(c) => Job::Gravel(c.to_native()),
        SovereignTextureConfig::ForestFloor(c) => Job::ForestFloor(c.to_native()),
        SovereignTextureConfig::Enamel(c) => Job::Enamel(c.to_native()),
        SovereignTextureConfig::Obsidian(c) => Job::Obsidian(c.to_native()),
        SovereignTextureConfig::Chitin(c) => Job::Chitin(c.to_native()),
        SovereignTextureConfig::SolarPanel(c) => Job::SolarPanel(c.to_native()),
        SovereignTextureConfig::Parquet(c) => Job::Parquet(c.to_native()),
        SovereignTextureConfig::Truchet(c) => Job::Truchet(c.to_native()),
        // The symbios-texture 0.8 surfaces (#1574): tileable like any - a
        // dry-stone field as a ground layer, say.
        SovereignTextureConfig::RoofTile(c) => Job::RoofTile(c.to_native()),
        SovereignTextureConfig::LogWall(c) => Job::LogWall(c.to_native()),
        SovereignTextureConfig::DryStone(c) => Job::DryStone(c.to_native()),
        SovereignTextureConfig::Fur(c) => Job::Fur(c.to_native()),
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
        // Additional tileable surfaces - usable as biome splat layers (sand
        // for desert, snow for tundra, lava for volcanic crust).
        SovereignTextureConfig::Fabric(c) => Job::Fabric(c.to_native()),
        SovereignTextureConfig::Sand(c) => Job::Sand(c.to_native()),
        SovereignTextureConfig::Snow(c) => Job::Snow(c.to_native()),
        SovereignTextureConfig::Ice(c) => Job::Ice(c.to_native()),
        SovereignTextureConfig::Lava(c) => Job::Lava(c.to_native()),
        // A tileable succulent skin - usable as a splat layer like any surface.
        SovereignTextureConfig::CactusSkin(c) => Job::CactusSkin(c.to_native()),
        // Ground-cover encrustations - tileable, and the point of them is to
        // be a terrain layer (Tundra lichen, forest-floor moss).
        SovereignTextureConfig::Moss(c) => Job::Moss(c.to_native()),
        SovereignTextureConfig::Lichen(c) => Job::Lichen(c.to_native()),
        SovereignTextureConfig::Leaf(c) => Job::Leaf(c.to_native()),
        SovereignTextureConfig::Twig(c) => Job::Twig(c.to_native()),
        SovereignTextureConfig::Window(c) => Job::Window(c.to_native()),
        SovereignTextureConfig::StainedGlass(c) => Job::StainedGlass(c.to_native()),
        SovereignTextureConfig::IronGrille(c) => Job::IronGrille(c.to_native()),
        SovereignTextureConfig::ChainLink(c) => Job::ChainLink(c.to_native()),
        SovereignTextureConfig::LogEnd(c) => Job::LogEnd(c.to_native()),
        // None / Unknown / Referenced - fall back to an opaque placeholder
        // via GroundConfig default so the splat array always has four live
        // textures to sample. For Referenced the fallback is what the splat
        // shows BEFORE the resolver paints the fetched image in; once the
        // fetch lands the layer's albedo handle is swapped over the placeholder.
        //
        // The particle sprite cards share the SovereignTextureConfig dropdown
        // but are alpha-silhouette billboards, not tileable surfaces - tiling
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
        | SovereignTextureConfig::Frond(_)
        | SovereignTextureConfig::Reed(_)
        | SovereignTextureConfig::Needle(_)
        | SovereignTextureConfig::Broadleaf(_) => {
            Job::Ground(crate::pds::SovereignGroundConfig::default().to_native())
        }
    }
}

/// Poll the in-flight [`SplatTexTask`] bakes; when one resolves, rebuild its
/// per-layer albedo + normal `Image`s from the returned [`GenResult::Texture`]
/// pixel buffers and store them by layer index.
///
/// The bake ran the procedural generator off the schedule (native:
/// `AsyncComputeTaskPool`; wasm: a Web Worker - see [`crate::offload`]),
/// returning RGBA buffers mip-chained inside the job.
/// [`map_to_images_with_usages`] stores them `MAIN_WORLD`-only (no GPU upload -
/// these per-layer images are only read back on the CPU by
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
                "texture-bake offload job yielded an unexpected result - skipping layer {}",
                pending.index
            );
            continue;
        };
        // Success only: record the bake latency (E-4) - a skipped/failed layer
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
            // `TextureMap::with_mips`), so the count must ride along - the
            // upload treats `1` as "base only" and would box-filter a
            // wrong-length buffer otherwise. Payloads from an older worker
            // deserialise the field to `1` and still mip-chain here.
            mip_level_count: data.mip_level_count,
            width: data.width,
            height: data.height,
        };
        // `MAIN_WORLD`-only: these per-layer images are never bound to a
        // material - `build_texture_array` reads their CPU bytes to assemble
        // the two splat arrays - so skip the GPU upload entirely and keep
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
    // `Option` for the same reason as in `poll_terrain_task`: headless
    // embedders build terrain without the network plugin that owns it.
    mut digest: Option<ResMut<crate::world_digest::WorldDigest>>,
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

    // Mutable phase - add new assets after all immutable reads are done.
    let albedo_array = images.add(albedo_img);
    let normal_array = images.add(normal_img);

    // Generate the RGBA weight map (one texel per cell): from the record's
    // height and slope rules, or - for ground built from real Berlin - from
    // its land use (#1586), each class on the layer whose role it shares.
    let hm = &hm_res.0;
    let world_extent = (hm.width() - 1) as f32 * hm.scale();

    let mapper = record_splat_mapper(record.as_ref().map(|r| &r.0));
    let mut weight_map = match hm_res.ground() {
        Some(ground) => ground.weight_map(),
        None => mapper.generate(hm),
    };
    // A world's paths, painted in (#1779): before anything reads the weights.
    let painted = match record
        .as_ref()
        .and_then(|r| crate::pds::find_terrain_config(&r.0))
    {
        Some(cfg) if !cfg.paths.is_empty() => {
            super::paths::paint(
                &mut weight_map.data,
                weight_map.width,
                hm.scale(),
                &cfg.paths,
            );
            true
        }
        _ => false,
    };

    // Build the weight-map image manually so we can use RENDER_WORLD-only
    // storage. The weights' bytes are kept once more on the CPU, in the
    // `SplatWeights` the ground cover writes the shelter beside (#1685).
    let wm_bytes: Vec<u8> = weight_map
        .data
        .iter()
        .flat_map(|pixel| pixel.iter().copied())
        .collect();

    // The splat half of the world digest (#1146). These bytes are where a
    // one-ULP arithmetic difference between two peers stops being invisible:
    // `SplatRule` scoring runs on `powf`, and the result is quantised to a
    // `u8` per channel - so a borderline score that rounds the other way is a
    // texel of ground the two peers give a different material. Hashed as-is,
    // with no further quantisation, because these bytes ARE the decision.
    if let Some(digest) = digest.as_deref_mut() {
        if let Some(record) = record.as_ref() {
            digest.retarget(crate::world_digest::record_fingerprint(&record.0));
        }
        digest.splat = Some(crate::world_digest::splat_digest(
            weight_map.width as u32,
            weight_map.height as u32,
            &wm_bytes,
        ));
    }
    // Before the bytes go to the GPU for good: their most, grass tile by
    // grass tile, which the grass bounds what can grow in a tile by (#1693).
    let maxima = std::sync::Arc::new(crate::ground_cover::WeightMaxima::of(
        &weight_map.data,
        weight_map.width,
        weight_map.height,
        world_extent,
    ));
    // Kept on the CPU this once: the ground cover writes the shelter beside
    // these weights whenever it redraws it (#1685).
    let layer = std::sync::Arc::new(crate::ground_cover::WeightLayer {
        width: weight_map.width as u32,
        height: weight_map.height as u32,
        bytes: wm_bytes,
    });

    // CPU mirror for the avatar-world interaction classifier (#245):
    // the same heightmap + splat weights the GPU sees, queryable for
    // ground height / normal / splat weights at any world XZ - the painted
    // weights themselves where the world has paths (#1779), which the
    // rules alone would read as grass. The heightfield collider *is* this
    // heightmap, so this is the terrain analogue of `WaterSurfaces` (a CPU
    // analytic query, not a physics raycast). `mapper` is moved in (unused
    // after `generate`); the heightmap is deep-cloned once per terrain
    // build (~1 MiB at the default 512 grid), and Berlin's land use with it
    // (a quarter of that). Overwrites any prior query on regenerate.
    commands.insert_resource(
        TerrainSurfaceQuery::new(TerrainQuery::new(hm.clone(), mapper), world_extent * 0.5)
            .with_ground(hm_res.ground().cloned())
            .with_painted(painted.then(|| layer.clone())),
    );
    // The room's water as drawn - its sea and its lakes (#1760), or Berlin's
    // own bodies in a geodata region (#1724) - and the bodies beside the
    // weights from the first textured frame (#1727), which the ground cover
    // keeps after.
    let water = record
        .as_ref()
        .map(|r| crate::world_builder::compile::drawn_water(&r.0, Some(&hm_res)))
        .unwrap_or_default();
    let lakes = crate::ground_cover::lake_layer(hm, &water);
    let wm_handle = images.add(crate::ground_cover::weight_image(
        &layer,
        None,
        lakes.as_deref(),
    ));
    // Whatever grows on the ground reads the same weights (#1674).
    commands.insert_resource(crate::ground_cover::SplatWeights(
        wm_handle.clone(),
        maxima,
        layer,
    ));

    // Damp ground near the water line (#913, WS5 step 3). Read from the
    // same drawn water the scatter sampler's riparian band uses, so the
    // darkened margin and the reeds standing in it cannot drift apart. A room
    // with no water generator has no waterline to be damp around, so the
    // effect stays off and the terrain renders exactly as it did before this
    // landed.
    // The record's texture tiling (#1669: the Tile scale slider wrote it for
    // months while the shader read the constant), and the rock layer's
    // world-space scale, as dense as the top-down layers'.
    let tile_scale = record
        .as_ref()
        .and_then(|r| crate::pds::find_terrain_config(&r.0))
        .map_or(tcfg::TILE_SCALE, |c| c.material.tile_scale.0);
    let triplanar_scale = tile_scale / world_extent.max(1.0);

    if let Some(mut mat) = materials.get_mut(&splat_mat.0) {
        enable_splat(
            &mut mat,
            wm_handle,
            (albedo_array, normal_array),
            (tile_scale, triplanar_scale),
            &water,
        );

        // Bind the avatar-interaction stains overlay (#245). The image
        // is allocated zeroed at startup, so enabling it now is inert
        // until the stamper writes contacts - backward-compatible. Every
        // build binds it, the WebGL2 one too since #1681: see the note on
        // `SplatExtension` in `crate::splat` for its texture slots.
        if let Some(stains) = stains.as_ref() {
            mat.extension.stains_tex = stains.handle.clone();
            mat.extension.stains.world_period = scfg::WORLD_PERIOD;
            mat.extension.stains.enabled = 1;
        }
    }

    // The four per-layer source images have now been concatenated into the two
    // RENDER_WORLD splat arrays bound above, so their bytes are fully redundant.
    // Drop the handles to free their MAIN_WORLD CPU copies (~11 MiB at the 512
    // splat resolution) - except when a Referenced layer is present, since a
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

/// Flip a ground material from its placeholder to splat blending: its
/// weight map, the layer arrays, the world-space texture scale, and the
/// damp margin round the room's `water` where it has some - the sea's line,
/// and its lakes' read from the weight map ([`lakes_uniform`]).
fn enable_splat(
    mat: &mut SplatTerrainMaterial,
    weight_map: Handle<Image>,
    (albedo_array, normal_array): (Handle<Image>, Handle<Image>),
    (tile_scale, triplanar_scale): (f32, f32),
    water: &crate::water::bodies::RoomWater,
) {
    textured_surface(&mut mat.base);
    mat.extension.weight_map = weight_map;
    mat.extension.albedo_array = albedo_array;
    mat.extension.normal_array = normal_array;
    mat.extension.uniforms.enabled = 1;
    mat.extension.uniforms.tile_scale = tile_scale;
    mat.extension.uniforms.triplanar_scale = triplanar_scale;
    let (water_y, strength) = moisture(water);
    mat.extension.uniforms.water_y = water_y;
    mat.extension.uniforms.moisture_depth = tcfg::splat::MOISTURE_DEPTH;
    mat.extension.uniforms.moisture_strength = strength;
    mat.extension.uniforms.lakes = lakes_uniform(water);
}

/// The damp margin's datum and strength for the room's `water`: on wherever
/// it has any, measured from its sea; where Berlin's own bodies are its
/// water and no sea spans it (#1724), from a line far under all ground,
/// which darkens nothing - each body's own margin is read from the weight
/// map ([`lakes_uniform`]). Off, and the datum 0, in a room with none.
fn moisture(water: &crate::water::bodies::RoomWater) -> (f32, f32) {
    /// The sea's datum where none spans the room: far under any ground.
    const NO_SEA_Y: f32 = -1.0e4;
    match (water.sea, water.is_dry()) {
        (_, true) => (0.0, 0.0),
        (sea, false) => (sea.unwrap_or(NO_SEA_Y), tcfg::splat::MOISTURE_STRENGTH),
    }
}

/// The splat uniforms' `lakes` for the room's `water` (#1727): on, with the
/// span the weight map's green is read over, where it has lakes; all zero
/// where it has none, which the ground reads as before they came.
fn lakes_uniform(water: &crate::water::bodies::RoomWater) -> Vec4 {
    if water.bodies.is_empty() {
        Vec4::ZERO
    } else {
        Vec4::new(1.0, tcfg::splat::LAKE_SPAN, 0.0, 0.0)
    }
}

/// The ground's surface once its splat textures land: a white base colour
/// (the shader draws the layer blend in its place), rough, not metallic,
/// and less reflective than `StandardMaterial`'s default, whose sheen
/// toward a low sun read as tan sand on every hill (#1467).
fn textured_surface(base: &mut StandardMaterial) {
    base.base_color = Color::WHITE;
    base.perceptual_roughness = tcfg::splat::MATERIAL_ROUGHNESS;
    base.metallic = tcfg::splat::MATERIAL_METALLIC;
    base.reflectance = tcfg::splat::MATERIAL_REFLECTANCE;
}

/// The ground's flat placeholder until its splat textures land - on first
/// load, through every terrain regeneration, and for good if a layer's bake
/// fails - at the textured ground's reflectance, so the sheen of #1467 does
/// not come back in between.
pub(super) fn placeholder_surface() -> StandardMaterial {
    let pc = tcfg::splat::PLACEHOLDER_COLOR;
    StandardMaterial {
        base_color: Color::srgb(pc[0], pc[1], pc[2]),
        perceptual_roughness: tcfg::splat::PLACEHOLDER_ROUGHNESS,
        reflectance: tcfg::splat::MATERIAL_REFLECTANCE,
        ..default()
    }
}

/// Keep the damp-ground datum in step with an edited water level (#913).
///
/// [`apply_splat_textures`] sets `water_y` when the splat textures land,
/// which covers first load and every terrain regeneration - but moving the
/// water plane in the editor changes the record WITHOUT regenerating the
/// heightmap, so the material would keep darkening the ground around the
/// old waterline until something else forced a terrain rebuild. Cheaper to
/// re-read the one scalar on record change than to make water edits
/// regenerate terrain they do not otherwise affect.
///
/// Writes only when the value actually moved: `Assets<T>::get_mut` fires a
/// change event that re-uploads the material's uniform block, and doing
/// that on every record edit would be a needless per-keystroke upload while
/// the editor is open.
pub(super) fn sync_moisture_water_level(
    record: Option<Res<LiveRoomRecord>>,
    splat_mat: Option<Res<SplatMaterialHandle>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut materials: ResMut<Assets<SplatTerrainMaterial>>,
) {
    let (Some(record), Some(splat_mat)) = (record, splat_mat) else {
        return;
    };
    if !record.is_changed() {
        return;
    }
    // The drawn water: an edit to a Berlin region's water layer moves
    // nothing, since Berlin's own bodies are drawn in its place (#1586,
    // #1724). A body's own margin is read from the weight map the ground
    // cover writes it in (#1727); this turns the reading on or off.
    let water = crate::world_builder::compile::drawn_water(&record.0, heightmap.as_deref());
    // No water generator, no waterline to be damp around.
    let (water_y, strength) = moisture(&water);
    let lakes = lakes_uniform(&water);
    let handle = &splat_mat.0;
    // Peek before taking the change-triggering mutable borrow.
    let stale = materials.get(handle).is_some_and(|m| {
        m.extension.uniforms.water_y != water_y
            || m.extension.uniforms.moisture_strength != strength
            || m.extension.uniforms.lakes != lakes
    });
    if stale && let Some(mut mat) = materials.get_mut(handle) {
        mat.extension.uniforms.water_y = water_y;
        mat.extension.uniforms.moisture_strength = strength;
        mat.extension.uniforms.lakes = lakes;
    }
}

/// The weather on the ground (#1683) for the drawn weather `now` over ground
/// standing from `low` to `high` (world Y): how wet it is, how much snow
/// lies where it has settled, and the snow line and its edge - the line
/// falling from over the highest ground (none settled) to under the lowest
/// (all of it) as the snow settles, so it creeps down the slopes.
pub(crate) fn ground_weather(now: &WeatherNow, low: f32, high: f32) -> GroundWeather {
    use crate::config::lighting::weather as wcfg;
    let span = (high - low).max(1.0);
    let feather = span * wcfg::SNOW_FEATHER_SHARE + wcfg::SNOW_FEATHER_M;
    // The line's edge reaches this many feathers from it: half a feather
    // of the shader's blend, and its patches' jitter
    // (`ground_common::snow_settled`).
    let reach = feather * SNOW_EDGE_FEATHERS;
    GroundWeather {
        wetness: now.wet,
        // Eased in, so the first snow lands as a dusting on the heights,
        // not at once at full white.
        snow: smoothstep(0.0, wcfg::SNOW_SHOWS, now.cover),
        snow_line: high + reach - (span + 2.0 * reach) * now.cover,
        snow_feather: feather,
    }
}

/// How many feathers from the snow line its edge reaches: the shader's
/// blend spans half a feather either side, and the patches move the line
/// up to 0.6 of one (`SNOW_PATCHINESS` 1.2 times a jitter of at most 0.5,
/// in `ground_common.wgsl`).
const SNOW_EDGE_FEATHERS: f32 = 1.1;

/// What [`ground_weather`] gives the ground's shader (and the grass's).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundWeather {
    pub wetness: f32,
    pub snow: f32,
    pub snow_line: f32,
    pub snow_feather: f32,
}

/// The weather on the ground now, as [`weather_the_ground`] gave it to the
/// ground's shader: the grass takes the same, so its far blades fade into
/// ground weathered as it is (#1683).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundWeatherNow(pub GroundWeather);

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The lowest and highest ground (world Y) of heightmap `hm`.
pub(crate) fn ground_range(hm: &bevy_symbios_ground::HeightMap) -> (f32, f32) {
    hm.data()
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)))
}

/// Lay the drawn weather on the ground (#1683): written into the splat
/// material whenever the drawn environment, the ground or the material
/// changes, and only when it moved - a material write rebuilds its bind
/// group (#1675) - and published as [`GroundWeatherNow`] for the grass.
pub(crate) fn weather_the_ground(
    drawn: Option<Res<crate::drawn_environment::DrawnEnvironment>>,
    splat_mat: Option<Res<SplatMaterialHandle>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut materials: ResMut<Assets<SplatTerrainMaterial>>,
    mut published: ResMut<GroundWeatherNow>,
    mut range: Local<Option<(f32, f32)>>,
) {
    let (Some(splat_mat), Some(heightmap)) = (splat_mat, heightmap) else {
        return;
    };
    // A new material comes with new ground: a heightmap that landed while
    // there was no material was seen by a run that returned above.
    if heightmap.is_changed() || splat_mat.is_changed() || range.is_none() {
        *range = Some(ground_range(&heightmap.0));
    }
    let drawn_changed = drawn.as_ref().is_some_and(|d| d.is_changed());
    if !drawn_changed && !heightmap.is_changed() && !splat_mat.is_changed() {
        return;
    }
    // The render tool's `--terrain` draws the ground with no drawn
    // environment: no weather.
    let weather = drawn.map_or_else(WeatherNow::default, |d| d.weather);
    let (low, high) = range.unwrap_or((0.0, 0.0));
    let wanted = ground_weather(&weather, low, high);
    if published.0 != wanted {
        published.0 = wanted;
    }
    let held = |m: &SplatTerrainMaterial| {
        let u = &m.extension.uniforms;
        GroundWeather {
            wetness: u.wetness,
            snow: u.snow,
            snow_line: u.snow_line,
            snow_feather: u.snow_feather,
        }
    };
    if materials
        .get(&splat_mat.0)
        .is_some_and(|m| held(m) != wanted)
        && let Some(mut m) = materials.get_mut(&splat_mat.0)
    {
        let u = &mut m.extension.uniforms;
        u.wetness = wanted.wetness;
        u.snow = wanted.snow;
        u.snow_line = wanted.snow_line;
        u.snow_feather = wanted.snow_feather;
    }
}

/// The grass the ground shows (#1695), as `crate::grass` lays it: the room's
/// grass colours, its flowers and how thickly it grows on each layer, the
/// wind it leans in and how far it sways (#1700), and how strongly the
/// ground shows it - 1 while the grass grows, 0 while it is off.
/// [`show_the_grass`] writes it into the splat material; the zeroed default
/// shows none.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundGrass {
    pub carpet: f32,
    /// Its blades' colours at the root, the tip and dry, linear RGB, and
    /// the share of them that are dry.
    pub base: Vec3,
    pub tip: Vec3,
    pub dry: Vec3,
    pub dry_share: f32,
    /// Its flowers' mean colour, linear RGB, and the share of its blades
    /// that are flowers (#1700).
    pub flower: Vec3,
    pub flower_share: f32,
    /// How thickly it grows on each splat layer, 0 to 1.
    pub layer_density: Vec4,
    /// The wind: the way it blows, its speed (m/s) and how stormy it is.
    pub wind_dir: Vec2,
    pub wind_speed: f32,
    pub storm: f32,
    /// How far the wind leans it, as a share of `config::grass::WIND_LEAN`
    /// (#1700).
    pub sway: f32,
    /// How much it grows in tussocks and clumps, and how much of the ground
    /// it hides (#1702, `grass::profile::GrassProfile`).
    pub clumpiness: f32,
    pub carpet_depth: f32,
}

impl GroundGrass {
    /// What splat uniforms `u` hold of it.
    fn held(u: &crate::splat::SplatUniforms) -> Self {
        Self {
            carpet: u.grass_carpet,
            base: u.grass_base.truncate(),
            tip: u.grass_tip.truncate(),
            dry: u.grass_dry.truncate(),
            dry_share: u.grass_dry_share,
            flower: u.grass_flower.truncate(),
            flower_share: u.grass_flower.w,
            layer_density: u.grass_layer_density,
            wind_dir: u.wind.truncate().truncate(),
            wind_speed: u.wind.z,
            storm: u.storm,
            sway: u.wind.w,
            clumpiness: u.grass_shape.x,
            carpet_depth: u.grass_shape.y,
        }
    }

    /// Write it into splat uniforms `u`.
    fn write(&self, u: &mut crate::splat::SplatUniforms) {
        u.grass_carpet = self.carpet;
        u.grass_base = self.base.extend(0.0);
        u.grass_tip = self.tip.extend(0.0);
        u.grass_dry = self.dry.extend(0.0);
        u.grass_dry_share = self.dry_share;
        u.grass_flower = self.flower.extend(self.flower_share);
        u.grass_layer_density = self.layer_density;
        u.wind = self.wind_dir.extend(self.wind_speed).extend(self.sway);
        u.storm = self.storm;
        u.grass_shape = Vec4::new(self.clumpiness, self.carpet_depth, 0.0, 0.0);
    }
}

/// Where [`show_the_grass`] runs: before the frame's asset events, so the
/// grass laid this frame is drawn this frame. Whatever lays [`GroundGrass`]
/// orders itself before it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroundGrassSet;

/// Lay the grass on the ground (#1695): [`GroundGrass`] into the splat
/// material whenever it, or the material, changes - and only when it
/// moved, as a material write rebuilds its bind group (#1675).
pub(crate) fn show_the_grass(
    grass: Res<GroundGrass>,
    splat_mat: Option<Res<SplatMaterialHandle>>,
    mut materials: ResMut<Assets<SplatTerrainMaterial>>,
) {
    let Some(splat_mat) = splat_mat else {
        return;
    };
    if !grass.is_changed() && !splat_mat.is_changed() {
        return;
    }
    if materials
        .get(&splat_mat.0)
        .is_some_and(|m| GroundGrass::held(&m.extension.uniforms) != *grass)
        && let Some(mut m) = materials.get_mut(&splat_mat.0)
    {
        grass.write(&mut m.extension.uniforms);
    }
}

/// Free the four per-layer source images of a *Referenced* room once every layer
/// fetch has resolved (#642). `apply_splat_textures` drops them the same frame
/// for procedural rooms, but deliberately skips the free while any Referenced
/// layer is configured - a late blob fetch flips `state.applied = false` and
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

/// The splat mapper the ground's weight map is generated with: the record's
/// four rules, heights scaled from fractions of `height_scale` to metres -
/// or the canonical defaults for a record with no terrain generator. Pulling
/// the rules from the record is what lets the World Editor re-balance the
/// ground without a recompile.
///
/// One builder for every reader of the weights: the GPU weight map, the
/// contact classifier's CPU mirror, and the render tool's terrain report,
/// which prints each layer's share at a point (#1461).
pub(crate) fn record_splat_mapper(record: Option<&crate::pds::RoomRecord>) -> SplatMapper {
    let (rules, hs) = record
        .and_then(|r| {
            crate::pds::find_terrain_config(r).map(|c| (c.material.rules, c.height_scale.0))
        })
        .unwrap_or_else(|| {
            (
                SovereignTerrainConfig::default().material.rules,
                tcfg::HEIGHT_SCALE,
            )
        });
    // R, G, B, A: layers 0 to 3 (the defaults' grass, dirt, rock, snow).
    SplatMapper::new(rules.map(|r| {
        SplatRule::new(
            (hs * r.height_min.0, hs * r.height_max.0),
            (r.slope_min.0, r.slope_max.0),
            r.sharpness.0,
        )
    }))
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
    // the descriptor's *default* `mip_level_count = 1` and panics - the
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #1724: the damp margin is on wherever the room has water - measured
    /// from its sea, or, where Berlin's own bodies are its water and no sea
    /// spans it, from a datum under all ground, which darkens nothing, each
    /// body's margin being read from the weight map - and off in a dry one.
    #[test]
    fn the_damp_margin_follows_the_rooms_water() {
        use crate::water::bodies::RoomWater;
        assert_eq!(moisture(&RoomWater::default()), (0.0, 0.0));
        assert_eq!(
            moisture(&RoomWater::sea_alone(Some(4.5))),
            (4.5, tcfg::splat::MOISTURE_STRENGTH)
        );
        let hm = bevy_symbios_ground::HeightMap::new(8, 8, 1.0);
        let berlin = RoomWater {
            sea: None,
            bodies: crate::terrain::geo::GeoGround::water_of_cover(
                &hm,
                &[Some(geodata::berlin::LandUse::Water); 64],
                30.5,
            ),
        };
        let (datum, strength) = moisture(&berlin);
        assert!(datum < -1_000.0 && strength == tcfg::splat::MOISTURE_STRENGTH);
        assert_eq!(
            lakes_uniform(&berlin).x,
            1.0,
            "its bodies' margins are read"
        );
    }

    /// The snow creeps down the slopes (#1683): with none settled its line
    /// stands over the highest ground, and it falls as the snow settles
    /// until, all settled, it is under the lowest; the snow eases in, a
    /// dusting first; and a dry ground stays dry.
    #[test]
    fn the_snow_line_falls_as_the_snow_settles() {
        let (low, high) = (10.0, 70.0);
        let at = |cover: f32| {
            ground_weather(
                &WeatherNow {
                    cover,
                    ..WeatherNow::default()
                },
                low,
                high,
            )
        };
        let none = at(0.0);
        assert_eq!(none.snow, 0.0);
        assert!(none.snow_line - none.snow_feather >= high, "{none:?}");
        let all = at(1.0);
        assert_eq!(all.snow, 1.0);
        assert!(all.snow_line + all.snow_feather <= low, "{all:?}");
        let mut last = none.snow_line;
        for k in 1..=20 {
            let line = at(k as f32 / 20.0).snow_line;
            assert!(line < last, "the line falls: {line} after {last}");
            last = line;
        }
        let dusting = at(0.05);
        assert!(dusting.snow > 0.0 && dusting.snow < 0.5, "{dusting:?}");
        assert_eq!(
            ground_weather(&WeatherNow::default(), low, high).wetness,
            0.0
        );
        // A flat ground still has an edge to its line.
        assert!(ground_weather(&WeatherNow::default(), 5.0, 5.0).snow_feather > 0.0);
    }

    /// #1467: seen toward a low sun, the ground's default reflectance put a
    /// sheen on every hill that read as tan sand, whatever its texture.
    #[test]
    fn the_textured_ground_reflects_less_than_the_default() {
        let mut base = StandardMaterial::default();
        textured_surface(&mut base);
        assert_eq!(base.reflectance, tcfg::splat::MATERIAL_REFLECTANCE);
        assert!(
            base.reflectance < StandardMaterial::default().reflectance,
            "the textured ground keeps less sheen than the default material"
        );
        assert_eq!(base.perceptual_roughness, tcfg::splat::MATERIAL_ROUGHNESS);
    }

    /// #1467's review: the placeholder the ground wears until its textures
    /// land - through every terrain regeneration - kept the old sheen.
    #[test]
    fn the_placeholder_ground_reflects_as_the_textured_ground_does() {
        assert_eq!(
            placeholder_surface().reflectance,
            tcfg::splat::MATERIAL_REFLECTANCE
        );
    }

    /// The room's grass as the grass lays it on the ground (#1695).
    fn some_grass() -> GroundGrass {
        GroundGrass {
            carpet: 1.0,
            base: Vec3::new(0.1, 0.2, 0.05),
            tip: Vec3::new(0.4, 0.6, 0.2),
            dry: Vec3::new(0.6, 0.5, 0.3),
            dry_share: 0.25,
            flower: Vec3::new(0.5, 0.2, 0.6),
            flower_share: 0.1,
            layer_density: Vec4::new(1.0, 0.5, 0.0, 0.1),
            wind_dir: Vec2::new(0.6, 0.8),
            wind_speed: 5.0,
            storm: 0.3,
            sway: 1.5,
            clumpiness: 1.2,
            carpet_depth: 2.1,
        }
    }

    /// What the uniforms hold of the grass is what was written into them,
    /// field for field, and a ground made without grass shows none.
    #[test]
    fn the_ground_holds_the_grass_laid_on_it() {
        let mut u = crate::splat::SplatUniforms::default();
        assert_eq!(GroundGrass::held(&u), GroundGrass::default());
        assert_eq!(GroundGrass::held(&u).carpet, 0.0);
        some_grass().write(&mut u);
        assert_eq!(GroundGrass::held(&u), some_grass());
    }

    /// The grass is written into the ground's material when it changes, and
    /// not again while it holds - a write rebuilds the material - nor while
    /// no ground stands.
    #[test]
    fn the_grass_is_laid_on_the_ground_only_when_it_moves() {
        #[derive(Resource, Default)]
        struct Writes(usize);
        fn count(materials: Res<Assets<SplatTerrainMaterial>>, mut writes: ResMut<Writes>) {
            if materials.is_changed() {
                writes.0 += 1;
            }
        }
        let mut app = App::new();
        app.init_resource::<Assets<SplatTerrainMaterial>>()
            .init_resource::<GroundGrass>()
            .init_resource::<Writes>()
            .add_systems(Update, (show_the_grass, count).chain());
        // No ground yet: nothing to lay it on, and none made for it.
        app.insert_resource(some_grass());
        app.update();
        assert!(
            app.world()
                .resource::<Assets<SplatTerrainMaterial>>()
                .is_empty()
        );
        let handle = app
            .world_mut()
            .resource_mut::<Assets<SplatTerrainMaterial>>()
            .add(SplatTerrainMaterial {
                base: StandardMaterial::default(),
                extension: crate::splat::SplatExtension::default(),
            });
        app.insert_resource(super::super::SplatMaterialHandle(handle.clone()));
        app.update();
        let held = |app: &App| {
            let materials = app.world().resource::<Assets<SplatTerrainMaterial>>();
            GroundGrass::held(
                &materials
                    .get(&handle)
                    .expect("the ground")
                    .extension
                    .uniforms,
            )
        };
        assert_eq!(held(&app), some_grass());
        let writes = app.world().resource::<Writes>().0;
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<Writes>().0,
            writes,
            "rewritten unchanged"
        );
        // Told it changed when it did not - laid again as it was, or the
        // ground's handle given again - the material is not written.
        app.world_mut().resource_mut::<GroundGrass>().set_changed();
        app.insert_resource(super::super::SplatMaterialHandle(handle.clone()));
        app.update();
        assert_eq!(
            app.world().resource::<Writes>().0,
            writes,
            "rewritten as it was"
        );
        app.world_mut().resource_mut::<GroundGrass>().carpet = 0.0;
        app.update();
        assert_eq!(held(&app).carpet, 0.0);
        assert_eq!(app.world().resource::<Writes>().0, writes + 1);
    }
}
