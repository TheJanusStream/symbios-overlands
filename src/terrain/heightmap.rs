//! Async heightmap generation + the terrain mesh / collider spawner.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::AsyncComputeTaskPool;
use bevy_symbios_ground::ThermalErosion;
use bevy_symbios_ground::{
    DiamondSquare, FbmNoise, HeightMap, HeightMapMeshBuilder, HydraulicErosion, NormalMethod,
    TerrainGenerator, VoronoiTerracing, build_heightfield_collider,
};

use crate::config::terrain as tcfg;
use crate::pds::{SovereignGeneratorKind, SovereignTerrainConfig};
use crate::splat::{SplatExtension, SplatTerrainMaterial, SplatUniforms};
use crate::state::LiveRoomRecord;

use super::{FinishedHeightMap, OutgoingTerrain, SplatMaterialHandle, TerrainMesh, TerrainTask};

pub(super) fn start_terrain_generation(mut commands: Commands, record: Res<LiveRoomRecord>) {
    // `find_terrain_config` walks the generator map in sorted-key order so
    // every peer compiling this record picks the same entry — `HashMap`
    // iteration is SipHash-randomised per process, and without the helper
    // two clients could generate different terrains from the same record.
    let cfg = crate::pds::find_terrain_config(&record.0)
        .cloned()
        .unwrap_or_default();

    let pool = AsyncComputeTaskPool::get();
    // The heightmap is the only product of this task. Roads are re-meshed
    // separately by `roads::maybe_rebuild_roads`, which drapes over the
    // finished heightmap and reacts to road-config edits without a regen.
    let task = pool.spawn(async move { generate_terrain(&cfg) });
    commands.insert_resource(TerrainTask(task));
}

pub(super) fn poll_terrain_task(mut commands: Commands, mut task_res: ResMut<TerrainTask>) {
    if let Some(hm) =
        futures_lite::future::block_on(futures_lite::future::poll_once(&mut task_res.0))
    {
        commands.remove_resource::<TerrainTask>();
        commands.insert_resource(FinishedHeightMap(hm));
    }
}

pub(super) fn spawn_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SplatTerrainMaterial>>,
    hm_res: Res<FinishedHeightMap>,
    outgoing: Query<Entity, With<OutgoingTerrain>>,
) {
    // Atomic hand-off from the previous terrain (which has been displaying
    // the player on its collider while the new heightmap generated) to the
    // freshly-spawned one. Queuing the despawn before the new-entity spawn
    // keeps the command order correct — the old colliders are gone by the
    // time physics observes a transform, and no frame ever has zero terrain
    // in the world.
    for e in &outgoing {
        commands.entity(e).try_despawn();
    }

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

    // Roads are spawned separately by `roads::maybe_rebuild_roads` (it drapes
    // over this finished heightmap and reacts to road-config edits). Water is
    // spawned by the `world_builder` module from the active record's `Water`.
}

/// Generate the heightmap for `cfg` — runs on the async compute pool.
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
            ..HydraulicErosion::new(cfg.seed)
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
