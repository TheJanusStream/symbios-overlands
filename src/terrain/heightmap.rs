//! Async heightmap generation + the terrain mesh / collider spawner.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_symbios_ground::{
    HeightMap, HeightMapMeshBuilder, NormalMethod, build_heightfield_collider,
};

use crate::config::terrain as tcfg;
use crate::offload::{GenJob, GenResult};
use crate::pds::{SovereignGeneratorKind, SovereignTerrainConfig};
use crate::splat::{SplatExtension, SplatTerrainMaterial, SplatUniforms};
use crate::state::LiveRoomRecord;

use super::{FinishedHeightMap, OutgoingTerrain, SplatMaterialHandle, TerrainMesh, TerrainTask};

pub(super) fn start_terrain_generation(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
    time: Res<Time>,
) {
    // `find_terrain_config` walks the generator map in sorted-key order so
    // every peer compiling this record picks the same entry — `HashMap`
    // iteration is SipHash-randomised per process, and without the helper
    // two clients could generate different terrains from the same record.
    let cfg = crate::pds::find_terrain_config(&record.0)
        .cloned()
        .unwrap_or_default();

    // The heightmap is the only product of this task. Roads are re-meshed
    // separately by `roads::maybe_rebuild_roads`, which drapes over the
    // finished heightmap and reacts to road-config edits without a regen.
    // Dispatched through `offload` so the heavy noise + erosion run off the
    // schedule (native: AsyncComputeTaskPool; wasm: task pool / Web Worker).
    let task = crate::offload::offload(GenJob::Heightmap(heightmap_params(&cfg)));
    commands.insert_resource(TerrainTask(task, time.elapsed_secs_f64()));
}

pub(super) fn poll_terrain_task(
    mut commands: Commands,
    mut task_res: ResMut<TerrainTask>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    if let Some(result) =
        futures_lite::future::block_on(futures_lite::future::poll_once(&mut task_res.0))
    {
        let now = time.elapsed_secs_f64();
        let spawned_at = task_res.1;
        commands.remove_resource::<TerrainTask>();
        match result {
            GenResult::Heightmap(data) => {
                crate::diagnostics::samplers::heightmap_latency_secs(
                    &mut metrics,
                    now - spawned_at,
                );
                // Typed completion for the B-2 loading-gate heightmap distro.
                session_log.info(
                    now,
                    crate::diagnostics::event::EventPayload::HeightmapGenCompleted {
                        duration_secs: now - spawned_at,
                        width: data.width,
                        height: data.height,
                    },
                );
                commands.insert_resource(FinishedHeightMap(heightmap_from_data(data)));
            }
            // A heightmap job only ever yields a heightmap; count an unexpected
            // variant as an offload error (E-4) and leave the terrain unloaded —
            // the loading-gate stall rule surfaces it — rather than panicking.
            _ => {
                crate::diagnostics::samplers::offload_job_error(&mut metrics, now);
                warn!("heightmap offload job yielded an unexpected result — terrain will not load");
            }
        }
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

/// Distil the app's terrain config into the platform-agnostic
/// [`gen_jobs::HeightmapParams`] the offload layer runs. The generation itself
/// lives in the Bevy-free [`gen_jobs`] crate so native and the wasm Web Worker
/// share one (deterministic) implementation.
pub(super) fn heightmap_params(cfg: &SovereignTerrainConfig) -> gen_jobs::HeightmapParams {
    use gen_jobs::GeneratorKind;
    gen_jobs::HeightmapParams {
        grid_size: cfg.grid_size,
        cell_scale: cfg.cell_scale.0,
        height_scale: cfg.height_scale.0,
        generator_kind: match cfg.generator_kind {
            SovereignGeneratorKind::FbmNoise => GeneratorKind::FbmNoise,
            SovereignGeneratorKind::DiamondSquare => GeneratorKind::DiamondSquare,
            SovereignGeneratorKind::VoronoiTerracing => GeneratorKind::VoronoiTerracing,
        },
        seed: cfg.seed,
        octaves: cfg.octaves,
        persistence: cfg.persistence.0,
        lacunarity: cfg.lacunarity.0,
        base_frequency: cfg.base_frequency.0,
        ds_roughness: cfg.ds_roughness.0,
        voronoi_num_seeds: cfg.voronoi_num_seeds,
        voronoi_num_terraces: cfg.voronoi_num_terraces,
        erosion_enabled: cfg.erosion_enabled,
        erosion_drops: cfg.erosion_drops,
        inertia: cfg.inertia.0,
        erosion_rate: cfg.erosion_rate.0,
        deposition_rate: cfg.deposition_rate.0,
        evaporation_rate: cfg.evaporation_rate.0,
        capacity_factor: cfg.capacity_factor.0,
        thermal_enabled: cfg.thermal_enabled,
        thermal_iterations: cfg.thermal_iterations,
        thermal_talus_angle: cfg.thermal_talus_angle.0,
    }
}

/// Rebuild a [`HeightMap`] from the plain data returned by the offload job.
pub(super) fn heightmap_from_data(d: gen_jobs::HeightmapData) -> HeightMap {
    let mut hm = HeightMap::new(d.width as usize, d.height as usize, d.scale);
    hm.data_mut().copy_from_slice(&d.data);
    hm
}
