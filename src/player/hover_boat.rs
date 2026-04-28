//! HoverBoat preset — the 4-corner-suspension cuboid chassis with WASD
//! drive + buoyancy. Parameters live on [`HoverBoatParams`] and are read
//! every fixed step so slider edits take effect without a rebuild.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::{HoverBoatPreset, chassis_corners};

/// Push tuning changes (mass, damping) from the live record onto the
/// chassis components every fixed step, so slider tweaks take effect
/// immediately without requiring a locomotion rebuild.
#[allow(clippy::type_complexity)]
pub(super) fn sync_hover_boat_physics(
    live: Res<LiveAvatarRecord>,
    mut query: Query<
        (&mut Mass, &mut LinearDamping, &mut AngularDamping),
        (With<LocalPlayer>, With<HoverBoatPreset>),
    >,
) {
    let Ok((mut mass, mut lin_damp, mut ang_damp)) = query.single_mut() else {
        return;
    };
    let LocomotionConfig::HoverBoat(p) = &live.0.locomotion else {
        return;
    };
    if mass.0 != p.mass.0 {
        mass.0 = p.mass.0;
    }
    if lin_damp.0 != p.linear_damping.0 {
        lin_damp.0 = p.linear_damping.0;
    }
    if ang_damp.0 != p.angular_damping.0 {
        ang_damp.0 = p.angular_damping.0;
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_hover_boat_suspension(
    live: Res<LiveAvatarRecord>,
    mut query: Query<
        (Entity, Forces, &GlobalTransform),
        (With<LocalPlayer>, With<HoverBoatPreset>),
    >,
    spatial_query: SpatialQuery,
) {
    let LocomotionConfig::HoverBoat(p) = &live.0.locomotion else {
        return;
    };
    let Ok((chassis_entity, mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let half_extents = Vec3::from_array(p.chassis_half_extents.0);
    let corners = chassis_corners(half_extents);
    let ray_max = p.suspension_rest_length.0 + 1.5;
    let chassis_tf = global_tf.compute_transform();
    let filter = SpatialQueryFilter::default().with_excluded_entities([chassis_entity]);
    let lin_vel = forces.linear_velocity();
    let ang_vel = forces.angular_velocity();
    let center_of_mass = global_tf.translation();

    for local_offset in corners {
        let world_origin = chassis_tf.transform_point(local_offset);
        let Some(hit) = spatial_query.cast_ray(world_origin, Dir3::NEG_Y, ray_max, true, &filter)
        else {
            continue;
        };

        let compression = p.suspension_rest_length.0 - hit.distance;
        if compression > 0.0 {
            let r = world_origin - center_of_mass;
            let point_vel = lin_vel + ang_vel.cross(r);
            let closing_speed = -point_vel.dot(hit.normal);
            let spring_force = p.suspension_stiffness.0 * compression;
            let damping_force = p.suspension_damping.0 * closing_speed;
            let total_force = (spring_force + damping_force).max(0.0);
            forces.apply_force_at_point(Vec3::Y * total_force, world_origin);
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_hover_boat_drive(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HoverBoatPreset>)>,
    traveling: Option<Res<TravelingTo>>,
) {
    if traveling.is_some() {
        return;
    }
    let LocomotionConfig::HoverBoat(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let lin_vel = forces.linear_velocity();
    let forward = global_tf.forward().as_vec3();
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let local_up = global_tf.up().as_vec3();
    let right = global_tf.right().as_vec3();

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        forces.apply_force(flat_forward * p.drive_force.0);
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        forces.apply_force(-flat_forward * p.drive_force.0);
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        forces.apply_torque(local_up * p.turn_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        forces.apply_torque(-local_up * p.turn_torque.0);
    }

    let lateral_vel = right.dot(lin_vel);
    forces.apply_force(-right * lateral_vel * p.lateral_grip.0);

    if keyboard.pressed(KeyCode::Space) {
        forces.apply_force(Vec3::Y * p.jump_force.0);
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_hover_boat_uprighting(
    live: Res<LiveAvatarRecord>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HoverBoatPreset>)>,
) {
    let LocomotionConfig::HoverBoat(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    let vehicle_up = global_tf.up().as_vec3();
    forces.apply_torque(vehicle_up.cross(Vec3::Y) * p.uprighting_torque.0);
}

/// Sample the runtime water-surface registry at each chassis corner and
/// apply per-corner buoyancy + flow current. Mirror of the legacy hover-
/// rover system, retargeted to read params from [`HoverBoatParams`] and
/// to derive corner offsets from the preset's authored chassis size.
#[allow(clippy::type_complexity)]
pub(super) fn apply_hover_boat_buoyancy(
    live: Res<LiveAvatarRecord>,
    water_surfaces: Res<crate::water::WaterSurfaces>,
    hm_res: Option<Res<crate::terrain::FinishedHeightMap>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HoverBoatPreset>)>,
) {
    let LocomotionConfig::HoverBoat(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    if let Some(hm_res) = hm_res.as_deref() {
        let hm = &hm_res.0;
        let half = (hm.width() - 1) as f32 * hm.scale() * 0.5;
        let pos = global_tf.translation();
        if pos.x.abs() > half || pos.z.abs() > half {
            return;
        }
    }

    let half_extents = Vec3::from_array(p.chassis_half_extents.0);
    let corners = chassis_corners(half_extents);
    let chassis_tf = global_tf.compute_transform();
    let lin_vel = forces.linear_velocity();
    let ang_vel = forces.angular_velocity();
    let center_of_mass = global_tf.translation();
    let buoyancy_scale = 1.0 / corners.len() as f32;

    for local_offset in corners {
        let world_origin = chassis_tf.transform_point(local_offset);

        // `water_rest_length` shifts the buoyancy plane upward relative to
        // the visible surface so a partially-submerged chassis sits with
        // some hull above water. The signed-distance query returns depth
        // below the visible surface; we add `water_rest_length` so depth
        // is taken against the buoyancy plane along the same normal.
        let Some(q) = water_surfaces.query(world_origin) else {
            continue;
        };
        let depth = (q.depth + p.water_rest_length.0).clamp(0.0, p.buoyancy_max_depth.0);
        if depth <= 0.0 {
            continue;
        }
        let r = world_origin - center_of_mass;
        let point_vel = lin_vel + ang_vel.cross(r);
        // Drag opposes the velocity component along the surface normal —
        // movement *parallel* to the surface should not be damped by
        // buoyancy itself (water's lateral resistance is captured by the
        // body's `linear_damping`).
        let normal_vel = point_vel.dot(q.normal);
        let lift = p.buoyancy_strength.0 * depth;
        let drag = -p.buoyancy_damping.0 * normal_vel;
        forces.apply_force_at_point(q.normal * ((lift + drag) * buoyancy_scale), world_origin);

        // Flow current — projected gravity tangent to the surface, scaled by
        // submerged depth so a corner barely under water feels less push
        // than one fully immersed.
        if q.flow_strength > 0.0 && q.flow_dir != Vec3::ZERO {
            let flow_force = q.flow_dir * q.flow_strength * depth;
            forces.apply_force_at_point(flow_force, world_origin);
        }
    }
}
