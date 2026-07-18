//! Car preset — ground vehicle with raycast suspension + steering + handbrake.

use super::{LocomotionConfig, LocomotionPreset, clamp_half_extents, clamp_pos};
use crate::pds::types::{Fp, Fp3};
use serde::{Deserialize, Serialize};

/// Car preset: ground vehicle. 4-corner raycast suspension (same approach
/// as the hover-boat, no buoyancy), W/S throttle/reverse, A/D steer
/// (yaw torque only when grounded), Space handbrake (zero forward force +
/// extra lateral grip). Sinks in water.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CarParams {
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    pub suspension_rest_length: Fp,
    pub suspension_stiffness: Fp,
    pub suspension_damping: Fp,
    pub drive_force: Fp,
    pub turn_torque: Fp,
    pub lateral_grip: Fp,
    /// Extra lateral-grip multiplier applied when Space (handbrake) is held.
    pub handbrake_grip_factor: Fp,
    /// Tilt (degrees from upright) beyond which the uprighting assist
    /// engages (#804). Below it the assist stays silent so cornering lean
    /// and slope driving are never fought. Promoted from
    /// `CAR_UPRIGHT_ASSIST_COS` by #876 (60° ↔ cos 0.5); field-level
    /// serde default keeps pre-#876 records at the historical feel.
    #[serde(default = "default_upright_engage_tilt")]
    pub upright_engage_tilt_degrees: Fp,
    /// Mass-normalised righting acceleration (rad/s²-equivalent) applied
    /// past the engage tilt.
    #[serde(default = "default_upright_accel")]
    pub upright_assist_accel: Fp,
    /// Mass-normalised spin damping while righting, so the chassis
    /// settles level instead of oscillating.
    #[serde(default = "default_upright_damping")]
    pub upright_assist_damping: Fp,
    /// Centre-of-mass drop as a fraction of the chassis half-height,
    /// below the body origin (#804's anti-rollover lever). Applied when
    /// the chassis is (re)built, like the collider dimensions.
    #[serde(default = "default_center_of_mass_drop")]
    pub center_of_mass_drop: Fp,
}

/// Serde fallbacks for records published before #876 — the values the
/// uprighting/centre-of-mass code hard-coded (formerly the
/// `config::rover::CAR_UPRIGHT_*` constants). Shared with `Default` so an
/// old record and a fresh preset agree.
fn default_upright_engage_tilt() -> Fp {
    Fp(60.0)
}
fn default_upright_accel() -> Fp {
    Fp(2.5)
}
fn default_upright_damping() -> Fp {
    Fp(0.8)
}
fn default_center_of_mass_drop() -> Fp {
    Fp(0.6)
}

impl Default for CarParams {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            chassis_half_extents: Fp3([0.8, 0.4, 1.6]),
            mass: Fp(900.0),
            linear_damping: Fp(0.8),
            angular_damping: Fp(4.0),
            suspension_rest_length: Fp(0.6),
            // Stiffer than hover-boat: cars need quick response on terrain.
            suspension_stiffness: Fp(cfg::SUSPENSION_STIFFNESS * 4.0),
            suspension_damping: Fp(cfg::SUSPENSION_DAMPING * 2.5),
            drive_force: Fp(8_000.0),
            turn_torque: Fp(1_800.0),
            lateral_grip: Fp(20_000.0),
            handbrake_grip_factor: Fp(0.25),
            upright_engage_tilt_degrees: default_upright_engage_tilt(),
            upright_assist_accel: default_upright_accel(),
            upright_assist_damping: default_upright_damping(),
            center_of_mass_drop: default_center_of_mass_drop(),
        }
    }
}

impl LocomotionPreset for CarParams {
    const KIND_TAG: &'static str = "car";
    const DISPLAY_LABEL: &'static str = "Car";

    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 50_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.suspension_rest_length = clamp_pos(self.suspension_rest_length, 0.001, 5.0);
        self.suspension_stiffness = clamp_pos(self.suspension_stiffness, 0.0, 200_000.0);
        self.suspension_damping = clamp_pos(self.suspension_damping, 0.0, 20_000.0);
        self.drive_force = clamp_pos(self.drive_force, 0.0, 200_000.0);
        self.turn_torque = clamp_pos(self.turn_torque, 0.0, 50_000.0);
        self.lateral_grip = clamp_pos(self.lateral_grip, 0.0, 200_000.0);
        self.handbrake_grip_factor = clamp_pos(self.handbrake_grip_factor, 0.0, 100.0);
        // Floor of 15°: an assist that engages inside ordinary cornering
        // lean would fight normal driving every turn.
        self.upright_engage_tilt_degrees = clamp_pos(self.upright_engage_tilt_degrees, 15.0, 90.0);
        self.upright_assist_accel = clamp_pos(self.upright_assist_accel, 0.0, 50.0);
        self.upright_assist_damping = clamp_pos(self.upright_assist_damping, 0.0, 20.0);
        self.center_of_mass_drop = clamp_pos(self.center_of_mass_drop, 0.0, 1.0);
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Car(Box::new(self))
    }
}
