//! Helicopter preset — auto-stabilising arcade hover.
//!
//! Controls (chassis-local):
//!   * **W / S** — forward / backward cyclic horizontal force.
//!   * **A / D** — yaw torque.
//!   * **Q / E** — lateral strafe force.
//!   * **Space** — climb at `vertical_speed`.
//!   * **Shift** — descend at `vertical_speed`.
//!
//! `hover_thrust` cancels gravity at idle so the helicopter floats
//! without sinking. The chassis auto-stabilises to upright via a Y-axis-
//! aligning torque so the player never has to fight rotor-induced spin.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::HelicopterPreset;

/// Strength (N·m) of the auto-stabilising torque that pulls the
/// helicopter's chassis-up axis toward world-up. High enough that the
/// player never inverts but low enough that visual tilt from cyclic
/// input still reads as motion. Hard-coded because it is a behavioural
/// constant of the preset rather than an authoring knob.
const AUTO_STABILIZE_TORQUE: f32 = 800.0;

#[allow(clippy::type_complexity)]
pub(super) fn apply_helicopter_forces(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HelicopterPreset>)>,
    traveling: Option<Res<TravelingTo>>,
) {
    if traveling.is_some() {
        return;
    }
    let LocomotionConfig::Helicopter(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    // Hover thrust along world-Y so the helicopter floats independent
    // of cyclic pitch — players can tilt for forward speed without
    // bleeding altitude.
    forces.apply_force(Vec3::Y * p.hover_thrust.0);

    // Vertical climb / descend lerps the Y component of velocity toward
    // ±vertical_speed when Space / Shift is held; idle leaves vertical
    // motion to the gravity / hover_thrust equilibrium.
    let lin_vel = forces.linear_velocity();
    let want_climb = keyboard.pressed(KeyCode::Space);
    let want_descend =
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let target_vy = match (want_climb, want_descend) {
        (true, false) => p.vertical_speed.0,
        (false, true) => -p.vertical_speed.0,
        _ => lin_vel.y,
    };
    // Approach the target Y velocity at a fixed rate so input feels
    // immediate without overshooting. The 8.0 here matches the cyclic
    // feel — tunable, but again a behavioural constant.
    let dy = (target_vy - lin_vel.y).clamp(-15.0, 15.0);
    forces.apply_force(Vec3::Y * dy * p.mass.0 * 4.0);

    // Cyclic forward/back — uses the chassis-flat forward so a tilted
    // helicopter still moves "where the player is looking".
    let forward = global_tf.forward().as_vec3();
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        forces.apply_force(flat_forward * p.cyclic_force.0);
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        forces.apply_force(-flat_forward * p.cyclic_force.0);
    }

    // Strafe — chassis-flat right.
    let right = global_tf.right().as_vec3();
    let flat_right = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();
    if keyboard.pressed(KeyCode::KeyQ) {
        forces.apply_force(-flat_right * p.strafe_force.0);
    }
    if keyboard.pressed(KeyCode::KeyE) {
        forces.apply_force(flat_right * p.strafe_force.0);
    }

    // Yaw on A/D.
    let local_up = global_tf.up().as_vec3();
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        forces.apply_torque(local_up * p.yaw_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        forces.apply_torque(-local_up * p.yaw_torque.0);
    }

    // Auto-stabilise: torque the chassis-up axis toward world-up so
    // the player never inverts. The cross product gives a torque
    // perpendicular to both, which is the rotation-axis of the
    // shortest-path correction.
    let chassis_up = global_tf.up().as_vec3();
    forces.apply_torque(chassis_up.cross(Vec3::Y) * AUTO_STABILIZE_TORQUE);
}
