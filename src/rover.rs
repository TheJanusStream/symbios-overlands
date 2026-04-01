use avian3d::prelude::*;
use bevy::prelude::*;

use crate::state::{AppState, LocalPlayer};

// --- Suspension (Hooke's law + damping) ------------------------------------
// Spring stiffness and damping are tuned for ROVER_MASS = 50 kg.
// Critical damping per corner: 2 * sqrt(k * m/4) = 2 * sqrt(1200 * 12.5) ≈ 245 Ns/m.
// We target ~0.7× critical for a slightly underdamped (responsive) feel.
const SUSPENSION_REST_LENGTH: f32 = 0.6;
const SUSPENSION_STIFFNESS: f32 = 1_200.0;
const SUSPENSION_DAMPING: f32 = 175.0;
const RAY_MAX_DIST: f32 = SUSPENSION_REST_LENGTH + 0.5;

// --- Drive -----------------------------------------------------------------
const DRIVE_FORCE: f32 = 3_000.0;
const TURN_TORQUE: f32 = 1_800.0;
/// Lateral grip: resists sideways sliding proportional to lateral velocity.
const LATERAL_GRIP: f32 = 6_000.0;

// --- Chassis ---------------------------------------------------------------
const LINEAR_DAMPING: f32 = 1.5;
const ANGULAR_DAMPING: f32 = 6.0;
const ROVER_MASS: f32 = 50.0;

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
    hm_res: Res<crate::terrain::FinishedHeightMap>,
) {
    let hm = &hm_res.0;

    // Spawn just above the terrain centre, pre-tilted to match the surface.
    let half = (hm.width() - 1) as f32 * hm.scale() * 0.5;
    let ground_y = hm.get_height_at(half, half);
    let surface_normal = hm.get_normal_at(half, half);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));

    // 1.0 m above ground gives a gentle landing with the new spring parameters.
    let start_y = ground_y + 1.0;

    let chassis = commands
        .spawn((
            Transform::from_xyz(0.0, start_y, 0.0).with_rotation(tilt),
            RigidBody::Dynamic,
            Collider::cuboid(CHASSIS_X * 2.0, CHASSIS_Y * 2.0, CHASSIS_Z * 2.0),
            Mass(ROVER_MASS),
            LinearDamping(LINEAR_DAMPING),
            AngularDamping(ANGULAR_DAMPING),
            LocalPlayer,
        ))
        .id();

    commands.entity(chassis).with_children(|parent| {
        // Main chassis visual
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                CHASSIS_X * 2.0,
                CHASSIS_Y * 2.0,
                CHASSIS_Z * 2.0,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                ..default()
            })),
            Transform::IDENTITY,
        ));

        // Sail (profile picture surface)
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(0.05, 0.8, 0.8))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.8, 0.9),
                ..default()
            })),
            Transform::from_xyz(0.0, 0.7, 0.0),
            RoverSail,
        ));
    });
}

/// Marker for the sail mesh so the avatar fetch can target it.
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

    let lin_vel = forces.linear_velocity();
    let ang_vel = forces.angular_velocity();
    let center_of_mass = global_tf.translation();

    for offset in CORNER_OFFSETS {
        let local_offset = Vec3::from_array(offset);
        let world_origin = chassis_tf.transform_point(local_offset);

        let Some(hit) =
            spatial_query.cast_ray(world_origin, Dir3::NEG_Y, RAY_MAX_DIST, true, &filter)
        else {
            continue;
        };

        let compression = SUSPENSION_REST_LENGTH - hit.distance;
        if compression > 0.0 {
            let r = world_origin - center_of_mass;
            let point_vel = lin_vel + ang_vel.cross(r);

            let spring_force = SUSPENSION_STIFFNESS * compression;
            let damping_force = -SUSPENSION_DAMPING * point_vel.y;

            let total_force = (spring_force + damping_force).max(0.0);
            forces.apply_force_at_point(Vec3::Y * total_force, world_origin);
        }
    }
}

fn apply_drive_forces(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), With<LocalPlayer>>,
) {
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let lin_vel = forces.linear_velocity();
    let forward = global_tf.forward().as_vec3();
    // Flatten to horizontal plane so driving on slopes feels natural.
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    // Steer around the rover's local up axis so turns work correctly on slopes.
    let local_up = global_tf.up().as_vec3();
    let right = global_tf.right().as_vec3();

    if keyboard.pressed(KeyCode::KeyW) {
        forces.apply_force(flat_forward * DRIVE_FORCE);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        forces.apply_force(-flat_forward * DRIVE_FORCE);
    }
    if keyboard.pressed(KeyCode::KeyA) {
        forces.apply_torque(local_up * TURN_TORQUE);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        forces.apply_torque(-local_up * TURN_TORQUE);
    }

    // Lateral grip — cancels sideways sliding proportional to lateral speed.
    let lateral_vel = right.dot(lin_vel);
    forces.apply_force(-right * lateral_vel * LATERAL_GRIP);

    // Thruster for escaping ditches (toned down from original 5000 N).
    if keyboard.pressed(KeyCode::Space) {
        forces.apply_force(Vec3::Y * 2_500.0);
    }
}
