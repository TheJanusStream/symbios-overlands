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
    HeightMap, HydraulicErosion, SplatMapper, SplatRule, TerrainGenerator, ThermalErosion,
    VoronoiTerracing,
};

use crate::config::terrain as tcfg;
use crate::splat::{SplatExtension, SplatTerrainMaterial, SplatUniforms};
use crate::state::{AppState, CurrentRoomDid};
use crate::water::{WaterExtension, WaterMaterial};

/// Deterministic FNV-1a 64-bit hash.  Standard `DefaultHasher` is randomly
/// keyed per-process (HashDoS mitigation) so it cannot be used here — every
/// player visiting the same DID must derive the identical seed.
pub fn hash_did_to_seed(did: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in did.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

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

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SymbiosTexturePlugin)
            .add_plugins(MaterialPlugin::<SplatTerrainMaterial>::default())
            .add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<TerrainSplatState>()
            .add_systems(
                OnEnter(AppState::Loading),
                (start_terrain_generation, start_texture_tasks),
            )
            .add_systems(
                Update,
                poll_terrain_task
                    .run_if(in_state(AppState::Loading).and(resource_exists::<TerrainTask>)),
            )
            .add_systems(OnEnter(AppState::InGame), spawn_terrain_mesh)
            .add_systems(
                Update,
                (collect_texture_results, apply_splat_textures)
                    .chain()
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
    mut splat_state: ResMut<TerrainSplatState>,
) {
    for e in &terrain {
        commands.entity(e).despawn();
    }
    for e in &water {
        commands.entity(e).despawn();
    }
    *splat_state = TerrainSplatState::default();
    commands.remove_resource::<FinishedHeightMap>();
    commands.remove_resource::<SplatMaterialHandle>();
}

// ---------------------------------------------------------------------------
// Loading state — terrain generation + async texture tasks
// ---------------------------------------------------------------------------

fn start_terrain_generation(mut commands: Commands, room_did: Res<CurrentRoomDid>) {
    let seed = hash_did_to_seed(&room_did.0);
    let pool = AsyncComputeTaskPool::get();
    let task = pool.spawn(async move { generate_terrain(seed) });
    commands.insert_resource(TerrainTask(task));
}

/// Spawn four `PendingTexture` entities (one per splat layer) in Loading so
/// they run in parallel with heightmap generation.  `SymbiosTexturePlugin`
/// polls them every frame and attaches `TextureReady` when done.
fn start_texture_tasks(mut commands: Commands) {
    // Layer 0 — Grass (lush green ground cover)
    commands.spawn((
        PendingTexture::ground(
            GroundConfig {
                seed: tcfg::grass::SEED,
                macro_scale: tcfg::grass::MACRO_SCALE,
                macro_octaves: tcfg::grass::MACRO_OCTAVES,
                micro_scale: tcfg::grass::MICRO_SCALE,
                micro_octaves: tcfg::grass::MICRO_OCTAVES,
                micro_weight: tcfg::grass::MICRO_WEIGHT,
                color_dry: tcfg::grass::COLOR_DRY,
                color_moist: tcfg::grass::COLOR_MOIST,
                normal_strength: tcfg::grass::NORMAL_STRENGTH,
            },
            tcfg::TEXTURE_SIZE,
            tcfg::TEXTURE_SIZE,
        ),
        TextureLayerIndex(0),
    ));

    // Layer 1 — Dirt (brownish soil)
    commands.spawn((
        PendingTexture::ground(
            GroundConfig {
                seed: tcfg::dirt::SEED,
                macro_scale: tcfg::dirt::MACRO_SCALE,
                macro_octaves: tcfg::dirt::MACRO_OCTAVES,
                micro_scale: tcfg::dirt::MICRO_SCALE,
                micro_octaves: tcfg::dirt::MICRO_OCTAVES,
                micro_weight: tcfg::dirt::MICRO_WEIGHT,
                color_dry: tcfg::dirt::COLOR_DRY,
                color_moist: tcfg::dirt::COLOR_MOIST,
                normal_strength: tcfg::dirt::NORMAL_STRENGTH,
            },
            tcfg::TEXTURE_SIZE,
            tcfg::TEXTURE_SIZE,
        ),
        TextureLayerIndex(1),
    ));

    // Layer 2 — Rock (ridged multifractal stone)
    commands.spawn((
        PendingTexture::rock(
            RockConfig {
                seed: tcfg::rock::SEED,
                scale: tcfg::rock::SCALE,
                octaves: tcfg::rock::OCTAVES,
                attenuation: tcfg::rock::ATTENUATION,
                color_light: tcfg::rock::COLOR_LIGHT,
                color_dark: tcfg::rock::COLOR_DARK,
                normal_strength: tcfg::rock::NORMAL_STRENGTH,
            },
            tcfg::TEXTURE_SIZE,
            tcfg::TEXTURE_SIZE,
        ),
        TextureLayerIndex(2),
    ));

    // Layer 3 — Snow (pale, low-relief ground cover)
    commands.spawn((
        PendingTexture::ground(
            GroundConfig {
                seed: tcfg::snow::SEED,
                macro_scale: tcfg::snow::MACRO_SCALE,
                macro_octaves: tcfg::snow::MACRO_OCTAVES,
                micro_scale: tcfg::snow::MICRO_SCALE,
                micro_octaves: tcfg::snow::MICRO_OCTAVES,
                micro_weight: tcfg::snow::MICRO_WEIGHT,
                color_dry: tcfg::snow::COLOR_DRY,
                color_moist: tcfg::snow::COLOR_MOIST,
                normal_strength: tcfg::snow::NORMAL_STRENGTH,
            },
            tcfg::TEXTURE_SIZE,
            tcfg::TEXTURE_SIZE,
        ),
        TextureLayerIndex(3),
    ));
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
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    hm_res: Res<FinishedHeightMap>,
    room_record: Option<Res<crate::pds::RoomRecord>>,
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

    // Spawn translucent water cuboid at the configured visual water level.
    let water_mat = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(
                tcfg::water::COLOR[0],
                tcfg::water::COLOR[1],
                tcfg::water::COLOR[2],
                tcfg::water::COLOR[3],
            ),
            perceptual_roughness: tcfg::water::ROUGHNESS,
            metallic: tcfg::water::METALLIC,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        },
        extension: WaterExtension::default(),
    });

    let water_offset = room_record
        .as_ref()
        .map(|r| r.water_level_offset)
        .unwrap_or(0.0);
    let wl = (tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE + water_offset).max(0.001);
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(water_mat),
        Transform::from_xyz(0.0, wl / 2.0, 0.0).with_scale(Vec3::new(
            world_extent,
            wl,
            world_extent,
        )),
        WaterVolume,
    ));
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
    let hs = tcfg::HEIGHT_SCALE;
    let mapper = SplatMapper::new([
        // R — Grass: lower altitudes, gentle slopes
        SplatRule::new(
            (0.0, hs * tcfg::grass::ALT_MAX_FACTOR),
            (0.0, tcfg::grass::SLOPE_MAX),
            tcfg::grass::BLEND,
        ),
        // G — Dirt: mid-range altitude, moderate slopes
        SplatRule::new(
            (
                hs * tcfg::dirt::ALT_MIN_FACTOR,
                hs * tcfg::dirt::ALT_MAX_FACTOR,
            ),
            (0.0, tcfg::dirt::SLOPE_MAX),
            tcfg::dirt::BLEND,
        ),
        // B — Rock: steep terrain regardless of altitude (triplanar in shader)
        SplatRule::new((0.0, hs), (tcfg::rock::SLOPE_MIN, 1.0), tcfg::rock::BLEND),
        // A — Snow: high altitude, gentle slopes
        SplatRule::new(
            (hs * tcfg::snow::ALT_MIN_FACTOR, hs),
            (0.0, tcfg::snow::SLOPE_MAX),
            tcfg::snow::BLEND,
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

fn generate_terrain(seed: u64) -> HeightMap {
    let mut hm = HeightMap::new(tcfg::GRID_SIZE, tcfg::GRID_SIZE, tcfg::CELL_SCALE);

    VoronoiTerracing::new(seed, tcfg::voronoi::NUM_SEEDS, tcfg::voronoi::NUM_TERRACES)
        .generate(&mut hm);
    // normalize() is intentionally omitted: VoronoiTerracing already produces
    // bounded [0, 1] output. Only FbmNoise requires a post-generation normalize.

    for v in hm.data_mut() {
        *v *= tcfg::HEIGHT_SCALE;
    }

    // Hydraulic erosion runs first (carves the scaled terrain),
    // then thermal erosion smooths the resulting slopes.
    HydraulicErosion {
        seed,
        num_drops: tcfg::hydraulic::NUM_DROPS,
        max_steps: tcfg::hydraulic::MAX_STEPS,
        inertia: tcfg::hydraulic::INERTIA,
        erosion_rate: tcfg::hydraulic::EROSION_RATE,
        deposition_rate: tcfg::hydraulic::DEPOSITION_RATE,
        evaporation_rate: tcfg::hydraulic::EVAPORATION_RATE,
        capacity_factor: tcfg::hydraulic::CAPACITY_FACTOR,
        min_slope: tcfg::hydraulic::MIN_SLOPE,
        water_level: tcfg::hydraulic::WATER_LEVEL,
    }
    .erode(&mut hm);

    ThermalErosion::new()
        .with_iterations(tcfg::thermal::ITERATIONS)
        .with_talus_angle(tcfg::thermal::TALUS_ANGLE)
        .erode(&mut hm);

    hm
}
