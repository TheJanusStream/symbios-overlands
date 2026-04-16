//! Deterministic procedural terrain plugin.
//!
//! A room's seed is the FNV-1a 64-bit hash of its owner's DID, so every
//! client visiting the same overland derives the identical landscape locally
//! — there is no authoritative server to replicate from.  Heightmap
//! generation (Voronoi terracing → hydraulic erosion → thermal erosion) runs
//! on `AsyncComputeTaskPool` while the four splat layer textures
//! (grass / dirt / rock / snow) are baked in parallel by
//! `bevy_symbios_texture`.  Once every task has finished, the layers are
//! concatenated into a 2D texture array and the `SplatExtension` material is
//! flipped from placeholder flat-colour mode to triplanar PBR splat blending.
//!
//! Water is spawned by `world_builder.rs` from the `Water` generator in the
//! active `RoomRecord` — this plugin only produces the terrain mesh and
//! heightfield collider.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat}; // Added TextureFormat
use bevy::tasks::AsyncComputeTaskPool;
use bevy_symbios_ground::{HeightMapMeshBuilder, NormalMethod, build_heightfield_collider};
use bevy_symbios_texture::SymbiosTexturePlugin;
use bevy_symbios_texture::async_gen::{PendingTexture, TextureReady};
use bevy_symbios_texture::ground::GroundConfig;
use bevy_symbios_texture::rock::RockConfig;
use symbios_ground::{
    DiamondSquare, FbmNoise, HeightMap, HydraulicErosion, SplatMapper, SplatRule, TerrainGenerator,
    ThermalErosion, VoronoiTerracing,
};

use crate::config::terrain as tcfg;
use crate::pds::{
    RoomRecord, SovereignGeneratorKind, SovereignGroundConfig, SovereignRockConfig,
    SovereignTerrainConfig,
};
use crate::splat::{SplatExtension, SplatTerrainMaterial, SplatUniforms};
use crate::state::AppState;

/// Marker inserted once the texture-layer spawn step has run, so the
/// Loading-phase scheduler doesn't kick the same four tasks twice while
/// waiting for `poll_terrain_task` to drain.
#[derive(Resource)]
struct TextureTasksStarted;

#[derive(Component)]
pub struct TerrainMesh;

/// Marker component for the water-level volume entity (translucent cuboid).
#[derive(Component)]
pub struct WaterVolume;

#[derive(Resource)]
pub struct FinishedHeightMap(pub HeightMap);

#[derive(Resource)]
pub struct TerrainTask(pub bevy::tasks::Task<HeightMap>);

/// Marker on the PendingTexture entity; value = layer index (0–3).
#[derive(Component)]
struct TextureLayerIndex(usize);

/// Accumulated handles from completed PendingTexture tasks.
#[derive(Resource, Default)]
struct TerrainSplatState {
    layer_albedo: [Option<Handle<Image>>; 4],
    layer_normal: [Option<Handle<Image>>; 4],
    applied: bool,
}

impl TerrainSplatState {
    fn all_ready(&self) -> bool {
        self.layer_albedo.iter().all(|h| h.is_some())
            && self.layer_normal.iter().all(|h| h.is_some())
    }
}

/// Handle to the terrain's `SplatTerrainMaterial`; stored so `apply_splat_textures`
/// can update it once all texture tasks finish.
#[derive(Resource)]
struct SplatMaterialHandle(Handle<SplatTerrainMaterial>);

/// Serialised fingerprint of the terrain config currently compiled into the
/// live heightmap. `maybe_regenerate_terrain` compares the active
/// `RoomRecord`'s terrain config against this and, on mismatch, triggers a
/// full heightmap/texture/mesh rebuild in place. Without this, a room owner
/// editing noise parameters would desync every guest: the live terrain would
/// stay frozen for everyone already in the room, while a new guest joining
/// afterwards would enter `Loading` and generate the *new* terrain — so
/// older peers and newcomers end up driving on fundamentally different
/// ground.
#[derive(Resource, Default)]
struct LastTerrainConfigJson(Option<String>);

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SymbiosTexturePlugin)
            .add_plugins(MaterialPlugin::<SplatTerrainMaterial>::default())
            .init_resource::<TerrainSplatState>()
            .init_resource::<LastTerrainConfigJson>()
            // Terrain + texture + mesh spawning runs as conditional Update
            // systems in both Loading and InGame so the same plumbing handles
            // the initial world build *and* in-place regeneration when the
            // room owner edits terrain parameters mid-session. Each step
            // guards itself against double-kicking with a resource/marker
            // check.
            .add_systems(
                Update,
                (
                    start_terrain_generation.run_if(
                        resource_exists::<RoomRecord>
                            .and(not(resource_exists::<TerrainTask>))
                            .and(not(resource_exists::<FinishedHeightMap>)),
                    ),
                    start_texture_tasks.run_if(
                        resource_exists::<RoomRecord>
                            .and(not(resource_exists::<TextureTasksStarted>)),
                    ),
                    poll_terrain_task.run_if(resource_exists::<TerrainTask>),
                    spawn_terrain_mesh.run_if(
                        resource_exists::<FinishedHeightMap>
                            .and(not(resource_exists::<SplatMaterialHandle>)),
                    ),
                )
                    .run_if(not(in_state(AppState::Login))),
            )
            .add_systems(
                Update,
                (
                    maybe_regenerate_terrain.run_if(resource_exists::<RoomRecord>),
                    collect_texture_results,
                    apply_splat_textures,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_terrain);
    }
}

/// Despawn terrain + water entities and reset terrain-specific resources so
/// the next login cycle restarts heightmap generation and splat texture
/// uploads from scratch.
fn cleanup_terrain(
    mut commands: Commands,
    terrain: Query<Entity, With<TerrainMesh>>,
    water: Query<Entity, With<WaterVolume>>,
    pending_textures: Query<Entity, With<TextureLayerIndex>>,
    pending_raw: Query<Entity, With<PendingTexture>>,
    mut splat_state: ResMut<TerrainSplatState>,
    mut last_cfg: ResMut<LastTerrainConfigJson>,
) {
    for e in &terrain {
        commands.entity(e).despawn();
    }
    for e in &water {
        commands.entity(e).despawn();
    }
    // In-flight splat texture tasks would otherwise survive into the next
    // login cycle, land their `TextureReady` components on orphaned entities,
    // and leak until process exit. Drain both the marker-tagged and any
    // `PendingTexture` entities missing the marker (recovery path) here.
    for e in &pending_textures {
        commands.entity(e).despawn();
    }
    for e in &pending_raw {
        commands.entity(e).despawn();
    }
    *splat_state = TerrainSplatState::default();
    last_cfg.0 = None;
    commands.remove_resource::<FinishedHeightMap>();
    commands.remove_resource::<SplatMaterialHandle>();
    commands.remove_resource::<TextureTasksStarted>();
    commands.remove_resource::<TerrainTask>();
}

/// Watch the active room record for terrain-config changes. When the owner
/// edits grid size, noise params, erosion, splat rules, or any other
/// terrain-affecting field, despawn the existing heightfield, drop the
/// cached heightmap / splat resources, and let the generic `Update`
/// pipeline re-kick terrain + texture tasks from scratch. The first
/// observation of a config simply records the fingerprint — Loading handled
/// the initial build — so this only fires on *changes* after the player is
/// already InGame.
#[allow(clippy::too_many_arguments)]
fn maybe_regenerate_terrain(
    mut commands: Commands,
    record: Res<RoomRecord>,
    mut last_cfg: ResMut<LastTerrainConfigJson>,
    terrain_q: Query<Entity, With<TerrainMesh>>,
    pending_textures: Query<Entity, With<TextureLayerIndex>>,
    mut splat_state: ResMut<TerrainSplatState>,
    terrain_task: Option<Res<TerrainTask>>,
) {
    // Refuse to tear down in-flight generation — the previous async task's
    // output would still land in `FinishedHeightMap` and the new pipeline
    // couldn't start. Retain the pending record change for a later frame;
    // this system runs every frame, not just on `is_changed`, so once the
    // task completes the regeneration kicks in.
    if terrain_task.is_some() {
        return;
    }
    let Some(cfg) = crate::pds::find_terrain_config(&record) else {
        return;
    };
    let Ok(fp) = serde_json::to_string(cfg) else {
        return;
    };
    let should_regen = match &last_cfg.0 {
        Some(prev) if prev == &fp => false,
        Some(_) => true,
        None => false, // first observation — initial Loading pipeline built it
    };
    last_cfg.0 = Some(fp);
    if !should_regen {
        return;
    }

    // Tear down everything tied to the old heightmap so the generic Update
    // pipeline above re-kicks terrain generation, texture baking, and mesh
    // spawning against the new config on subsequent frames. Water is a
    // `RoomEntity`, so `compile_room_record` despawns and rebuilds it in
    // response to the same record change — touching it here would race and
    // double-despawn.
    for e in &terrain_q {
        commands.entity(e).despawn();
    }
    for e in &pending_textures {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<FinishedHeightMap>();
    commands.remove_resource::<SplatMaterialHandle>();
    commands.remove_resource::<TextureTasksStarted>();
    commands.remove_resource::<TerrainTask>();
    *splat_state = TerrainSplatState::default();
    info!("Terrain config changed — regenerating heightmap + splat textures");
}

// ---------------------------------------------------------------------------
// Loading state — terrain generation + async texture tasks
// ---------------------------------------------------------------------------

fn start_terrain_generation(mut commands: Commands, record: Res<RoomRecord>) {
    // `find_terrain_config` walks the generator map in sorted-key order so
    // every peer compiling this record picks the same entry — `HashMap`
    // iteration is SipHash-randomised per process, and without the helper
    // two clients could generate different terrains from the same record.
    let cfg = crate::pds::find_terrain_config(&record)
        .cloned()
        .unwrap_or_default();

    let pool = AsyncComputeTaskPool::get();
    let task = pool.spawn(async move { generate_terrain(&cfg) });
    commands.insert_resource(TerrainTask(task));
}

/// Spawn four `PendingTexture` entities (one per splat layer), pulling the
/// procedural configs from the active `RoomRecord`'s terrain generator.
/// `SymbiosTexturePlugin` polls them every frame and attaches `TextureReady`
/// when done. A `TextureTasksStarted` marker is inserted to make this a
/// one-shot inside the Loading-phase scheduler loop.
fn start_texture_tasks(mut commands: Commands, record: Res<RoomRecord>) {
    let mat = crate::pds::find_terrain_config(&record)
        .map(|c| c.material.clone())
        .unwrap_or_default();

    let texture_size = mat.texture_size.max(16);

    // Layer 0 — Grass
    commands.spawn((
        PendingTexture::ground(
            sovereign_ground_to_texture(&mat.grass),
            texture_size,
            texture_size,
        ),
        TextureLayerIndex(0),
    ));

    // Layer 1 — Dirt
    commands.spawn((
        PendingTexture::ground(
            sovereign_ground_to_texture(&mat.dirt),
            texture_size,
            texture_size,
        ),
        TextureLayerIndex(1),
    ));

    // Layer 2 — Rock (ridged multifractal stone)
    commands.spawn((
        PendingTexture::rock(
            sovereign_rock_to_texture(&mat.rock),
            texture_size,
            texture_size,
        ),
        TextureLayerIndex(2),
    ));

    // Layer 3 — Snow
    commands.spawn((
        PendingTexture::ground(
            sovereign_ground_to_texture(&mat.snow),
            texture_size,
            texture_size,
        ),
        TextureLayerIndex(3),
    ));

    commands.insert_resource(TextureTasksStarted);
}

fn sovereign_ground_to_texture(g: &SovereignGroundConfig) -> GroundConfig {
    GroundConfig {
        seed: g.seed,
        macro_scale: g.macro_scale.0,
        macro_octaves: g.macro_octaves as usize,
        micro_scale: g.micro_scale.0,
        micro_octaves: g.micro_octaves as usize,
        micro_weight: g.micro_weight.0,
        color_dry: g.color_dry.0,
        color_moist: g.color_moist.0,
        normal_strength: g.normal_strength.0,
    }
}

fn sovereign_rock_to_texture(r: &SovereignRockConfig) -> RockConfig {
    RockConfig {
        seed: r.seed,
        scale: r.scale.0,
        octaves: r.octaves as usize,
        attenuation: r.attenuation.0,
        color_light: r.color_light.0,
        color_dark: r.color_dark.0,
        normal_strength: r.normal_strength.0,
    }
}

fn poll_terrain_task(mut commands: Commands, mut task_res: ResMut<TerrainTask>) {
    if let Some(hm) =
        futures_lite::future::block_on(futures_lite::future::poll_once(&mut task_res.0))
    {
        commands.remove_resource::<TerrainTask>();
        commands.insert_resource(FinishedHeightMap(hm));
    }
}

// ---------------------------------------------------------------------------
// InGame state — mesh + material, then apply splat when textures arrive
// ---------------------------------------------------------------------------

fn spawn_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SplatTerrainMaterial>>,
    hm_res: Res<FinishedHeightMap>,
) {
    let hm = &hm_res.0;
    let world_extent = (hm.width() - 1) as f32 * hm.scale();
    let half = world_extent * 0.5;

    let mut mesh = HeightMapMeshBuilder::new()
        .with_normal_method(NormalMethod::AreaWeighted)
        .with_uv_tile_size(world_extent)
        .build(hm);

    mesh.generate_tangents()
        .expect("terrain tangent generation failed");

    let collider = build_heightfield_collider(hm);

    // Generate D2Array placeholders to satisfy WGPU validation until the real arrays load
    let albedo_placeholder = images.add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 4,
        },
        TextureDimension::D2,
        vec![255u8; 16],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));

    let normal_placeholder = images.add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 4,
        },
        TextureDimension::D2,
        [128u8, 128, 255, 255].repeat(4),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    ));

    // Material starts disabled (flat colour) until the texture tasks finish.
    let pc = tcfg::splat::PLACEHOLDER_COLOR;
    let mat_handle = materials.add(bevy::pbr::ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(pc[0], pc[1], pc[2]),
            perceptual_roughness: tcfg::splat::PLACEHOLDER_ROUGHNESS,
            ..default()
        },
        extension: SplatExtension {
            albedo_array: albedo_placeholder,
            normal_array: normal_placeholder,
            uniforms: SplatUniforms {
                tile_scale: tcfg::TILE_SCALE,
                enabled: 0,
                triplanar_scale: tcfg::TILE_SCALE / world_extent.max(1.0),
                triplanar_sharpness: tcfg::splat::TRIPLANAR_SHARPNESS,
            },
            ..default() // weight_map defaults to 1x1 D2, which is fine for the weight sampler
        },
    });

    commands.insert_resource(SplatMaterialHandle(mat_handle.clone()));

    commands
        .spawn((
            Transform::IDENTITY,
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RigidBody::Static,
            collider,
            TerrainMesh,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(-half, 0.0, -half),
            ));
        });

    // Water is spawned separately by `world_builder.rs` from whichever
    // `Water` generator the active `RoomRecord` declares.
}

/// Consume `TextureReady` components attached by `SymbiosTexturePlugin` and
/// store the GPU handles by layer index.
fn collect_texture_results(
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
fn apply_splat_textures(
    mut state: ResMut<TerrainSplatState>,
    hm_res: Option<Res<FinishedHeightMap>>,
    splat_mat: Option<Res<SplatMaterialHandle>>,
    record: Option<Res<RoomRecord>>,
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
            crate::pds::find_terrain_config(r).map(|c| (c.material.rules, c.height_scale.0))
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

    let mut array_img = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 4,
        },
        TextureDimension::D2,
        merged,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    array_img.texture_descriptor.mip_level_count = mip_count;
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

// ---------------------------------------------------------------------------
// Heightmap generation (runs on async thread)
// ---------------------------------------------------------------------------

fn generate_terrain(cfg: &SovereignTerrainConfig) -> HeightMap {
    let grid = (cfg.grid_size as usize).max(2);
    let mut hm = HeightMap::new(grid, grid, cfg.cell_scale.0.max(0.01));

    // Dispatch on the requested algorithm; each generator is reproducible
    // from `cfg.seed` alone, which is the same across every visiting peer.
    match cfg.generator_kind {
        SovereignGeneratorKind::FbmNoise => {
            let fbm = FbmNoise {
                seed: cfg.seed,
                octaves: cfg.octaves.clamp(1, 32),
                persistence: cfg.persistence.0,
                lacunarity: cfg.lacunarity.0,
                base_frequency: cfg.base_frequency.0,
            };
            fbm.generate(&mut hm);
            hm.normalize();
        }
        SovereignGeneratorKind::DiamondSquare => {
            DiamondSquare::new(cfg.seed, cfg.ds_roughness.0).generate(&mut hm);
            hm.normalize();
        }
        SovereignGeneratorKind::VoronoiTerracing => {
            VoronoiTerracing::new(
                cfg.seed,
                cfg.voronoi_num_seeds.max(1) as usize,
                cfg.voronoi_num_terraces.max(1) as usize,
            )
            .generate(&mut hm);
            // Voronoi already emits bounded [0, 1] output.
        }
    }

    for v in hm.data_mut() {
        *v *= cfg.height_scale.0;
    }

    if cfg.erosion_enabled {
        HydraulicErosion {
            seed: cfg.seed,
            num_drops: cfg.erosion_drops,
            max_steps: tcfg::hydraulic::MAX_STEPS,
            inertia: cfg.inertia.0,
            erosion_rate: cfg.erosion_rate.0,
            deposition_rate: cfg.deposition_rate.0,
            evaporation_rate: cfg.evaporation_rate.0,
            capacity_factor: cfg.capacity_factor.0,
            min_slope: tcfg::hydraulic::MIN_SLOPE,
            water_level: tcfg::hydraulic::WATER_LEVEL,
        }
        .erode(&mut hm);
    }

    if cfg.thermal_enabled {
        ThermalErosion::new()
            .with_iterations(cfg.thermal_iterations)
            .with_talus_angle(cfg.thermal_talus_angle.0)
            .erode(&mut hm);
    }

    hm
}
