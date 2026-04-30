//! Helicopter preset — auto-stabilising arcade hover with cyclic +
//! strafe + yaw input.

use super::{LocomotionConfig, LocomotionPreset, clamp_half_extents, clamp_pos};
use crate::pds::types::{Fp, Fp3};
use serde::{Deserialize, Serialize};

/// Helicopter preset: arcade hover model. The chassis auto-stabilises to
/// upright (no torque-induced spin), `hover_thrust` exactly cancels
/// gravity at idle, Space ascends, Shift descends, W/S apply a tilt-cyclic
/// horizontal force, A/D yaw, Q/E strafe.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HelicopterParams {
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    /// Vertical force that holds the helicopter at hover when no input is
    /// pressed (N). Set so `hover_thrust ≈ mass · 9.81` for equilibrium.
    pub hover_thrust: Fp,
    /// Vertical climb / descend speed when Space / Shift is held (m/s).
    /// Drives a target-velocity lerp rather than a raw force so input
    /// feels responsive without overshooting.
    pub vertical_speed: Fp,
    /// Forward / backward (cyclic) horizontal force on W/S (N).
    pub cyclic_force: Fp,
    /// Lateral strafe force on Q/E (N).
    pub strafe_force: Fp,
    /// Yaw input torque on A/D (N·m).
    pub yaw_torque: Fp,
}

impl Default for HelicopterParams {
    fn default() -> Self {
        // hover_thrust default ≈ mass · 9.81 so a fresh helicopter floats
        // at idle without sinking or climbing.
        Self {
            chassis_half_extents: Fp3([0.7, 0.6, 1.4]),
            mass: Fp(60.0),
            linear_damping: Fp(0.8),
            angular_damping: Fp(4.0),
            hover_thrust: Fp(60.0 * 9.81),
            vertical_speed: Fp(6.0),
            cyclic_force: Fp(900.0),
            strafe_force: Fp(800.0),
            yaw_torque: Fp(400.0),
        }
    }
}

impl LocomotionPreset for HelicopterParams {
    const KIND_TAG: &'static str = "helicopter";
    const DISPLAY_LABEL: &'static str = "Helicopter";

    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.hover_thrust = clamp_pos(self.hover_thrust, 0.0, 200_000.0);
        self.vertical_speed = clamp_pos(self.vertical_speed, 0.0, 50.0);
        self.cyclic_force = clamp_pos(self.cyclic_force, 0.0, 50_000.0);
        self.strafe_force = clamp_pos(self.strafe_force, 0.0, 50_000.0);
        self.yaw_torque = clamp_pos(self.yaw_torque, 0.0, 50_000.0);
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Helicopter(Box::new(self))
    }
}
