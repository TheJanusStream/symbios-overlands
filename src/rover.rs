use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::state::{AppState, LocalPlayer};

// Corner offsets in local space for the four suspension rays (derived from chassis half-extents).
const CORNER_OFFSETS: [[f32; 3]; 4] = [
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
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

    let start_y = ground_y + cfg::SPAWN_HEIGHT_OFFSET;

    let chassis = commands
        .spawn((
            Transform::from_xyz(0.0, start_y, 0.0).with_rotation(tilt),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RigidBody::Dynamic,
            Collider::cuboid(cfg::CHASSIS_X * 2.0, cfg::CHASSIS_Y * 2.0, cfg::CHASSIS_Z * 2.0),
            Mass(cfg::MASS),
            LinearDamping(cfg::LINEAR_DAMPING),
            AngularDamping(cfg::ANGULAR_DAMPING),
            LocalPlayer,
        ))
        .id();

    commands.entity(chassis).with_children(|parent| {
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                cfg::CHASSIS_X * 2.0,
                cfg::CHASSIS_Y * 2.0,
                cfg::CHASSIS_Z * 2.0,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                ..default()
            })),
            Transform::IDENTITY,
        ));

        // Sail (profile picture surface)
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(cfg::SAIL_THICKNESS, cfg::SAIL_SIZE, cfg::SAIL_SIZE))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.8, 0.9),
                ..default()
            })),
            Transform::from_xyz(0.0, cfg::SAIL_OFFSET_Y, 0.0),
            RoverSail,
        ));
    });
}

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
            spatial_query.cast_ray(world_origin, Dir3::NEG_Y, cfg::RAY_MAX_DIST, true, &filter)
        else {
            continue;
        };

        let compression = cfg::SUSPENSION_REST_LENGTH - hit.distance;
        if compression > 0.0 {
            let r = world_origin - center_of_mass;
            let point_vel = lin_vel + ang_vel.cross(r);

            let spring_force = cfg::SUSPENSION_STIFFNESS * compression;
            let damping_force = -cfg::SUSPENSION_DAMPING * point_vel.y;

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
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let local_up = global_tf.up().as_vec3();
    let right = global_tf.right().as_vec3();

    if keyboard.pressed(KeyCode::KeyW) {
        forces.apply_force(flat_forward * cfg::DRIVE_FORCE);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        forces.apply_force(-flat_forward * cfg::DRIVE_FORCE);
    }
    if keyboard.pressed(KeyCode::KeyA) {
        forces.apply_torque(local_up * cfg::TURN_TORQUE);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        forces.apply_torque(-local_up * cfg::TURN_TORQUE);
    }

    let lateral_vel = right.dot(lin_vel);
    forces.apply_force(-right * lateral_vel * cfg::LATERAL_GRIP);

    if keyboard.pressed(KeyCode::Space) {
        forces.apply_force(Vec3::Y * cfg::JUMP_FORCE);
    }
}
