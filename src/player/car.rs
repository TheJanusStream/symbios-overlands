//! Car preset — ground vehicle with 4-corner raycast suspension, no
//! buoyancy.
//!
//! Controls:
//!   * **W / S** — forward / reverse drive force.
//!   * **A / D** — yaw torque (steer). Applied unconditionally; on a
//!     fully airborne chassis the yaw still works but feels light, which
//!     is fine for arcade play.
//!   * **Space** — handbrake: cuts forward force and multiplies lateral
//!     grip by `handbrake_grip_factor` (typically <1 → letting the rear
//!     slip out).
//!
//! The car has no uprighting torque and no buoyancy. If it tips over,
//! it stays tipped until the fall-respawn system catches it.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::{CarPreset, chassis_corners};

#[allow(clippy::type_complexity)]
pub(super) fn apply_car_suspension(
    live: Res<LiveAvatarRecord>,
    mut query: Query<(Entity, Forces, &GlobalTransform), (With<LocalPlayer>, With<CarPreset>)>,
    spatial_query: SpatialQuery,
) {
    let LocomotionConfig::Car(p) = &live.0.locomotion else {
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
pub(super) fn apply_car_drive(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<CarPreset>)>,
    traveling: Option<Res<TravelingTo>>,
) {
    if traveling.is_some() {
        return;
    }
    let LocomotionConfig::Car(p) = &live.0.locomotion else {
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

    let handbrake = keyboard.pressed(KeyCode::Space);

    if !handbrake {
        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            forces.apply_force(flat_forward * p.drive_force.0);
        }
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            forces.apply_force(-flat_forward * p.drive_force.0);
        }
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        forces.apply_torque(local_up * p.turn_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        forces.apply_torque(-local_up * p.turn_torque.0);
    }

    // Lateral grip — strong by default to keep the car planted; reduced
    // by `handbrake_grip_factor` when Space is held so the rear breaks
    // loose for arcade-style drifts.
    let grip = if handbrake {
        p.lateral_grip.0 * p.handbrake_grip_factor.0
    } else {
        p.lateral_grip.0
    };
    let lateral_vel = right.dot(lin_vel);
    forces.apply_force(-right * lateral_vel * grip);
}
