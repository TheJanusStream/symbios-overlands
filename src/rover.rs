//! Local rover plugin: airship visual construction, raycast suspension,
//! keyboard drive, buoyancy, and automatic respawn.
//!
//! The vessel is a single `RigidBody::Dynamic` cuboid chassis decorated with
//! non-colliding visual children (hull, pontoons, mast, sail, struts).  Four
//! downward raycasts from the chassis corners implement a Hooke's-law spring
//! with damping projected onto the contact normal so horizontal travel across
//! a slope contributes zero damping force.  When the chassis origin dips
//! below the buoyancy equilibrium altitude the same physics system switches
//! to Archimedean lift, turning the rover into a raft seamlessly.

use crate::avatar::AvatarMaterial;
use crate::config::airship as ac;
use crate::config::rover as cfg;
use crate::config::terrain as tcfg;
use crate::protocol::{AirshipParams, PontoonShape};
use crate::state::{AppState, LocalAirshipParams, LocalPhysicsParams, LocalPlayer};
use avian3d::prelude::*;
use bevy::prelude::*;

// Corner offsets in local space for the four suspension rays (derived from chassis half-extents).
const CORNER_OFFSETS: [[f32; 3]; 4] = [
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
];

/// Draw an (x, z) pair uniformly distributed inside a square of
/// `SPAWN_SCATTER_SIZE` metres per side, centred on the origin.  Successive
/// calls yield different positions via a splitmix64 PRNG stored in process
/// memory, so every spawn and respawn within a session lands somewhere new.
fn random_spawn_xz() -> (f32, f32) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let s = SEED.fetch_add(0xDA94_2042_E4DD_58B5, Ordering::Relaxed);
    // splitmix64 finaliser
    let mut z = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let u = (z as u32 as f32) / (u32::MAX as f32);
    let v = ((z >> 32) as u32 as f32) / (u32::MAX as f32);
    let side = cfg::SPAWN_SCATTER_SIZE;
    ((u - 0.5) * side, (v - 0.5) * side)
}

/// Visual water-plane altitude used by both terrain rendering and the
/// swimming/buoyancy system so the two stay in perfect agreement.
#[inline]
pub fn water_level_y() -> f32 {
    (tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE).max(0.001)
}

/// Marker placed on the solar-sail mesh child so the avatar system can find it.
#[derive(Component)]
pub struct RoverSail;

/// Marker placed on the mast-tip hemisphere child so the social-resonance
/// system can light it up when a peer is a mutual follow.
#[derive(Component)]
pub struct MastTip;

pub struct RoverPlugin;

impl Plugin for RoverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_local_rover)
            .add_systems(
                Update,
                (rebuild_local_rover, lift_rover_above_new_ground)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    sync_chassis_physics,
                    apply_suspension_forces,
                    apply_buoyancy_forces,
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
    avatar_override: Option<&Handle<StandardMaterial>>,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    let hull_l = params.hull_length;
    let hull_w = params.hull_width;
    let mast_h = params.mast_height;
    let [mx, mz] = params.mast_offset;
    // Mast base sits on the deck rim (y = 0 in hull-mesh space).
    let mast_top_y = mast_h;
    // Strut & pontoon vertical drop as fraction of hull keel depth.
    let drop_y = -params.strut_drop * params.hull_depth;

    let [hr, hg, hb] = params.hull_color;
    let [pr, pg, pb] = params.pontoon_color;
    let [mr, mg, mb] = params.mast_color;
    let [sr, sg, sb] = params.strut_color;

    // Hull material is double-sided so the concave V interior is visible from above.
    let hull_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(hr, hg, hb),
        metallic: params.metallic,
        perceptual_roughness: params.roughness,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let pontoon_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(pr, pg, pb),
        metallic: params.metallic * 0.5,
        perceptual_roughness: (params.roughness + 0.15).min(1.0),
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let mast_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(mr, mg, mb),
        metallic: 0.75,
        perceptual_roughness: 0.35,
        ..default()
    });
    let strut_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(sr, sg, sb),
        metallic: params.metallic * 0.7,
        perceptual_roughness: (params.roughness + 0.1).min(1.0),
        ..default()
    });
    // Sail is double-sided so the avatar face shows from both port and starboard.
    let sail_mat = avatar_override.cloned().unwrap_or_else(|| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.82, 0.92),
            double_sided: true,
            cull_mode: None,
            ..default()
        })
    });

    commands.entity(entity).with_children(|parent| {
        // Main hull — V cross-section extruded along Z with sinusoidal taper.
        // Deck rim fixed at y = 0; keel dips to -hull_depth at midship.
        parent.spawn((
            Mesh3d(meshes.add(build_v_hull_mesh(hull_l, hull_w, params.hull_depth))),
            MeshMaterial3d(hull_mat.clone()),
            Transform::IDENTITY,
        ));

        // Outrigger pontoons — shape selected by the player.
        let pontoon_mesh = match params.pontoon_shape {
            PontoonShape::Capsule => meshes.add(Capsule3d::new(
                params.pontoon_width / 2.0,
                params.pontoon_length,
            )),
            PontoonShape::VHull => meshes.add(build_v_hull_mesh(
                params.pontoon_length,
                params.pontoon_width,
                params.pontoon_height,
            )),
        };
        // Capsules need a 90° rotation to align along Z; V-hull meshes already run along Z.
        let pontoon_rot = match params.pontoon_shape {
            PontoonShape::Capsule => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            PontoonShape::VHull => Quat::IDENTITY,
        };

        // Port outrigger pontoon (−X).
        parent.spawn((
            Mesh3d(pontoon_mesh.clone()),
            MeshMaterial3d(pontoon_mat.clone()),
            Transform::from_xyz(-params.pontoon_spread, drop_y, 0.0).with_rotation(pontoon_rot),
        ));

        // Starboard outrigger pontoon (+X).
        parent.spawn((
            Mesh3d(pontoon_mesh),
            MeshMaterial3d(pontoon_mat),
            Transform::from_xyz(params.pontoon_spread, drop_y, 0.0).with_rotation(pontoon_rot),
        ));

        // Forward cross-strut — capsule aligned along X.
        let strut_mesh = meshes.add(Capsule3d::new(
            ac::STRUT_THICKNESS * 0.5,
            params.pontoon_spread * 2.0,
        ));
        parent.spawn((
            Mesh3d(strut_mesh.clone()),
            MeshMaterial3d(strut_mat.clone()),
            Transform::from_xyz(0.0, drop_y, hull_l * 0.3)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ));

        // Aft cross-strut — capsule aligned along X.
        parent.spawn((
            Mesh3d(strut_mesh),
            MeshMaterial3d(strut_mat),
            Transform::from_xyz(0.0, drop_y, -hull_l * 0.3)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ));

        // Central mast — base sits on the deck rim (y = 0).
        let mast_r = params.mast_radius;
        parent.spawn((
            Mesh3d(meshes.add(Cylinder::new(mast_r, mast_h))),
            MeshMaterial3d(mast_mat.clone()),
            Transform::from_xyz(mx, mast_h * 0.5, mz),
        ));

        // Mast tip cap — hemisphere sitting on top of the mast cylinder.
        parent.spawn((
            Mesh3d(meshes.add(Sphere::new(mast_r).mesh().uv(16, 8))),
            MeshMaterial3d(mast_mat),
            Transform::from_xyz(mx, mast_h, mz),
            MastTip,
        ));

        // Solar sail — flat double-sided panel oriented as a flag streaming aft.
        // The panel is in the YZ plane (faces ±X) so the avatar face is visible
        // from the sides.  Hoist edge at the mast, trailing edge streams along +Z.
        parent.spawn((
            Mesh3d(meshes.add(Rectangle::new(params.sail_size, params.sail_size))),
            MeshMaterial3d(sail_mat),
            Transform::from_xyz(
                mx,
                mast_top_y - params.sail_size * 0.5,
                mz + params.sail_size * 0.5,
            )
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            RoverSail,
        ));
    });
}

/// Build a smooth V-hull mesh extruded along Z.
///
/// Cross-section at each station: two rim points at `(±r, 0)` and a keel point
/// at `(0, −depth)`.  Both `r` and `depth` follow a sine envelope along the
/// hull length so bow and stern taper to a point while the deck rim stays at
/// y = 0.  The mesh has three panels per segment: port side, starboard side,
/// and the flat deck strip across the top.
fn build_v_hull_mesh(hull_length: f32, hull_width: f32, hull_depth: f32) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    const SEGMENTS: usize = 20;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((SEGMENTS + 1) * 3);
    let mut indices: Vec<u32> = Vec::with_capacity(SEGMENTS * 12);

    for i in 0..=SEGMENTS {
        let t = i as f32 / SEGMENTS as f32;
        let z = -hull_length * 0.5 + t * hull_length;
        // Sine envelope: zero at bow/stern, one at midship.
        let scale = (t * std::f32::consts::PI).sin();
        let r = (hull_width * 0.5) * scale;
        let keel_y = -hull_depth * scale;
        // Three vertices per cross-section: port rim, keel, starboard rim.
        positions.push([-r, 0.0, z]); // 3i+0  port rim
        positions.push([0.0, keel_y, z]); // 3i+1  keel
        positions.push([r, 0.0, z]); // 3i+2  starboard rim
    }

    for i in 0..SEGMENTS {
        let l0 = (i * 3) as u32; // port rim      station i
        let k0 = l0 + 1; // keel           station i
        let r0 = l0 + 2; // starboard rim  station i
        let l1 = ((i + 1) * 3) as u32; // port rim      station i+1
        let k1 = l1 + 1; // keel           station i+1
        let r1 = l1 + 2; // starboard rim  station i+1

        // Port panel — outward normal faces (−X, −Y).
        indices.extend_from_slice(&[l0, k0, k1]);
        indices.extend_from_slice(&[l0, k1, l1]);

        // Starboard panel — outward normal faces (+X, −Y).
        indices.extend_from_slice(&[k0, r0, r1]);
        indices.extend_from_slice(&[k0, r1, k1]);

        // Deck strip — outward normal faces +Y.
        indices.extend_from_slice(&[l0, l1, r1]);
        indices.extend_from_slice(&[l0, r1, r0]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

fn spawn_local_rover(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    hm_res: Res<crate::terrain::FinishedHeightMap>,
    params: Res<LocalAirshipParams>,
) {
    let hm = &hm_res.0;
    let centre = (hm.width() - 1) as f32 * hm.scale() * 0.5;
    // Heightmap is sampled in its own coordinate space where (centre, centre)
    // is the world origin; offsets from the 10×10 m scatter square are added
    // directly to both the sampling coordinates and the world position.
    let (ox, oz) = random_spawn_xz();
    let ground_y = hm.get_height_at(centre + ox, centre + oz);
    let surface_normal = hm.get_normal_at(centre + ox, centre + oz);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));

    let chassis = commands
        .spawn((
            Transform::from_xyz(ox, ground_y + cfg::SPAWN_HEIGHT_OFFSET, oz).with_rotation(tilt),
            Visibility::default(),
            RigidBody::Dynamic,
            Collider::cuboid(
                cfg::CHASSIS_X * 2.0,
                cfg::CHASSIS_Y * 2.0,
                cfg::CHASSIS_Z * 2.0,
            ),
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
        None,
    );
}

#[allow(clippy::type_complexity)]
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
        avatar_mat.map(|m| &m.0),
    );
}

// ---------------------------------------------------------------------------
// Physics systems
// ---------------------------------------------------------------------------

/// Push mass and damping changes from the GUI resource onto the ECS components.
fn sync_chassis_physics(
    pp: Res<LocalPhysicsParams>,
    mut query: Query<(&mut Mass, &mut LinearDamping, &mut AngularDamping), With<LocalPlayer>>,
) {
    let Ok((mut mass, mut lin_damp, mut ang_damp)) = query.single_mut() else {
        return;
    };
    if mass.0 != pp.mass {
        mass.0 = pp.mass;
    }
    if lin_damp.0 != pp.linear_damping {
        lin_damp.0 = pp.linear_damping;
    }
    if ang_damp.0 != pp.angular_damping {
        ang_damp.0 = pp.angular_damping;
    }
}

fn apply_suspension_forces(
    pp: Res<LocalPhysicsParams>,
    mut query: Query<(Entity, Forces, &GlobalTransform), With<LocalPlayer>>,
    spatial_query: SpatialQuery,
) {
    let Ok((chassis_entity, mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let ray_max = pp.suspension_rest_length + 1.5;
    let chassis_tf = global_tf.compute_transform();
    let filter = SpatialQueryFilter::default().with_excluded_entities([chassis_entity]);
    let lin_vel = forces.linear_velocity();
    let ang_vel = forces.angular_velocity();
    let center_of_mass = global_tf.translation();

    for offset in CORNER_OFFSETS {
        let local_offset = Vec3::from_array(offset);
        let world_origin = chassis_tf.transform_point(local_offset);

        let Some(hit) = spatial_query.cast_ray(world_origin, Dir3::NEG_Y, ray_max, true, &filter)
        else {
            continue;
        };

        let compression = pp.suspension_rest_length - hit.distance;
        if compression > 0.0 {
            let r = world_origin - center_of_mass;
            let point_vel = lin_vel + ang_vel.cross(r);
            // Damping must oppose the *compression rate* of the spring, not
            // world-space vertical motion.  Project the contact-point velocity
            // onto the terrain contact normal so horizontal travel across a
            // slope — where `point_vel.y` is large but the spring length is
            // effectively constant — produces zero damping force.
            let closing_speed = -point_vel.dot(hit.normal);
            let spring_force = pp.suspension_stiffness * compression;
            let damping_force = pp.suspension_damping * closing_speed;
            let total_force = (spring_force + damping_force).max(0.0);
            forces.apply_force_at_point(Vec3::Y * total_force, world_origin);
        }
    }
}

fn apply_drive_forces(
    pp: Res<LocalPhysicsParams>,
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
        forces.apply_force(flat_forward * pp.drive_force);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        forces.apply_force(-flat_forward * pp.drive_force);
    }
    if keyboard.pressed(KeyCode::KeyA) {
        forces.apply_torque(local_up * pp.turn_torque);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        forces.apply_torque(-local_up * pp.turn_torque);
    }

    let lateral_vel = right.dot(lin_vel);
    forces.apply_force(-right * lateral_vel * pp.lateral_grip);

    if keyboard.pressed(KeyCode::Space) {
        forces.apply_force(Vec3::Y * pp.jump_force);
    }
}

fn apply_uprighting_force(
    pp: Res<LocalPhysicsParams>,
    mut query: Query<(Forces, &GlobalTransform), With<LocalPlayer>>,
) {
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    let vehicle_up = global_tf.up().as_vec3();
    forces.apply_torque(vehicle_up.cross(Vec3::Y) * pp.uprighting_torque);
}

/// Archimedean buoyancy — an upward force proportional to depth below the
/// buoyancy equilibrium altitude (visual water level, the owner-configured
/// room offset, and `WATER_REST_LENGTH` combined), plus a vertical-velocity
/// drag term.  The rest length holds the chassis hovering slightly above the
/// visual surface, so the force engages as soon as the origin dips below that
/// target altitude rather than waiting for the vessel to fully submerge.
fn apply_buoyancy_forces(
    pp: Res<LocalPhysicsParams>,
    room_record: Option<Res<crate::pds::RoomRecord>>,
    mut query: Query<(Forces, &GlobalTransform), With<LocalPlayer>>,
) {
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    // Pick the Water generator with the lexicographically smallest key so
    // every peer computes the same buoyancy equilibrium — the raw
    // `HashMap::values()` iteration order is process-random (SipHash).
    let water_offset: f32 = room_record
        .as_ref()
        .and_then(|r| {
            let mut keys: Vec<&String> = r.generators.keys().collect();
            keys.sort();
            for k in keys {
                if let Some(crate::pds::Generator::Water { level_offset }) = r.generators.get(k) {
                    return Some(level_offset.0);
                }
            }
            None
        })
        .unwrap_or(0.0);
    let wl = water_level_y() + water_offset + pp.water_rest_length;
    let y = global_tf.translation().y;
    let depth = (wl - y).clamp(0.0, pp.buoyancy_max_depth);
    if depth <= 0.0 {
        return;
    }
    let lin_vel = forces.linear_velocity();
    let lift = pp.buoyancy_strength * depth;
    let drag = -pp.buoyancy_damping * lin_vel.y;
    forces.apply_force(Vec3::Y * (lift + drag));
}

/// When the room owner edits terrain parameters mid-session, `terrain.rs`
/// tears down the old heightfield collider and rebuilds it once the new
/// heightmap finishes generating. During that gap the rigid body is in
/// free-fall, and when the new collider spawns the chassis can wind up
/// visibly embedded in (or below) the fresh terrain surface. This system
/// detects the moment a new `FinishedHeightMap` resource is inserted and
/// snaps the chassis to `ground_y + SPAWN_HEIGHT_OFFSET` if it is sitting
/// below that altitude, zeroing linear velocity so the raycast suspension
/// can take over cleanly on the next physics tick.
fn lift_rover_above_new_ground(
    hm_res: Option<Res<crate::terrain::FinishedHeightMap>>,
    mut query: Query<(&mut Position, &mut LinearVelocity, &mut AngularVelocity), With<LocalPlayer>>,
) {
    let Some(hm_res) = hm_res else {
        return;
    };
    // Only act on the tick the resource was just (re-)inserted — otherwise
    // this would tug the vehicle upward every frame.
    if !hm_res.is_added() {
        return;
    }
    let Ok((mut pos, mut lin_vel, mut ang_vel)) = query.single_mut() else {
        return;
    };
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let hm_x = (pos.x + half).clamp(0.0, extent);
    let hm_z = (pos.z + half).clamp(0.0, extent);
    let ground_y = hm.get_height_at(hm_x, hm_z);
    let min_y = ground_y + cfg::SPAWN_HEIGHT_OFFSET;
    if pos.y < min_y {
        pos.y = min_y;
        lin_vel.0 = Vec3::ZERO;
        ang_vel.0 = Vec3::ZERO;
    }
}

fn respawn_if_fallen(
    mut query: Query<
        (
            &mut Position,
            &mut Rotation,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<LocalPlayer>,
    >,
    hm_res: Option<Res<crate::terrain::FinishedHeightMap>>,
) {
    let Ok((mut pos, mut rot, mut lin_vel, mut ang_vel)) = query.single_mut() else {
        return;
    };
    // Heightmap is torn down in-place whenever the room owner edits terrain
    // parameters mid-session (see `maybe_regenerate_terrain`). Skip this
    // tick; the chassis is a dynamic rigid body and will tumble harmlessly
    // until the new heightfield collider comes online.
    let Some(hm_res) = hm_res else {
        return;
    };
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    // Use the local ground altitude under the rover as the fall threshold.
    // The owner controls `height_scale` and erosion depth, so a map can
    // legitimately sit far below any fixed world-Y threshold; a purely
    // absolute check would then fire every tick, respawn the rover a metre
    // above the sunken ground, see `pos.y < threshold` again on the next
    // tick, and soft-lock the session.
    let hm_x = (pos.x + half).clamp(0.0, extent);
    let hm_z = (pos.z + half).clamp(0.0, extent);
    let local_ground = hm.get_height_at(hm_x, hm_z);
    if pos.y > local_ground - cfg::FALL_BELOW_GROUND {
        return;
    }
    let centre = extent * 0.5;
    let (ox, oz) = random_spawn_xz();
    let ground_y = hm.get_height_at(centre + ox, centre + oz);
    let surface_normal = hm.get_normal_at(centre + ox, centre + oz);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
    pos.0 = Vec3::new(ox, ground_y + cfg::SPAWN_HEIGHT_OFFSET, oz);
    rot.0 = tilt;
    lin_vel.0 = Vec3::ZERO;
    ang_vel.0 = Vec3::ZERO;
}
