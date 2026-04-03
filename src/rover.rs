use avian3d::prelude::*;
use bevy::prelude::*;
use crate::avatar::{AvatarMaterial, NeedsAvatarReapply};
use crate::config::airship as ac;
use crate::config::rover as cfg;
use crate::protocol::AirshipParams;
use crate::state::{AppState, LocalAirshipParams, LocalPlayer};

// Corner offsets in local space for the four suspension rays (derived from chassis half-extents).
const CORNER_OFFSETS: [[f32; 3]; 4] = [
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
];

/// Marker placed on the solar-sail mesh child so the avatar system can find it.
#[derive(Component)]
pub struct RoverSail;

pub struct RoverPlugin;

impl Plugin for RoverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_local_rover)
            .add_systems(
                Update,
                rebuild_local_rover.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    apply_suspension_forces,
                    apply_drive_forces,
                    apply_uprighting_force,
                    respawn_if_fallen,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ---------------------------------------------------------------------------
// Spawn / rebuild helpers
// ---------------------------------------------------------------------------

/// Build the steampunk-airship visual children of `entity`.
/// Pass `existing_children` when rebuilding so old children are despawned first.
pub fn rebuild_airship_children(
    commands: &mut Commands,
    entity: Entity,
    params: &AirshipParams,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    let chassis_half_y = cfg::CHASSIS_Y; // 0.2 m — top surface of physics hull
    let hull_h = chassis_half_y * 2.0;
    let hull_w = params.hull_width;
    let hull_l = params.hull_length;
    let mast_h = params.mast_height;
    let mast_top_y = chassis_half_y + mast_h;

    let [hr, hg, hb] = params.hull_color;
    let [pr, pg, pb] = params.pontoon_color;
    let [mr, mg, mb] = ac::MAST_COLOR;

    let hull_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(hr, hg, hb),
        metallic: params.metallic,
        perceptual_roughness: params.roughness,
        ..default()
    });
    let pontoon_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(pr, pg, pb),
        metallic: params.metallic * 0.5,
        perceptual_roughness: (params.roughness + 0.15).min(1.0),
        ..default()
    });
    let mast_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(mr, mg, mb),
        metallic: 0.75,
        perceptual_roughness: 0.35,
        ..default()
    });
    let sail_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.82, 0.92),
        ..default()
    });

    commands.entity(entity).with_children(|parent| {
        // Main hull
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(hull_w, hull_h, hull_l))),
            MeshMaterial3d(hull_mat.clone()),
            Transform::IDENTITY,
        ));

        // Port outrigger pontoon (−X)
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                ac::PONTOON_SIZE,
                ac::PONTOON_SIZE,
                params.pontoon_length,
            ))),
            MeshMaterial3d(pontoon_mat.clone()),
            Transform::from_xyz(-params.pontoon_spread, 0.0, 0.0),
        ));

        // Starboard outrigger pontoon (+X)
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                ac::PONTOON_SIZE,
                ac::PONTOON_SIZE,
                params.pontoon_length,
            ))),
            MeshMaterial3d(pontoon_mat),
            Transform::from_xyz(params.pontoon_spread, 0.0, 0.0),
        ));

        // Forward cross-strut
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                params.pontoon_spread * 2.0,
                ac::STRUT_THICKNESS,
                ac::STRUT_THICKNESS,
            ))),
            MeshMaterial3d(hull_mat.clone()),
            Transform::from_xyz(0.0, 0.0, hull_l * 0.3),
        ));

        // Aft cross-strut
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                params.pontoon_spread * 2.0,
                ac::STRUT_THICKNESS,
                ac::STRUT_THICKNESS,
            ))),
            MeshMaterial3d(hull_mat),
            Transform::from_xyz(0.0, 0.0, -hull_l * 0.3),
        ));

        // Central mast
        parent.spawn((
            Mesh3d(meshes.add(Cylinder::new(ac::MAST_RADIUS, mast_h))),
            MeshMaterial3d(mast_mat),
            Transform::from_xyz(0.0, chassis_half_y + mast_h * 0.5, 0.0),
        ));

        // Solar sail — thin in Z, square in XY so the profile picture faces
        // forward/backward.  Sits just below the mast tip.
        parent.spawn((
            Mesh3d(meshes.add(Cuboid::new(
                params.sail_size,
                params.sail_size,
                ac::SAIL_THICKNESS,
            ))),
            MeshMaterial3d(sail_mat),
            Transform::from_xyz(0.0, mast_top_y - params.sail_size * 0.5, 0.0),
            RoverSail,
        ));
    });
}

fn spawn_local_rover(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    hm_res: Res<crate::terrain::FinishedHeightMap>,
    params: Res<LocalAirshipParams>,
) {
    let hm = &hm_res.0;
    let half = (hm.width() - 1) as f32 * hm.scale() * 0.5;
    let ground_y = hm.get_height_at(half, half);
    let surface_normal = hm.get_normal_at(half, half);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));

    let chassis = commands
        .spawn((
            Transform::from_xyz(0.0, ground_y + cfg::SPAWN_HEIGHT_OFFSET, 0.0)
                .with_rotation(tilt),
            Visibility::default(),
            RigidBody::Dynamic,
            Collider::cuboid(cfg::CHASSIS_X * 2.0, cfg::CHASSIS_Y * 2.0, cfg::CHASSIS_Z * 2.0),
            Mass(cfg::MASS),
            LinearDamping(cfg::LINEAR_DAMPING),
            AngularDamping(cfg::ANGULAR_DAMPING),
            LocalPlayer,
        ))
        .id();

    rebuild_airship_children(
        &mut commands,
        chassis,
        &params.params,
        None,
        &mut meshes,
        &mut materials,
    );
}

fn rebuild_local_rover(
    mut commands: Commands,
    mut ap: ResMut<LocalAirshipParams>,
    query: Query<(Entity, Option<&Children>, Option<&AvatarMaterial>), With<LocalPlayer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !ap.needs_rebuild {
        return;
    }
    ap.needs_rebuild = false;

    let Ok((entity, children, avatar_mat)) = query.single() else {
        return;
    };

    rebuild_airship_children(
        &mut commands,
        entity,
        &ap.params.clone(),
        children,
        &mut meshes,
        &mut materials,
    );

    // If an avatar was already fetched, schedule a re-apply to the new sail
    // child rather than triggering a redundant network request.
    if avatar_mat.is_some() {
        commands.entity(entity).insert(NeedsAvatarReapply);
    }
    // If no AvatarMaterial yet, the in-flight fetch (started by
    // fetch_local_avatar on spawn) will apply to the new sail once done.
}

// ---------------------------------------------------------------------------
// Physics systems (hover-craft suspension, unchanged)
// ---------------------------------------------------------------------------

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

fn apply_uprighting_force(mut query: Query<(Forces, &GlobalTransform), With<LocalPlayer>>) {
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    let vehicle_up = global_tf.up().as_vec3();
    forces.apply_torque(vehicle_up.cross(Vec3::Y) * cfg::UPRIGHTING_TORQUE);
}

fn respawn_if_fallen(
    mut query: Query<
        (&mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity),
        With<LocalPlayer>,
    >,
    hm_res: Res<crate::terrain::FinishedHeightMap>,
) {
    let Ok((mut pos, mut rot, mut lin_vel, mut ang_vel)) = query.single_mut() else {
        return;
    };
    if pos.y > cfg::FALL_Y_THRESHOLD {
        return;
    }
    let hm = &hm_res.0;
    let half = (hm.width() - 1) as f32 * hm.scale() * 0.5;
    let ground_y = hm.get_height_at(half, half);
    let surface_normal = hm.get_normal_at(half, half);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
    pos.0 = Vec3::new(0.0, ground_y + cfg::SPAWN_HEIGHT_OFFSET, 0.0);
    rot.0 = tilt;
    lin_vel.0 = Vec3::ZERO;
    ang_vel.0 = Vec3::ZERO;
}
