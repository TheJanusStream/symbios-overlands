//! Airplane preset — arcade flight with continuous thrust + control
//! surfaces.
//!
//! Controls (chassis-local):
//!   * **W / S** — pitch input torque (`pitch_torque`). W pitches nose down,
//!     S pitches nose up.
//!   * **A / D** — roll input torque (`roll_torque`).
//!   * **Q / E** — yaw / rudder torque (`yaw_torque`).
//!   * **Space** — throttle up: full forward thrust.
//!   * **Shift** — throttle down: zero thrust. Without input, thrust is
//!     half — the airplane cruises by default and the modifier keys steer
//!     speed.
//!
//! Lift = `lift_per_speed` × forward airspeed, applied along world-Y.
//! Below `min_airspeed`, lift drops to zero — approximates a stall
//! without simulating AOA. Drag damps motion along the negative-velocity
//! direction. No take-off mechanic; the avatar is effectively always
//! airborne and crashes on terrain contact like any other physics body.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::AirplanePreset;

#[allow(clippy::type_complexity)]
pub(super) fn apply_airplane_forces(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<AirplanePreset>)>,
    traveling: Option<Res<TravelingTo>>,
) {
    if traveling.is_some() {
        return;
    }
    let LocomotionConfig::Airplane(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let forward = global_tf.forward().as_vec3();
    let right = global_tf.right().as_vec3();

    // Throttle: Space = full, Shift = zero, idle = half. Holding both
    // cancels out at half by the same logic that resolves "neither
    // pressed". This gives the player a default cruise without requiring
    // permanent Space-mashing.
    let space = keyboard.pressed(KeyCode::Space);
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let throttle = match (space, shift) {
        (true, false) => 1.0,
        (false, true) => 0.0,
        _ => 0.5,
    };
    forces.apply_force(forward * p.thrust.0 * throttle);

    // Pitch: W tips the nose down (positive forward → +X local axis
    // rotation). The airplane's local right vector is the rotation axis
    // for pitch.
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        forces.apply_torque(-right * p.pitch_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        forces.apply_torque(right * p.pitch_torque.0);
    }
    // Roll: A rolls left (positive Z-forward axis rotation in chassis
    // local space).
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        forces.apply_torque(forward * p.roll_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        forces.apply_torque(-forward * p.roll_torque.0);
    }
    // Yaw / rudder.
    let local_up = global_tf.up().as_vec3();
    if keyboard.pressed(KeyCode::KeyQ) {
        forces.apply_torque(local_up * p.yaw_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyE) {
        forces.apply_torque(-local_up * p.yaw_torque.0);
    }

    // Aerodynamics.
    let lin_vel = forces.linear_velocity();
    let airspeed = forward.dot(lin_vel);
    if airspeed >= p.min_airspeed.0 {
        // Lift acts along world-Y, scaled by forward airspeed. The
        // simplification (no AOA, no banked-turn redirection of lift)
        // keeps the model arcade-friendly: rolling does not bleed altitude
        // unless the player also pitches the nose up.
        forces.apply_force(Vec3::Y * (p.lift_per_speed.0 * airspeed));
    }
    // Drag — opposes the velocity vector; the squared term is folded
    // into a single coefficient so authoring stays one knob.
    let speed = lin_vel.length();
    if speed > 0.0 {
        let drag_force = lin_vel * (-p.drag_coefficient.0 * speed);
        forces.apply_force(drag_force);
    }
}
