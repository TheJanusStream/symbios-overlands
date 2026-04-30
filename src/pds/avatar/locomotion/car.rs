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
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Car(Box::new(self))
    }
}
