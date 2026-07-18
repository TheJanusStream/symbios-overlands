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
//! The car has no buoyancy (it sinks in water). It resists rollover through
//! a low centre of mass (set in [`super::preset`]) and recovers from a flip
//! through a gated uprighting assist ([`apply_car_uprighting`]) that engages
//! only once the chassis is tipped well past any cornering lean, so a car can
//! never end up stuck on its roof (#804).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::{CarPreset, chassis_corners};

#[allow(clippy::type_complexity)]
pub(super) fn apply_car_suspension(
    live: Res<LiveAvatarRecord>,
    mut query: Query<(Entity, Forces, &GlobalTransform), (With<LocalPlayer>, With<CarPreset>)>,
    sensors: Query<Entity, With<Sensor>>,
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
    // Exclude self + every sensor so the suspension never rests on a gateway
    // veil / portal (#813) — see [`super::ground_ray_filter`].
    let filter = super::ground_ray_filter(chassis_entity, sensors.iter());
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
    // Invert the steer response when actually reversing (#723): with the
    // wheels held one way, a real car's heading rotates the opposite way in
    // reverse vs. forward, so a raw key→yaw mapping steers backwards while
    // backing up. Keyed on longitudinal (not raw) velocity so a sideways
    // slide doesn't flip it.
    let forward_speed = flat_forward.dot(lin_vel);
    let steer = local_up * p.turn_torque.0 * super::reverse_steer_sign(forward_speed);
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        forces.apply_torque(steer);
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        forces.apply_torque(-steer);
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

/// World-up restoring torque for the car's uprighting assist, or `None` when
/// the chassis is upright enough to leave alone. Pure so the engage threshold,
/// the mass-scaling, and the dead-inverted fallback can be unit-tested without
/// a physics world.
///
/// `up` / `right` are the chassis's world-space up and right axes, `ang_vel`
/// its angular velocity, and `p` the record's car tuning (#876 promoted the
/// engage tilt / accel / damping from constants). Returns `None` while the
/// tilt is inside `upright_engage_tilt_degrees` — i.e. within normal
/// cornering-lean / slope-driving range — so the assist never fights ordinary
/// driving. Past that tilt it returns a torque that rotates `up` toward
/// world-up (falling back to the roll axis when the chassis is dead-inverted,
/// so it can't perch on its roof), minus a spin-damping term so it settles
/// level rather than oscillating.
fn upright_assist_torque(
    up: Vec3,
    right: Vec3,
    ang_vel: Vec3,
    p: &crate::pds::CarParams,
) -> Option<Vec3> {
    if up.dot(Vec3::Y) >= p.upright_engage_tilt_degrees.0.to_radians().cos() {
        return None;
    }
    let mut axis = up.cross(Vec3::Y);
    if axis.length_squared() < cfg::CAR_UPRIGHT_DEGENERATE_SQ {
        axis = right;
    }
    let restoring = axis.normalize_or_zero() * (p.mass.0 * p.upright_assist_accel.0);
    let damping = -ang_vel * (p.mass.0 * p.upright_assist_damping.0);
    Some(restoring + damping)
}

/// Right a car that has tipped onto its side or roof. Runs every fixed step
/// (like the hover-boat's uprighting) but stays dormant until the chassis is
/// tilted past the record's engage tilt, so it leaves normal driving —
/// cornering lean, driving across slopes — untouched and only rescues a
/// genuine flip. Not input-gated: a flipped car keeps righting even while the
/// owner types in a chat field.
#[allow(clippy::type_complexity)]
pub(super) fn apply_car_uprighting(
    live: Res<LiveAvatarRecord>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<CarPreset>)>,
) {
    let LocomotionConfig::Car(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    let up = global_tf.up().as_vec3();
    let right = global_tf.right().as_vec3();
    let ang_vel = forces.angular_velocity();
    if let Some(torque) = upright_assist_torque(up, right, ang_vel, p) {
        forces.apply_torque(torque);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::CarParams;

    fn params() -> CarParams {
        CarParams::default()
    }

    #[test]
    fn upright_car_gets_no_assist() {
        // Dead level, and a modest cornering lean (30°) — both within the
        // assist's silent band, so it must return None (never fight driving).
        let p = params();
        assert!(upright_assist_torque(Vec3::Y, Vec3::X, Vec3::ZERO, &p).is_none());
        let leaned = Quat::from_rotation_z(30f32.to_radians()) * Vec3::Y;
        assert!(upright_assist_torque(leaned, Vec3::X, Vec3::ZERO, &p).is_none());
    }

    #[test]
    fn tipped_car_is_pushed_back_toward_upright() {
        // Rolled 100° about +Z (past the default 60° engage tilt). With no
        // spin the torque must point along `up × Y` — the axis whose rotation
        // lifts the up vector back toward world-up — and be mass-scaled.
        let p = params();
        let up = Quat::from_rotation_z(100f32.to_radians()) * Vec3::Y;
        let torque = upright_assist_torque(up, Vec3::X, Vec3::ZERO, &p)
            .expect("a car tipped past 60° must be assisted");
        let righting_axis = up.cross(Vec3::Y).normalize();
        assert!(
            torque.normalize().dot(righting_axis) > 0.99,
            "torque should roll the chassis back toward upright, got {torque:?}"
        );
        assert!(
            torque.length() >= p.mass.0 * p.upright_assist_accel.0 * 0.5,
            "righting torque should be mass-scaled and substantial"
        );
    }

    #[test]
    fn dead_inverted_car_falls_back_to_the_roll_axis() {
        // Exactly upside down: `up × Y` degenerates to ~zero, so without the
        // fallback the car would perch on its roof. The assist must instead
        // torque about the supplied roll (right) axis to tip it off.
        let p = params();
        let right = Vec3::X;
        let torque = upright_assist_torque(Vec3::NEG_Y, right, Vec3::ZERO, &p)
            .expect("an inverted car must be assisted");
        let along_roll = torque.dot(right);
        assert!(
            along_roll.abs() >= p.mass.0 * p.upright_assist_accel.0 * 0.5,
            "inverted assist should act about the roll axis, got {torque:?}"
        );
    }

    #[test]
    fn spin_is_damped_while_righting() {
        // A chassis tipped 100° about +Z rights by rotating in -Z. Give it a
        // spin already in that righting sense: the damping term must shrink the
        // net righting torque (magnitude along -Z) versus the static case, so
        // the car settles upright instead of overshooting past level.
        let p = params();
        let up = Quat::from_rotation_z(100f32.to_radians()) * Vec3::Y;
        let righting_axis = up.cross(Vec3::Y).normalize(); // ≈ -Z
        let still = upright_assist_torque(up, Vec3::X, Vec3::ZERO, &p).unwrap();
        let spinning = upright_assist_torque(up, Vec3::X, righting_axis * 5.0, &p).unwrap();
        assert!(
            spinning.dot(righting_axis) < still.dot(righting_axis),
            "a chassis already rotating upright should get less righting torque"
        );
    }

    #[test]
    fn widened_engage_tilt_keeps_the_assist_silent_longer() {
        // The promoted engage-tilt knob (#876): at 80° the default assist
        // engages but a record tuned to 85° stays silent.
        let tilted = Quat::from_rotation_z(80f32.to_radians()) * Vec3::Y;
        let p = params();
        assert!(upright_assist_torque(tilted, Vec3::X, Vec3::ZERO, &p).is_some());
        let mut wide = params();
        wide.upright_engage_tilt_degrees = crate::pds::Fp(85.0);
        assert!(upright_assist_torque(tilted, Vec3::X, Vec3::ZERO, &wide).is_none());
    }
}
