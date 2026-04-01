use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy_symbios_ground::{HeightMapMeshBuilder, NormalMethod, build_heightfield_collider};
use symbios_ground::{FbmNoise, HeightMap, HydraulicErosion, TerrainGenerator, ThermalErosion};

use crate::state::AppState;

const TERRAIN_SEED: u64 = 42;
const GRID_SIZE: usize = 257;
const CELL_SCALE: f32 = 1.0;
const HEIGHT_SCALE: f32 = 30.0;
const EROSION_DROPS: u32 = 80_000;

#[derive(Component)]
pub struct TerrainMesh;

#[derive(Resource)]
pub struct FinishedHeightMap(pub HeightMap);

#[derive(Resource)]
pub struct TerrainTask(pub bevy::tasks::Task<HeightMap>);

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Loading), start_terrain_generation)
            .add_systems(
                Update,
                poll_terrain_task.run_if(in_state(AppState::Loading)),
            )
            .add_systems(OnEnter(AppState::InGame), spawn_terrain_mesh);
    }
}

fn start_terrain_generation(mut commands: Commands) {
    let pool = AsyncComputeTaskPool::get();
    let task = pool.spawn(async move { generate_terrain() });
    commands.insert_resource(TerrainTask(task));
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

fn spawn_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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

    // The collider is inherently centered at the local origin.
    // We attach the visual mesh as a child, offsetting it by -half so it aligns.
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
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.35, 0.55, 0.25),
                    perceptual_roughness: 0.9,
                    ..default()
                })),
                Transform::from_xyz(-half, 0.0, -half),
            ));
        });
}

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
