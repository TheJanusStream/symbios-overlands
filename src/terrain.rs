use avian3d::prelude::*;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension};
use bevy::tasks::AsyncComputeTaskPool;
use bevy_symbios_ground::{
    HeightMapMeshBuilder, NormalMethod, build_heightfield_collider, splat_to_image,
};
use bevy_symbios_texture::async_gen::{PendingTexture, TextureReady};
use bevy_symbios_texture::ground::GroundConfig;
use bevy_symbios_texture::rock::RockConfig;
use bevy_symbios_texture::SymbiosTexturePlugin;
use symbios_ground::{FbmNoise, HeightMap, HydraulicErosion, SplatMapper, SplatRule, TerrainGenerator, ThermalErosion};

use crate::splat::{SplatExtension, SplatTerrainMaterial, SplatUniforms};
use crate::state::AppState;

const TERRAIN_SEED: u64 = 42;
const GRID_SIZE: usize = 257;
const CELL_SCALE: f32 = 1.0;
const HEIGHT_SCALE: f32 = 30.0;
const EROSION_DROPS: u32 = 80_000;
/// How many times the tiling textures repeat across the terrain.
const TILE_SCALE: f32 = 64.0;
/// Resolution of each procedurally generated texture layer.
const TEXTURE_SIZE: u32 = 512;

#[derive(Component)]
pub struct TerrainMesh;

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
            .init_resource::<TerrainSplatState>()
            .add_systems(
                OnEnter(AppState::Loading),
                (start_terrain_generation, start_texture_tasks),
            )
            .add_systems(
                Update,
                poll_terrain_task.run_if(in_state(AppState::Loading)),
            )
            .add_systems(OnEnter(AppState::InGame), spawn_terrain_mesh)
            .add_systems(
                Update,
                (collect_texture_results, apply_splat_textures)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ---------------------------------------------------------------------------
// Loading state — terrain generation + async texture tasks
// ---------------------------------------------------------------------------

fn start_terrain_generation(mut commands: Commands) {
    let pool = AsyncComputeTaskPool::get();
    let task = pool.spawn(async move { generate_terrain() });
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
                seed: 1,
                macro_scale: 2.5,
                macro_octaves: 4,
                micro_scale: 10.0,
                micro_octaves: 3,
                micro_weight: 0.3,
                color_dry: [0.30, 0.48, 0.15],
                color_moist: [0.14, 0.28, 0.07],
                normal_strength: 1.5,
            },
            TEXTURE_SIZE,
            TEXTURE_SIZE,
        ),
        TextureLayerIndex(0),
    ));

    // Layer 1 — Dirt (default brownish soil)
    commands.spawn((
        PendingTexture::ground(GroundConfig::default(), TEXTURE_SIZE, TEXTURE_SIZE),
        TextureLayerIndex(1),
    ));

    // Layer 2 — Rock (ridged multifractal stone)
    commands.spawn((
        PendingTexture::rock(RockConfig::default(), TEXTURE_SIZE, TEXTURE_SIZE),
        TextureLayerIndex(2),
    ));

    // Layer 3 — Snow (pale, low-relief ground cover)
    commands.spawn((
        PendingTexture::ground(
            GroundConfig {
                seed: 99,
                macro_scale: 4.0,
                macro_octaves: 3,
                micro_scale: 12.0,
                micro_octaves: 3,
                micro_weight: 0.4,
                color_dry: [0.95, 0.95, 0.98],
                color_moist: [0.80, 0.82, 0.88],
                normal_strength: 0.8,
            },
            TEXTURE_SIZE,
            TEXTURE_SIZE,
        ),
        TextureLayerIndex(3),
    ));
}

fn poll_terrain_task(
    mut commands: Commands,
    mut task_res: ResMut<TerrainTask>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let Some(hm) =
        futures_lite::future::block_on(futures_lite::future::poll_once(&mut task_res.0))
    {
        commands.remove_resource::<TerrainTask>();
        commands.insert_resource(FinishedHeightMap(hm));
        next_state.set(AppState::InGame);
    }
}

// ---------------------------------------------------------------------------
// InGame state — mesh + material, then apply splat when textures arrive
// ---------------------------------------------------------------------------

fn spawn_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
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

    // Material starts disabled (flat colour) until the texture tasks finish.
    let mat_handle = materials.add(bevy::pbr::ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.35, 0.55, 0.25),
            perceptual_roughness: 0.9,
            ..default()
        },
        extension: SplatExtension {
            uniforms: SplatUniforms {
                tile_scale: TILE_SCALE,
                enabled: 0,
                triplanar_scale: TILE_SCALE / world_extent.max(1.0),
                triplanar_sharpness: 4.0,
            },
            ..default()
        },
    });
    commands.insert_resource(SplatMaterialHandle(mat_handle.clone()));

    commands
        .spawn((
            Transform::IDENTITY,
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
    let hs = HEIGHT_SCALE;
    let mapper = SplatMapper::new([
        // R — Grass: lower altitudes, gentle slopes
        SplatRule::new((0.0, hs * 0.45), (0.0, 0.30), 4.0),
        // G — Dirt: mid-range altitude, moderate slopes
        SplatRule::new((hs * 0.30, hs * 0.65), (0.0, 0.55), 2.0),
        // B — Rock: steep terrain regardless of altitude (triplanar in shader)
        SplatRule::new((0.0, hs), (0.25, 1.0), 3.0),
        // A — Snow: high altitude, gentle slopes
        SplatRule::new((hs * 0.68, hs), (0.0, 0.35), 4.0),
    ]);
    let weight_map = mapper.generate(hm);
    // `splat_to_image` gives us an RGBA8 image with ClampToEdge sampler —
    // correct for a full-terrain weight map that must not tile.
    let wm_handle = images.add(splat_to_image(&weight_map));

    if let Some(mat) = materials.get_mut(&splat_mat.0) {
        mat.base.base_color = Color::WHITE;
        mat.base.perceptual_roughness = 0.85;
        mat.base.metallic = 0.0;
        mat.extension.weight_map = wm_handle;
        mat.extension.albedo_array = albedo_array;
        mat.extension.normal_array = normal_array;
        mat.extension.uniforms.enabled = 1;
        mat.extension.uniforms.triplanar_scale = TILE_SCALE / world_extent.max(1.0);
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
    let first = images.get(handles[0].as_ref()?)?;
    let w = first.texture_descriptor.size.width;
    let h = first.texture_descriptor.size.height;
    let format = first.texture_descriptor.format;
    let mip_count = first.texture_descriptor.mip_level_count;

    // Concatenate every layer's full mipchain in order.
    let mut merged: Vec<u8> = Vec::new();
    for handle_opt in handles {
        let img = images.get(handle_opt.as_ref()?)?;
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

fn generate_terrain() -> HeightMap {
    let mut hm = HeightMap::new(GRID_SIZE, GRID_SIZE, CELL_SCALE);

    FbmNoise {
        seed: TERRAIN_SEED,
        octaves: 6,
        persistence: 0.5,
        lacunarity: 2.0,
        base_frequency: 0.003,
    }
    .generate(&mut hm);

    hm.normalize();

    for v in hm.data_mut() {
        *v *= HEIGHT_SCALE;
    }

    ThermalErosion::new()
        .with_iterations(50)
        .with_talus_angle(0.6)
        .erode(&mut hm);

    HydraulicErosion {
        seed: TERRAIN_SEED,
        num_drops: EROSION_DROPS,
        ..HydraulicErosion::new(TERRAIN_SEED)
    }
    .erode(&mut hm);

    hm
}
