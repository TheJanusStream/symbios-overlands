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
//!
//! Aerodynamics (lift, drag) and the idle cruise thrust are PASSIVE —
//! they live in [`apply_airplane_aerodynamics`], which is not gated on
//! egui keyboard focus, so typing in a chat/search field leaves the
//! airplane cruising hands-off instead of cutting the engine and
//! stalling it out of the sky (#821). The key-reading system
//! [`apply_airplane_forces`] contributes only the throttle DELTA
//! (Space = +half, Shift = −half) on top of the passive cruise half,
//! reproducing the original Space=full / Shift=zero / idle=half table.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::{AirplanePreset, VisualsEditFreeze};

/// Input contribution to the throttle, relative to the record's
/// hands-off `cruise` fraction (#876 promoted it from a constant; the
/// passive side is applied by [`apply_airplane_aerodynamics`]).
/// `cruise + delta` reproduces the historical absolute table exactly at
/// the 0.5 default: Space = 1.0, Shift = 0.0, neither = 0.5, both = 0.5.
fn throttle_delta(space: bool, shift: bool, cruise: f32) -> f32 {
    match (space, shift) {
        (true, false) => cruise,
        (false, true) => -cruise,
        _ => 0.0,
    }
}

/// World-Y lift for a given forward airspeed, with the stall cutoff.
/// Pure so the stall boundary is unit-testable.
fn lift_force(airspeed: f32, min_airspeed: f32, lift_per_speed: f32) -> Vec3 {
    if airspeed >= min_airspeed {
        Vec3::Y * (lift_per_speed * airspeed)
    } else {
        Vec3::ZERO
    }
}

/// Passive flight model: idle cruise thrust, airspeed lift, and drag.
///
/// Runs regardless of egui keyboard focus so a focused text field never
/// stalls the airplane (#821). It DOES still park during [`TravelingTo`]:
/// unlike the helicopter's hover (pure anti-gravity), cruise thrust is
/// active propulsion — letting it run would fly the player away from the
/// portal while the destination fetch is in flight. Travel-time altitude
/// hold is the travel-overlay issue's job (#842). The avatar-editor
/// freeze (#814) parks the chassis wholesale, so frozen bodies are
/// filtered out rather than fought.
#[allow(clippy::type_complexity)]
pub(super) fn apply_airplane_aerodynamics(
    live: Res<LiveAvatarRecord>,
    mut query: Query<
        (Forces, &GlobalTransform),
        (
            With<LocalPlayer>,
            With<AirplanePreset>,
            Without<VisualsEditFreeze>,
        ),
    >,
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

    // Idle cruise: the airplane flies by default; input only modulates.
    forces.apply_force(forward * p.thrust.0 * p.cruise_throttle.0);

    // Aerodynamics.
    let lin_vel = forces.linear_velocity();
    let airspeed = forward.dot(lin_vel);
    // Lift acts along world-Y, scaled by forward airspeed. The
    // simplification (no AOA, no banked-turn redirection of lift)
    // keeps the model arcade-friendly: rolling does not bleed altitude
    // unless the player also pitches the nose up.
    forces.apply_force(lift_force(airspeed, p.min_airspeed.0, p.lift_per_speed.0));
    // Drag — opposes the velocity vector; the squared term is folded
    // into a single coefficient so authoring stays one knob.
    let speed = lin_vel.length();
    if speed > 0.0 {
        let drag_force = lin_vel * (-p.drag_coefficient.0 * speed);
        forces.apply_force(drag_force);
    }
}

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

    // Throttle: Space = full, Shift = zero, idle = half — expressed as a
    // delta over the passive cruise applied by
    // `apply_airplane_aerodynamics`, so the totals match the historical
    // table while the airplane keeps cruising when this system is gated
    // off (egui focus).
    let space = keyboard.pressed(KeyCode::Space);
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    forces.apply_force(forward * p.thrust.0 * throttle_delta(space, shift, p.cruise_throttle.0));

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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default record cruise fraction — the historical constant.
    fn cruise() -> f32 {
        crate::pds::AirplaneParams::default().cruise_throttle.0
    }

    #[test]
    fn cruise_plus_delta_reproduces_the_historical_throttle_table() {
        // (space, shift) -> absolute throttle before the #821 split.
        let table = [
            (true, false, 1.0),
            (false, true, 0.0),
            (false, false, 0.5),
            (true, true, 0.5),
        ];
        for (space, shift, expected) in table {
            assert_eq!(
                cruise() + throttle_delta(space, shift, cruise()),
                expected,
                "space={space} shift={shift}"
            );
        }
    }

    #[test]
    fn hands_off_throttle_is_exactly_the_cruise_fraction() {
        // With the input system gated off (egui focus), only the passive
        // cruise applies — the airplane must keep flying at idle power,
        // i.e. the input system contributes nothing at rest.
        assert_eq!(throttle_delta(false, false, cruise()), 0.0);
    }

    #[test]
    fn lift_cuts_to_zero_below_the_stall_speed() {
        let min = 8.0;
        let per_speed = 30.0;
        assert_eq!(lift_force(7.99, min, per_speed), Vec3::ZERO);
        assert_eq!(lift_force(8.0, min, per_speed), Vec3::Y * (30.0 * 8.0));
        assert_eq!(lift_force(12.0, min, per_speed), Vec3::Y * (30.0 * 12.0));
        // Reversing (negative airspeed) is below stall by definition.
        assert_eq!(lift_force(-5.0, min, per_speed), Vec3::ZERO);
    }
}
