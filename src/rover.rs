use avian3d::prelude::*;
use bevy::prelude::*;

use crate::state::{AppState, LocalPlayer};

// Suspension constants (Hooke's Law + damping).
const SUSPENSION_REST_LENGTH: f32 = 0.6;
const SUSPENSION_STIFFNESS: f32 = 800.0;
const SUSPENSION_DAMPING: f32 = 80.0;
const RAY_MAX_DIST: f32 = SUSPENSION_REST_LENGTH + 0.3;

// Drive constants.
const DRIVE_FORCE: f32 = 1200.0;
const TURN_TORQUE: f32 = 600.0;
const LINEAR_DAMPING: f32 = 1.5;
const ANGULAR_DAMPING: f32 = 4.0;

// Chassis half-extents.
const CHASSIS_X: f32 = 0.8;
const CHASSIS_Y: f32 = 0.2;
const CHASSIS_Z: f32 = 1.2;

// Corner offsets in local space for the four suspension rays.
const CORNER_OFFSETS: [[f32; 3]; 4] = [
    [CHASSIS_X, -CHASSIS_Y, CHASSIS_Z],
    [-CHASSIS_X, -CHASSIS_Y, CHASSIS_Z],
    [CHASSIS_X, -CHASSIS_Y, -CHASSIS_Z],
    [-CHASSIS_X, -CHASSIS_Y, -CHASSIS_Z],
];

pub struct RoverPlugin;

impl Plugin for RoverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_local_rover)
            .add_systems(
                FixedUpdate,
                (apply_suspension_forces, apply_drive_forces)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn spawn_local_rover(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let chassis = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(
                CHASSIS_X * 2.0,
                CHASSIS_Y * 2.0,
                CHASSIS_Z * 2.0,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                ..default()
            })),
            Transform::from_xyz(0.0, 10.0, 0.0),
            RigidBody::Dynamic,
            Collider::cuboid(CHASSIS_X * 2.0, CHASSIS_Y * 2.0, CHASSIS_Z * 2.0),
            LinearDamping(LINEAR_DAMPING),
            AngularDamping(ANGULAR_DAMPING),
            LocalPlayer,
        ))
        .id();

    // Sail (profile picture surface — child of chassis).
    let sail_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.8, 0.9),
        ..default()
    });
    commands.entity(chassis).with_children(|parent| {
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(0.05, 0.8, 0.8))),
            MeshMaterial3d(sail_material),
            Transform::from_xyz(0.0, 0.7, 0.0),
            RoverSail,
        ));
    });
}

/// Marker for the sail mesh so avatar fetch can target it.
#[derive(Component)]
pub struct RoverSail;

fn apply_suspension_forces(
    mut query: Query<(Entity, Forces, &GlobalTransform), With<LocalPlayer>>,
    spatial_query: SpatialQuery,
) {
    let Ok((chassis_entity, mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let chassis_tf = global_tf.compute_transform();
    let filter = SpatialQueryFilter::default().with_excluded_entities([chassis_entity]);

    for offset in CORNER_OFFSETS {
        let local_offset = Vec3::from_array(offset);
        let world_origin = chassis_tf.transform_point(local_offset);
        let ray_dir = Dir3::NEG_Y;

        let Some(hit) = spatial_query.cast_ray(world_origin, ray_dir, RAY_MAX_DIST, true, &filter)
        else {
            continue;
        };

        let compression = SUSPENSION_REST_LENGTH - hit.distance;
        let spring_force = SUSPENSION_STIFFNESS * compression;

        // Damping: project chassis linear velocity along the ray (world up).
        let vel = forces.linear_velocity();
        let damping_force = -SUSPENSION_DAMPING * vel.dot(Vec3::Y);

        let total_force = (spring_force + damping_force).max(0.0);
        forces.apply_force_at_point(Vec3::Y * total_force, world_origin);
    }
}

fn apply_drive_forces(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), With<LocalPlayer>>,
) {
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let forward = global_tf.forward();
    let up = Vec3::Y;

    if keyboard.pressed(KeyCode::KeyW) {
        forces.apply_force(forward * DRIVE_FORCE);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        forces.apply_force(-forward * DRIVE_FORCE);
    }
    if keyboard.pressed(KeyCode::KeyA) {
        forces.apply_torque(up * TURN_TORQUE);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        forces.apply_torque(-up * TURN_TORQUE);
    }
}
