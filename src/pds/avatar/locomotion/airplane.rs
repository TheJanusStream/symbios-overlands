//! Airplane preset — arcade flight model: continuous thrust, lift =
//! `lift_per_speed × forward airspeed`, drag along the velocity vector,
//! pitch / roll / yaw from input.

use super::{LocomotionConfig, LocomotionPreset, clamp_half_extents, clamp_pos};
use crate::pds::types::{Fp, Fp3};
use serde::{Deserialize, Serialize};

/// Airplane preset: arcade flight model. W/S pitch, A/D roll, Q/E yaw,
/// Space throttle up, Shift throttle down. Lift = `lift_per_speed` ×
/// forward airspeed (no AOA simulation); drag damps motion along the
/// negative-velocity direction. No take-off mechanic — the avatar is
/// always "airborne" and crashes on terrain contact like any other
/// physics body.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AirplaneParams {
    /// Fuselage cuboid collider half-extents (m).
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    /// Continuous forward thrust applied by the throttle input (N).
    pub thrust: Fp,
    /// Pitch input torque (N·m). Positive W input pitches nose down.
    pub pitch_torque: Fp,
    /// Roll input torque (N·m).
    pub roll_torque: Fp,
    /// Yaw / rudder input torque (N·m).
    pub yaw_torque: Fp,
    /// Lift force per (m/s) of forward airspeed (N·s/m). Multiplied by
    /// the chassis-local forward-velocity component each fixed step.
    pub lift_per_speed: Fp,
    /// Air-resistance coefficient applied along the velocity vector (N·s/m).
    pub drag_coefficient: Fp,
    /// Minimum forward airspeed (m/s) below which lift drops to zero —
    /// approximates a stall without simulating AOA.
    pub min_airspeed: Fp,
}

impl Default for AirplaneParams {
    fn default() -> Self {
        Self {
            chassis_half_extents: Fp3([0.6, 0.3, 1.5]),
            mass: Fp(40.0),
            linear_damping: Fp(0.05),
            angular_damping: Fp(2.0),
            thrust: Fp(1_500.0),
            pitch_torque: Fp(900.0),
            roll_torque: Fp(900.0),
            yaw_torque: Fp(450.0),
            lift_per_speed: Fp(45.0),
            drag_coefficient: Fp(0.6),
            min_airspeed: Fp(6.0),
        }
    }
}

impl LocomotionPreset for AirplaneParams {
    const KIND_TAG: &'static str = "airplane";
    const DISPLAY_LABEL: &'static str = "Airplane";

    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.thrust = clamp_pos(self.thrust, 0.0, 100_000.0);
        self.pitch_torque = clamp_pos(self.pitch_torque, 0.0, 50_000.0);
        self.roll_torque = clamp_pos(self.roll_torque, 0.0, 50_000.0);
        self.yaw_torque = clamp_pos(self.yaw_torque, 0.0, 50_000.0);
        self.lift_per_speed = clamp_pos(self.lift_per_speed, 0.0, 1_000.0);
        self.drag_coefficient = clamp_pos(self.drag_coefficient, 0.0, 100.0);
        self.min_airspeed = clamp_pos(self.min_airspeed, 0.0, 100.0);
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Airplane(Box::new(self))
    }
}
